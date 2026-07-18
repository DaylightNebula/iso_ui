#![feature(linked_list_cursors)]

use anarchy::{Query, Res, ResMut, macros::{Getters, Resource, system}};
use cell::{App, Frame, Graphics, Plugin, WindowDimensions};
use magician_vgpu::{Buffer, LoadOp, MutableBuffer, PassAttachment, PassTarget, Pipeline, ShaderSource, ShaderType, StoreOp, glam::Vec2};
use mutual::CowData;
use vault::TextureVault;

use crate::{shader::{SDFRawBezier, SDFRawGlyph, SDFRawMetadata, SDFRawRectangle, SDFRawShaderData, SDFRawShape, SDFRawStyle}};

pub mod buffers;
pub mod data;
pub mod fonts;
pub mod nodes;
pub mod shader;

pub use buffers::*;
pub use data::*;
pub use nodes::*;
pub use fonts::*;

/// ECS plugin that registers GPU resources and a render pass for 2D SDF UI.
pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(TextureVault::default())
            .on_render_startup(init_resources)
            .on_render_update(ui_render_pass)
    }
}

/// GPU buffers, bind group, and pipeline used by the UI render pass.
///
/// `pipeline` is the SDF render pipeline, `bind_group` wires all uniform buffers
/// to that pipeline, `metadata_buffer` holds per-frame screen size/time/mode,
/// `shapes_buffer` stores the flattened element tree, and the remaining chunked
/// buffers hold deduplicated styles, rectangle radii, bezier curves, and glyph
/// headers referenced by shapes.
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

/// Allocates UI GPU buffers, bind group, and pipeline on render startup.
#[system(std::i32::MIN)]
fn init_resources(
    graphics: Res<Graphics>,
    vault: Res<TextureVault>
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
        .layout_raw::<TextureVault>(1, vault.bind_group_layout(&*graphics))
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

/// Flattens the UI tree, uploads it, and draws a fullscreen SDF pass over the frame.
#[system(std::i32::MAX / 2)]
fn ui_render_pass(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>,
    resources: Res<UIRenderResources>,
    window_dimensions: Res<WindowDimensions>,
    texture_vault: Res<TextureVault>
) {
    // create UI elements
    let nodes = Query::<&UINodeSDFRoot>::new(world.database())
        .as_iter()
        .map(|a| a.clone())
        .collect::<Vec<_>>();
    let elements = layout_ui_nodes(&nodes, [window_dimensions.x as f32, window_dimensions.y as f32]);

    // create root element
    let root = SDFElement { 
        center: Vec2::new(window_dimensions.x as f32 / 2.0, window_dimensions.y as f32 / 2.0), 
        dimensions: Vec2::new(window_dimensions.x as f32, window_dimensions.y as f32), 
        children: elements, 
        ..Default::default() 
    };

    // upload new UI tree
    resources.shapes_buffer.update(&*graphics, &root, &(&**resources, &**texture_vault))?;

    // setup render pass
    let mut pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store
            }
        ], None
    );

    // draw to the screen
    pass.use_pipeline(resources.pipeline().get_ref());
    pass.bind_raw(0, resources.bind_group());
    texture_vault.bind(&*graphics, &mut pass, 1);
    pass.pass_mut().draw(0..3, 0..1);
}
