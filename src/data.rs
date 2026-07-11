use std::collections::LinkedList;

use anarchy::macros::{Getters, GettersMut, Setters};
use magician_vgpu::glam::*;
use mutual::CowData;

use crate::{ChunkHandle, TreeBufferElement, shader::{SDFRawBezier, SDFRawGlyph, SDFRawRectangle, SDFRawShape, SDFRawStyle}};

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

#[derive(Default, Getters, Setters, GettersMut, Clone)]
pub struct SDFElement {
    pub center: Vec2,
    pub dimensions: Vec2,
    pub style: SDFStyle,
    pub shape: SDFShape,
    pub children: LinkedList<SDFElement>,
    pub handles: CowData<(ChunkHandle<SDFRawStyle>, SDFRawStyleHandle)>
}

impl TreeBufferElement for SDFElement {
    type OutputType = SDFRawShape;

    fn children(&self) -> impl Iterator<Item = &Self> {
        self.children.iter()
    }
}

#[derive(Clone, Default)]
pub enum SDFRawStyleHandle {
    #[default]
    Empty,
    Rectangle(ChunkHandle<SDFRawRectangle>),
    Curve(ChunkHandle<SDFRawBezier>),
    Glyph(ChunkHandle<SDFRawBezier>, ChunkHandle<SDFRawGlyph>)
}

impl SDFRawStyleHandle {
    pub fn handle_ptr(&self) -> u32 {
        match &self {
            SDFRawStyleHandle::Empty => std::u32::MAX,
            SDFRawStyleHandle::Rectangle(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Curve(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Glyph(_, ptr) => *ptr.start_idx()
        }
    }
}

#[derive(Debug, Getters, Setters, GettersMut, Clone, PartialEq)]
pub struct SDFStyle {
    pub primary_color: Vec4,
    pub border_color: Vec4,
    pub border_width: f32,
}

impl Default for SDFStyle {
    fn default() -> Self {
        Self {
            primary_color: Vec4::ONE,
            border_color: Vec4::ZERO,
            border_width: 5.0
        }
    }
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
