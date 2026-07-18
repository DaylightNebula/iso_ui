use std::sync::Arc;

use anarchy::{EntityBuilder, Query, Res, WorldDatabase, macros::system};
use cell::{App, Graphics};
use gearbox::{BasicMaterial, BasicMesh, Camera, GearboxRenderPlugin, MaterialRef, MeshRef, Transform, shaders::basic_vertex};
use magician_vgpu::{glam::{self, Quat, Vec4}, rust::{Vec2, Vec3}};
use iso_ui::*;
use vault::{AssetVault, TextureVault};

fn main() -> anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .on_render_startup(setup)
        .on_update(update)
        .run()
}

#[system]
fn setup(
    graphics: Res<Graphics>,
    vault: Res<TextureVault>
) {
    let test_texture = vault.load(vault::AssetContent::Binary(Box::new(*include_bytes!("cobblestone.png"))))?;

    let vertices: [basic_vertex::VertexInput; 3] = [
        basic_vertex::VertexInput { position: Vec3::new(0.0,  0.5, 0.0), uvs: Vec2::new(0.5, 0.0) },
        basic_vertex::VertexInput { position: Vec3::new(-0.5,  -0.5, 0.0), uvs: Vec2::new(0.0, 1.0) },
        basic_vertex::VertexInput { position: Vec3::new(0.5,  -0.5, 0.0), uvs: Vec2::new(1.0, 1.0) }
    ];

    let mesh = BasicMesh::new(
        &*graphics, 
        &vertices, 
        &[0, 1, 2]
    );

    world.insert(
        EntityBuilder::default()
            .add(Transform::identity())
            .add(MaterialRef::new(BasicMaterial::new(glam::Vec4::new(0.1, 0.8, 0.2, 1.0))))
            .add(MeshRef::new(mesh))
            .build()
    );

    world.insert(
        EntityBuilder::default()
            .add(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .add(Camera::default())
            .build()  
    );

    let font_bytes = include_bytes!("./LiberationSans-Regular.ttf");
    let font = Arc::new(SDFFont::new(font_bytes)?);

    let mut root = UINode::new("root".to_string());
    root.set_position_type(PositionType::Absolute(Rect::new_bottom_right(Val::Px(20.0), Val::Px(20.0))));
    root.set_display(Display::FlexColumn { vertical: Align::End, horizontal: Align::End });
    root.set_width(Val::PercentWidth(0.5));
    root.set_height(Val::PercentHeight(0.5));
    root.set_background(Background::Color(Vec4::new(0.05, 0.05, 0.05, 1.0)));
    root.set_border_color(Some(Vec4::new(0.5, 0.5, 0.5, 1.0)));
    root.set_border(Val::Px(1.0));
    root.set_border_radius(RectCorners::single(Val::Px(15.0)));

    let mut root_a = UINode::new("root_a".to_string());
    root_a.set_width(Val::Px(100.0));
    root_a.set_height(Val::Px(100.0));
    root_a.set_margin(Rect::single(Val::Px(10.0)));
    root_a.set_background(Background::Image(test_texture));
    root_a.set_border_color(Some(Vec4::ONE));
    root_a.set_border(Val::Px(1.0));
    root_a.set_border_radius(RectCorners::new(Val::Px(15.0), Val::Px(15.0), Val::Px(15.0), Val::Px(0.0)));

    let mut root_b = UINode::new("root_b".to_string());
    root_b.set_width(Val::Px(200.0));
    root_b.set_height(Val::Px(50.0));
    root_b.set_margin(Rect::single(Val::Px(10.0)));
    root_b.set_padding(Rect::single(Val::Px(5.0)));
    root_b.set_background(Background::Color(Vec4::new(0.05, 0.05, 0.05, 1.0)));
    root_b.set_border_color(Some(Vec4::new(0.5, 0.5, 0.5, 1.0)));
    root_b.set_border(Val::Px(1.0));
    root_b.set_border_radius(RectCorners::single(Val::Px(15.0)));
    root_b.set_text(Some(Text {
        font,
        content: "Hello World!".into(),
        color: Vec4::ONE,
        font_size: 24.0,
        horizontal_align: Align::End,
        vertical_align: Align::End
    }));

    root.add(root_a);
    root.add(root_b);
    
    world.insert(
        EntityBuilder::default()
            .add(UINodeSDFRoot(root))
            .build()
    );
}

#[system]
fn update(
    query: Query<(&MeshRef, &mut Transform)>
) {
    for (_mesh, mut transform) in query.as_iter() {
        transform.rotate_by(Quat::from_euler(glam::EulerRot::XYZ, 0.01, 0.01, 0.01));
    }
}
