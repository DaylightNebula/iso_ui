use anarchy::{Res, ResMut, macros::{Getters, Resource, system}};
use cell::{App, Frame, Graphics, Plugin};
use magician_vgpu::{Buffer, LoadOp, MutableBuffer, PassAttachment, PassTarget, Pipeline, ShaderSource, ShaderType, StoreOp};
use mutual::CowData;

use crate::shader::{SDFMode, SDFRawBezier, SDFRawGlyph, SDFRawMetadata, SDFRawRectangle, SDFRawShaderData, SDFRawShape, SDFRawStyle};

pub mod shader;

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
    pub shapes_buffer: MutableBuffer<[SDFRawShape; 1000]>,
    pub styles_buffer: MutableBuffer<[SDFRawStyle; 1000]>,
    pub rectangles_buffer: MutableBuffer<[SDFRawRectangle; 1000]>,
    pub bezier_buffer: MutableBuffer<[SDFRawBezier; 1000]>,
    pub glyphs_buffer: MutableBuffer<[SDFRawGlyph; 1000]>
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
    let shapes_buffer = MutableBuffer::new(
        &*graphics, 
        &[SDFRawShape { 
            center: (1.0.into(), 1.0.into()), 
            dimensions: (1.0.into(), 1.0.into()), 
            shape_ty: 0, 
            looks_ptrs: std::u32::MAX, 
            next_ptrs: std::u32::MAX, 
            _pad0: 0 
        }; 1000], 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
    );

    // create styles buffer
    let styles_buffer = MutableBuffer::new(
        &*graphics, 
        &[SDFRawStyle { 
            primary_color: (1.0.into(), 1.0.into(), 1.0.into(), 1.0.into()), 
            border_color: (1.0.into(), 1.0.into(), 1.0.into(), 1.0.into()), 
            border_width: 1.0.into(), 
            texture_ptr: std::u32::MAX, 
            _padding: (0, 0) 
        }; 1000], 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
    );

    // create rectangles buffer
    let rectangles_buffer = MutableBuffer::new(
        &*graphics,
        &[SDFRawRectangle {
            radii: (1.0.into(), 1.0.into(), 1.0.into(), 1.0.into())
        }; 1000],
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
    );

    // create bezier's buffer
    let bezier_buffer = MutableBuffer::new(
        &*graphics,
        &[SDFRawBezier {
            a_off: (1.0.into(), 1.0.into()),
            b_off: (1.0.into(), 1.0.into()),
            c_off: (1.0.into(), 1.0.into()),
            thickness: 1.0.into(),
            _pad0: 0
        }; 1000],
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
    );

    // create glyphs buffer
    let glyphs_buffer = MutableBuffer::new(
        &*graphics, 
        &[SDFRawGlyph {
            start_idx: 0,
            length: 0,
            _pad0: 0,
            _pad1: 0
        }; 1000], 
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
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
