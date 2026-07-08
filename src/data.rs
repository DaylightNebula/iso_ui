use std::collections::LinkedList;

use anarchy::macros::{Getters, GettersMut, Setters};
use magician_vgpu::glam::*;

#[derive(Debug, Getters, Setters, GettersMut, Clone, PartialEq)]
pub struct SDFMetadata {
    pub screen_dimensions: Vec2,
    pub time: f32,
    pub mode: SDFMode
}

/// Modes that `SDFMetadata` can use.
#[repr(u32)]
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum SDFMode {
    #[default]
    Normal = 0,
    HashColor = 1
}

#[derive(Debug, Getters, Setters, GettersMut, Clone, PartialEq)]
pub struct SDFElement {
    pub center: Vec2,
    pub dimensions: Vec2,
    pub style: SDFStyle,
    pub shape: SDFShape,
    pub children: LinkedList<SDFElement>
}

#[derive(Debug, Getters, Setters, GettersMut, Clone, PartialEq)]
pub struct SDFStyle {
    pub primary_color: Vec4,
    pub border_color: Vec4,
    pub border_width: f32,
    // pub texture: Option<RenderAssetHandle>
}

#[derive(Default, Debug, Clone, PartialEq)]
pub enum SDFShape {
    #[default]
    Empty,
    Circle,
    Rectangle(SDFRectangle),
    Bezier(SDFCurve),
    Glyph(Vec<SDFCurve>)
}

#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFRectangle {
    pub radii: Vec4
}

#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFCurve {
    pub a_offset: Vec2,
    pub b_offset: Vec2,
    pub c_offset: Vec2,
    pub thickness: f32
}
