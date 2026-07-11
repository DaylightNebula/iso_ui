use bytemuck::{Pod, Zeroable};
use magician_vgpu::VirtualGpu;
use ordered_float::OrderedFloat;

use crate::{ChunkedBufferContent, SDFElement, SDFShape, TreeBufferContent, UIRenderResources};

const HALF_MASK: u32 = 0xFFFF;
const HALF_NONE: u32 = 0xFFFF;

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

impl TreeBufferContent for SDFRawShape {
    type ConvertInput = UIRenderResources;
    type InputType = SDFElement;

    fn new_gpu_type(vgpu: &VirtualGpu, rust: &Self::InputType, input: &Self::ConvertInput, next_ptr: u32, first_child_ptr: u32) -> Self {
        let shape_ty = match &rust.shape {
            SDFShape::Empty => 0,
            SDFShape::Circle => 1,
            SDFShape::Rectangle(_sdfrectangle) => 2,
            SDFShape::Bezier(_sdfcurve) => 3,
            SDFShape::Glyph(_sdfcurves) => 4
        };

        // todo fix dropping

        let shape_ptr = match &rust.shape {
            SDFShape::Rectangle(sdfrectangle) => input.rectangles_buffer().get(
                vgpu, 
                &[SDFRawRectangle { 
                    radii: (
                        sdfrectangle.radii.x.into(),
                        sdfrectangle.radii.y.into(),
                        sdfrectangle.radii.z.into(),
                        sdfrectangle.radii.w.into()
                    )
                }]
            ).ok(),
            SDFShape::Bezier(_sdfcurve) => todo!(),
            SDFShape::Glyph(_sdfcurves) => todo!(),
            _ => None
        };

        let style_ptr = input.styles_buffer().get(
            vgpu, 
            &[SDFRawStyle {
                primary_color: (rust.style.primary_color.x.into(), rust.style.primary_color.y.into(), rust.style.primary_color.z.into(), rust.style.primary_color.w.into()),
                border_color: (rust.style.border_color.x.into(), rust.style.border_color.y.into(), rust.style.border_color.z.into(), rust.style.border_color.w.into()),
                border_width: rust.style.border_width.into(),
                texture_ptr: std::u32::MAX,
                _padding: (0, 0)
            }]
        ).unwrap();

        let looks_ptrs = pack_u32(
            shape_ptr.as_ref().map(|a| *a.start_idx() as u16).unwrap_or(std::u16::MAX),
            *style_ptr.start_idx() as u16
        );
        rust.handles.set((style_ptr, shape_ptr));

        Self {
            center: (rust.center.x.into(), rust.center.y.into()),
            dimensions: (rust.dimensions.x.into(), rust.dimensions.y.into()),
            shape_ty, looks_ptrs,
            next_ptrs: (pack_half(next_ptr) << 16) | pack_half(first_child_ptr),
            _pad0: 0,
        }
    }

    fn set_next_ptr(&mut self, ptr: u32) {
        self.next_ptrs = (self.next_ptrs & HALF_MASK) | (pack_half(ptr) << 16);
    }

    fn set_child_ptr(&mut self, ptr: u32) {
        self.next_ptrs = (self.next_ptrs & (HALF_MASK << 16)) | pack_half(ptr);
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

/// Pack a (possibly u32::MAX) pointer into its lower 16 bits, mapping
/// u32::MAX -> 0xFFFF (the "no pointer" sentinel for a half).
#[inline]
fn pack_half(ptr: u32) -> u32 {
    if ptr == u32::MAX {
        HALF_NONE
    } else {
        debug_assert!(
            ptr <= HALF_MASK,
            "pointer {ptr} does not fit in 16 bits used by SDFRawShape::next_ptrs"
        );
        ptr & HALF_MASK
    }
}

fn pack_u32(val1: u16, val2: u16) -> u32 {
    ((val1 as u32) << 16) | (val2 as u32)
}
