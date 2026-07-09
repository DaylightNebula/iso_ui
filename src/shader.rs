use bytemuck::{Pod, Zeroable};
use ordered_float::OrderedFloat;

use crate::ChunkedBufferContent;

/// This is a data container meant to be uploaded to the buffers
/// for use in the UISDFShader for rendering.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SDFRawShaderData {
    pub metadata: SDFRawMetadata,
    pub shapes: [SDFRawShape; 1000],
    pub styles: [SDFRawStyle; 1000],
    pub rectangles: [SDFRawRectangle; 1000],
    pub bezier: [SDFRawBezier; 1000],
    pub glyphs: [SDFRawGlyph; 1000]
}

/// Raw data associated with the shaders implementation of SDFMetadata
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawMetadata {
    pub screen_dimensions: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub time: OrderedFloat<f32>,
    pub mode: u32
}

unsafe impl Pod for SDFRawMetadata {}
unsafe impl Zeroable for SDFRawMetadata {}

/// Raw data associated with the shaders implementation of SDFShape
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawShape {
    pub center: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub dimensions: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub shape_ty: u32,
    pub looks_ptrs: u32,
    pub next_ptrs: u32,
    pub _pad0: u32
}

unsafe impl Pod for SDFRawShape {}
unsafe impl Zeroable for SDFRawShape {}

impl Default for SDFRawShape {
    fn default() -> Self {
        Self {
            center: (0.0.into(), 0.0.into()),
            dimensions: (0.0.into(), 0.0.into()),
            shape_ty: 0,
            looks_ptrs: std::u32::MAX,
            next_ptrs: std::u32::MAX,
            _pad0: 0
        }
    }
}

/// Raw data associated with the shaders implementation of SDFStyle
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawStyle {
    pub primary_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_width: OrderedFloat<f32>,
    pub texture_ptr: u32,
    pub _padding: (u32, u32)
}

impl Default for SDFRawStyle {
    fn default() -> Self {
        Self {
            primary_color: (1.0.into(), 1.0.into(), 1.0.into(), 0.0.into()),
            border_color: (1.0.into(), 1.0.into(), 1.0.into(), 0.0.into()),
            border_width: 0.0.into(),
            texture_ptr: std::u32::MAX,
            _padding: (0, 0),
        }
    }
}

unsafe impl Pod for SDFRawStyle {}
unsafe impl Zeroable for SDFRawStyle {}
impl ChunkedBufferContent for SDFRawStyle {}

/// Raw data associated with the shaders implementation of SDFRectangle
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawRectangle {
    pub radii: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>)
}

unsafe impl Pod for SDFRawRectangle {}
unsafe impl Zeroable for SDFRawRectangle {}
impl ChunkedBufferContent for SDFRawRectangle {}

/// Raw data associated with the shaders implementation of SDFBezier
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawBezier {
    pub a_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub b_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub c_off: (OrderedFloat<f32>, OrderedFloat<f32>),
    pub thickness: OrderedFloat<f32>,
    pub _pad0: u32
}

unsafe impl Pod for SDFRawBezier {}
unsafe impl Zeroable for SDFRawBezier {}
impl ChunkedBufferContent for SDFRawBezier {}

/// Raw data associated with the shaders implementation of SDFGlyph
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawGlyph {
    pub start_idx: u32,
    pub length: u32,
    pub _pad0: u32,
    pub _pad1: u32
}

unsafe impl Pod for SDFRawGlyph {}
unsafe impl Zeroable for SDFRawGlyph {}
impl ChunkedBufferContent for SDFRawGlyph {}
