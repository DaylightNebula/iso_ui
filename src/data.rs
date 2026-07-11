use std::collections::LinkedList;

use anarchy::macros::{Getters, GettersMut, Setters};
use magician_vgpu::glam::*;
use mutual::CowData;

use crate::{ChunkHandle, TreeBufferElement, shader::{SDFRawBezier, SDFRawGlyph, SDFRawRectangle, SDFRawShape, SDFRawStyle}};

/// Screen-wide parameters passed to the UI SDF shader each frame.
///
/// `screen_dimensions` is the render target size in pixels, `time` is elapsed
/// application time in seconds for animation, and `mode` selects how shapes are
/// colored.
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
    /// Default rendering: shapes use their assigned `SDFStyle` colors.
    #[default]
    Normal = 0,
    /// Debug rendering: each shape is assigned a deterministic hash color.
    HashColor = 1
}

/// A node in the CPU-side UI element tree.
///
/// Elements are laid out in screen space with a `center` and `dimensions`, carry
/// a `SDFStyle` and `SDFShape`, and may own a linked list of `children`.
/// When uploaded via `crate::TreeBuffer`, each node is converted to a
/// `crate::shader::SDFRawShape` and shape-specific detail is stored in
/// `crate::ChunkedBuffer`s. The `handles` field records the GPU chunks written
/// during that conversion and is not set by callers.
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

/// Identifies the GPU chunk that holds shape-specific data for an `SDFElement`.
///
/// Each variant corresponds to an `SDFShape` variant and stores the
/// `ChunkHandle` or raw index the shader uses to look up geometry.
#[derive(Clone, Default)]
pub enum SDFRawStyleHandle {
    /// No shape-specific data (empty shape or circle).
    #[default]
    Empty,
    /// Handle to a rounded-rectangle radii chunk.
    Rectangle(ChunkHandle<SDFRawRectangle>),
    /// Handle to a single cubic-bezier curve chunk.
    Curve(ChunkHandle<SDFRawBezier>),
    /// Handles to the bezier curves and glyph header describing a vector glyph.
    Glyph(ChunkHandle<SDFRawBezier>, ChunkHandle<SDFRawGlyph>),
    /// A raw buffer index when no chunked lookup is needed.
    Raw(u32)
}

impl SDFRawStyleHandle {
    /// Returns the 32-bit index the shader uses to fetch this element's shape data.
    ///
    /// `SDFRawStyleHandle::Empty` maps to `u32::MAX`, which the shader treats
    /// as "no pointer".
    pub fn handle_ptr(&self) -> u32 {
        match &self {
            SDFRawStyleHandle::Empty => std::u32::MAX,
            SDFRawStyleHandle::Rectangle(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Curve(chunk_handle) => *chunk_handle.start_idx(),
            SDFRawStyleHandle::Glyph(_, ptr) => *ptr.start_idx(),
            SDFRawStyleHandle::Raw(ptr) => *ptr
        }
    }
}

/// Fill and border colors applied when rasterizing an `SDFElement`.
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

/// Geometry drawn inside an `SDFElement`'s bounding box.
#[derive(Default, Debug, Clone, PartialEq)]
pub enum SDFShape {
    /// No visible geometry; only children are rendered.
    #[default]
    Empty,
    /// A filled circle inscribed in the element's dimensions.
    Circle,
    /// A rounded rectangle with per-corner radii.
    Rectangle(SDFRectangle),
    /// A single bezier stroke defined by three control-point offsets.
    Bezier(SDFCurve),
    /// A vector glyph composed of one or more bezier strokes.
    Glyph(Vec<SDFCurve>)
}

/// Per-corner radii of a rounded rectangle.
///
/// Components are ordered `(top-left, top-right, bottom-right, bottom-left)`.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFRectangle {
    pub radii: Vec4
}

/// A bezier stroke relative to an element's center.
///
/// Control points are expressed as offsets from the element center, and
/// `thickness` sets the stroke width in pixels.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct SDFCurve {
    pub a_offset: Vec2,
    pub b_offset: Vec2,
    pub c_offset: Vec2,
    pub thickness: f32
}
