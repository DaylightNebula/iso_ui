use anarchy::{Res, ResMut, macros::{Resource, system}};
use cell::{App, Frame, Graphics, Plugin};
use magician_vgpu::{LoadOp, PassAttachment, PassTarget, Pipeline, ShaderSource, ShaderType, StoreOp};
use mutual::CowData;

pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(UIRenderResources::default())
            .on_render_update(ui_render_pass)
    }
}

#[derive(Resource, Default)]
pub struct UIRenderResources {
    pub pipeline: CowData<Pipeline>
}

#[system(std::i32::MAX / 2)]
fn ui_render_pass(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>,
    resources: Res<UIRenderResources>
) {
    if resources.pipeline.is_null() {
        resources.pipeline.set(
            Pipeline::builder("UI Pipeline")
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
                        source: include_str!("../shaders/test_fs.wgsl").into(),
                        main_function: "fs_final".into()
                    }
                )
                .build(&*graphics)
        );
    }

    let mut pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store
            }
        ], None
    );

    pass.use_pipeline(resources.pipeline.get_ref());
    pass.pass_mut().draw(0..3, 0..1);
}
