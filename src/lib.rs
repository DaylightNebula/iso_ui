use anarchy::{Res, ResMut, macros::system};
use cell::{App, Frame, Graphics, Plugin};
use magician_vgpu::{LoadOp, PassAttachment, PassTarget, StoreOp};

pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(self, app: App) -> App {
        app.on_render_update(ui_render_pass)
    }
}

#[system(std::i32::MAX / 2)]
fn ui_render_pass(
    graphics: Res<Graphics>,
    frame: ResMut<Frame>
) {
    let pass = frame.init_pass(
        &[
            PassAttachment {
                target: PassTarget::PassOutput,
                load_op: LoadOp::Load,
                store_op: StoreOp::Store
            }
        ], None
    );
}
