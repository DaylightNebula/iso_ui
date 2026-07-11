#![feature(linked_list_cursors)]

use std::collections::LinkedList;

use anarchy::{Res, ResMut, macros::{Getters, Resource, system}};
use cell::{App, Frame, Graphics, Plugin};
use magician_vgpu::{Buffer, LoadOp, MutableBuffer, PassAttachment, PassTarget, Pipeline, ShaderSource, ShaderType, StoreOp, glam::{Vec2, Vec4}};
use mutual::CowData;

use crate::{shader::{SDFRawBezier, SDFRawGlyph, SDFRawMetadata, SDFRawRectangle, SDFRawShaderData, SDFRawShape, SDFRawStyle}};

pub mod chunked;
pub mod data;
pub mod shader;
pub mod tree;

pub use chunked::*;
pub use data::*;
pub use tree::*;

pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(self, app: App) -> App {
        app.on_render_startup(init_resources)
            .on_render_update(ui_render_pass)
    }
}

#[derive(Resource, Getters)]
pub struct UIRenderResources {
    pub pipeline: CowData<Pipeline>,
    pub bind_group: wgpu::BindGroup,
    pub metadata_buffer: MutableBuffer<SDFRawMetadata>,
    pub shapes_buffer: TreeBuffer<SDFRawShape>,
    pub styles_buffer: ChunkedBuffer<SDFRawStyle>,
    pub rectangles_buffer: ChunkedBuffer<SDFRawRectangle>,
    pub bezier_buffer: ChunkedBuffer<SDFRawBezier>,
    pub glyphs_buffer: ChunkedBuffer<SDFRawGlyph>
}

#[system(std::i32::MIN)]
fn init_resources(
    graphics: Res<Graphics>
) {
    // create metadata buffer
    let metadata_buffer = MutableBuffer::new(
        &*graphics, 
        &SDFRawMetadata { 
            screen_dimensions: (1.0.into(), 1.0.into()), 
            time: 1.0.into(), 
            mode: SDFMode::Normal as u32
        }, 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
    );

    // create shapes buffer
    let shapes_buffer = TreeBuffer::new(
        &*graphics,  
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, 
        1000
    );

    // create styles buffer
    let styles_buffer = ChunkedBuffer::new(
        &*graphics, 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, 
        1000
    );

    // create rectangles buffer
    let rectangles_buffer = ChunkedBuffer::new(
        &*graphics, 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, 
        1000
    );

    // create bezier's buffer
    let bezier_buffer = ChunkedBuffer::new(
        &*graphics, 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, 
        1000
    );

    // create glyphs buffer
    let glyphs_buffer = ChunkedBuffer::new(
        &*graphics, 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, 
        1000
    );

    // create bind group layout
    let bind_group_layout = graphics.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform, 
                        has_dynamic_offset: false, 
                        min_binding_size: None 
                    },
                    count: None,
                }
            ],
            label: Some("SDF UI BGL")
        });

    // create bind group
    let bind_group = graphics.device().create_bind_group(
        &wgpu::BindGroupDescriptor {
            label: Some("SDF UI BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: metadata_buffer.buffer().as_entire_binding()
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: shapes_buffer.buffer().as_entire_binding()
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: styles_buffer.buffer().as_entire_binding()
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: rectangles_buffer.buffer().as_entire_binding()
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bezier_buffer.buffer().as_entire_binding()
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: glyphs_buffer.buffer().as_entire_binding()
                }
            ]
        }
    );

    // create pipeline
    let pipeline = Pipeline::builder("UI Pipeline")
        .source(
            ShaderType::Vertex, 
            ShaderSource {
                source: include_str!("../shaders/no_vertex_screen.wgsl").into(),
                main_function: "vs_final".into()
            }
        )
        .source(
            ShaderType::Fragment, 
            ShaderSource {
                source: include_str!("../shaders/main.wgsl").into(),
                main_function: "fs_final".into()
            }
        )
        .layout_raw::<SDFRawShaderData>(0, bind_group_layout)
        .build(&*graphics);

    world.insert_resource(UIRenderResources {
        pipeline:CowData::new(pipeline), 
        bind_group, 
        metadata_buffer, 
        shapes_buffer, 
        styles_buffer, 
        rectangles_buffer, 
        bezier_buffer, 
        glyphs_buffer
    });
}

#[system(std::i32::MAX / 2)]
fn ui_render_pass(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>,
    resources: Res<UIRenderResources>
) {
    resources.shapes_buffer.update(&*graphics, &test_tree(), &resources)?;

    let mut pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store
            }
        ], None
    );

    pass.use_pipeline(resources.pipeline().get_ref());
    pass.bind_raw(0, resources.bind_group());
    pass.pass_mut().draw(0..3, 0..1);
}

fn test_tree() -> SDFElement {
    let mut children = LinkedList::new();
    children.push_back(SDFElement {
        center: Vec2::new(400.0, 250.0), 
        dimensions: Vec2::new(150.0, 150.0),
        style: SDFStyle { 
            primary_color: Vec4::new(0.0, 1.0, 0.0, 1.0), 
            border_color: Vec4::ZERO, 
            border_width: 0.0
        }, 
        shape: SDFShape::Bezier(SDFCurve { 
            a_offset: Vec2::new(-50.0, 0.0), 
            b_offset: Vec2::new(0.0, 50.0), 
            c_offset: Vec2::new(50.0, 0.0), 
            thickness: 3.0 
        }), 
        children: LinkedList::new(),
        ..Default::default()
    });

    SDFElement { 
        center: Vec2::new(400.0, 300.0), 
        dimensions: Vec2::new(200.0, 200.0),
        style: SDFStyle { 
            primary_color: Vec4::new(1.0, 0.0, 0.0, 1.0), 
            border_color: Vec4::new(0.0, 0.0, 0.0, 1.0), 
            border_width: 3.0
        }, 
        shape: SDFShape::Rectangle(SDFRectangle { radii: Vec4::new(15.0, 15.0, 15.0, 15.0) }), 
        // shape: SDFShape::Empty,
        children: children,
        ..Default::default()
    }
}
