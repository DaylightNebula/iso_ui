use bytemuck::{Pod, Zeroable};
use ordered_float::OrderedFloat;

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

/// Modes that `SDFMetadata` can use.
#[repr(u32)]
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum SDFMode {
    #[default]
    Normal = 0,
    HashColor = 1
}

/// Raw data associated with the shaders implementation of SDFShape
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
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

/// Raw data associated with the shaders implementation of SDFStyle
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawStyle {
    pub primary_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_color: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>),
    pub border_width: OrderedFloat<f32>,
    pub texture_ptr: u32,
    pub _padding: (u32, u32)
}

unsafe impl Pod for SDFRawStyle {}
unsafe impl Zeroable for SDFRawStyle {}

/// Raw data associated with the shaders implementation of SDFRectangle
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct SDFRawRectangle {
    pub radii: (OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>, OrderedFloat<f32>)
}

unsafe impl Pod for SDFRawRectangle {}
unsafe impl Zeroable for SDFRawRectangle {}

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
