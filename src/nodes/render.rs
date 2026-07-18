use magician_vgpu::glam::*;

use crate::{SDFRectangle, SDFShape, SDFStyle, nodes::{Align, Background, Display, PositionType, Rect, SDFElement, Style, Text, UINode, Val}};

// ── helpers ─────────────────────────────────────────────────────────────────

const TRANSPARENT: Vec4 = Vec4::ZERO;

fn eval(val: Val, display_size: [f32; 2]) -> Option<f32> {
    val.eval(&display_size)
}

fn eval_or(val: Val, display_size: [f32; 2], fallback: f32) -> f32 {
    eval(val, display_size).unwrap_or(fallback)
}

/// Resolve a `Rect` into concrete `[top, bottom, left, right]` pixel values.
fn resolve_rect(rect: Rect, display_size: [f32; 2]) -> [f32; 4] {
    [
        eval_or(*rect.top(),    display_size, 0.0),
        eval_or(*rect.bottom(), display_size, 0.0),
        eval_or(*rect.left(),   display_size, 0.0),
        eval_or(*rect.right(),  display_size, 0.0),
    ]
}

/// Resolve border-radius corners into a `Vec4` (top-left, top-right, bottom-left, bottom-right).
fn resolve_radii(style: &Style, display_size: [f32; 2]) -> Vec4 {
    let corners = style.border_radius();
    Vec4::new(
        eval_or(*corners.bottom_right(), display_size, 0.0),
        eval_or(*corners.bottom_left(),  display_size, 0.0),
        eval_or(*corners.top_right(),    display_size, 0.0),
        eval_or(*corners.top_left(),     display_size, 0.0),
    )
}

fn background_to_style(style: &Style, display_size: [f32; 2]) -> SDFStyle {
    let (primary_color, texture) = match style.background() {
        Background::Empty       => (TRANSPARENT, None),
        Background::Color(c)    => (*c, None),
        Background::Image(h)    => (Vec4::ONE, Some(h.clone())),
    };

    let border_color = style.border_color().unwrap_or(TRANSPARENT);
    let border_width = eval_or(*style.border(), display_size, 0.0);

    SDFStyle { primary_color, border_color, border_width, texture }
}

// ── main entry point ─────────────────────────────────────────────────────────

/// Convert a flat list of root `UINode`s into `SDFElement`s laid out over
/// `display_size` (width, height) in pixels.  Roots are stacked as a vertical
/// flex column filling the whole display.
pub fn layout_ui_nodes(nodes: &[UINode], display_size: [f32; 2]) -> Vec<SDFElement> {
    // Treat the root list as a flex-column that fills the display.
    let available = Vec2::from_array(display_size);
    layout_children(nodes, Vec2::ZERO, available, display_size)
}

// ── recursive layout ─────────────────────────────────────────────────────────

fn layout_node(
    node: &UINode,
    origin: Vec2,
    available: Vec2,
    display_size: [f32; 2],
) -> SDFElement {
    let style = node.style();

    // ── 1. resolve own size ───────────────────────────────────────────────

    let margin  = resolve_rect(*style.margin(),  display_size);
    let padding = resolve_rect(*style.padding(), display_size);

    // Space available inside the margin box.
    let margin_h = margin[2] + margin[3]; // left + right
    let margin_v = margin[0] + margin[1]; // top  + bottom

    // Space available *for* the node itself (before we know its size).
    let max_w = available.x - margin_h;
    let max_h = available.y - margin_v;

    let mut width  = eval(*style.width(),  display_size).unwrap_or(max_w);
    let mut height = eval(*style.height(), display_size).unwrap_or(max_h);

    // Clamp to min / max.
    if let Some(v) = eval(*style.min_width(),  display_size) { width  = width.max(v); }
    if let Some(v) = eval(*style.max_width(),  display_size) { width  = width.min(v); }
    if let Some(v) = eval(*style.min_height(), display_size) { height = height.min(v); }
    if let Some(v) = eval(*style.max_height(), display_size) { height = height.min(v); }

    // Aspect-ratio override.
    if let Some(ar) = style.aspect_ratio() {
        if *ar == 0.0 {
            // Match display aspect ratio.
            let display_ar = display_size[0] / display_size[1].max(0.001);
            height = width / display_ar;
        } else {
            height = width / ar;
        }
    }

    let dimensions = Vec2::new(width, height);

    // ── 2. position ───────────────────────────────────────────────────────

    // `origin` is the top-left of the allocated slot *inside* the parent's
    // padding box.  We add the node's own margin.
    let tl = match style.position_type() {
        PositionType::Relative => {
            Vec2::new(origin.x + margin[2], origin.y + margin[0])
        }
        PositionType::Absolute(rect) => {
            // Absolute positions are relative to the display root.
            let abs_x = eval(*rect.left(), display_size)
                .unwrap_or_else(|| display_size[0] - width - eval_or(*rect.right(), display_size, 0.0));
            let abs_y = eval(*rect.top(), display_size)
                .unwrap_or_else(|| display_size[1] - height - eval_or(*rect.bottom(), display_size, 0.0));
            Vec2::new(abs_x, abs_y)
        }
    };

    // Center is what `SDFElement` stores.
    let center = tl + dimensions * 0.5;

    // ── 3. inner area for children ────────────────────────────────────────

    let pad_h = padding[2] + padding[3];
    let pad_v = padding[0] + padding[1];
    let inner = Vec2::new(
        (width  - pad_h).max(0.0),
        (height - pad_v).max(0.0),
    );
    // Top-left of the padding box.
    let inner_origin = tl + Vec2::new(padding[2], padding[0]);

    // ── 4. layout children ────────────────────────────────────────────────

    let children_nodes = node.children();
    let child_elements = if children_nodes.is_empty() {
        vec![]
    } else {
        layout_flex(style, children_nodes, inner_origin, inner, display_size)
    };

    // ── 5. build SDFElement ───────────────────────────────────────────────

    let sdf_style = background_to_style(style, display_size);
    let radii     = resolve_radii(style, display_size);

    let shape = match style.background() {
        Background::Empty => SDFShape::Empty,
        _                 => SDFShape::Rectangle(SDFRectangle { radii }),
    };

    // Text child (rendered as a glyph SDFElement layered on top).
    let mut children: Vec<SDFElement> = child_elements
        .into_iter()
        .collect();

    if let Some(text) = style.text() {
        if let Some(text_el) = build_text_element(text, node.padding(), center, dimensions, &display_size) {
            children.push(text_el);
        }
    }

    SDFElement { center, dimensions, style: sdf_style, shape, children, ..Default::default() }
}

// ── flex layout ───────────────────────────────────────────────────────────────

fn layout_flex(
    parent_style: &Style,
    children: &[UINode],
    content_origin: Vec2,
    content_size: Vec2,
    display_size: [f32; 2],
) -> Vec<SDFElement> {
    let (is_row, v_align, h_align) = match parent_style.display() {
        Display::FlexRow    { vertical, horizontal } => (true,  *vertical, *horizontal),
        Display::FlexColumn { vertical, horizontal } => (false, *vertical, *horizontal),
        Display::Grid => return layout_grid(children, content_origin, content_size, display_size),
    };

    // ── pass 1: measure children (use full content_size as budget) ────────

    // Separate absolute-positioned children — they don't participate in flow.
    let (flow_children, abs_children): (Vec<_>, Vec<_>) = children.iter().partition(|n| {
        !matches!(n.style().position_type(), PositionType::Absolute(_))
    });

    // Compute fixed / intrinsic sizes along the main axis.
    let (main_axis, cross_axis) = if is_row {
        (content_size.x, content_size.y)
    } else {
        (content_size.y, content_size.x)
    };

    // For each flow child, figure out its desired main-axis size.
    let child_mains: Vec<f32> = flow_children.iter().map(|n| {
        let s = n.style();
        let m = resolve_rect(*s.margin(), display_size);
        let margin_main = if is_row { m[2] + m[3] } else { m[0] + m[1] };

        let explicit = if is_row {
            eval(*s.width(), display_size)
        } else {
            eval(*s.height(), display_size)
        };

        explicit.unwrap_or(0.0) + margin_main
    }).collect();

    let total_fixed: f32 = child_mains.iter().sum();
    let auto_count = child_mains.iter().filter(|&&v| v == 0.0).count() as f32;
    let remaining = (main_axis - total_fixed).max(0.0);
    let auto_share = if auto_count > 0.0 { remaining / auto_count } else { 0.0 };

    // Resolved main sizes (including margin).
    let resolved_mains: Vec<f32> = child_mains.iter()
        .map(|&v| if v == 0.0 { auto_share } else { v })
        .collect();

    // ── pass 2: compute cross-axis offset for alignment ───────────────────

    let align_offset = |child_cross: f32| -> f32 {
        match if is_row { v_align } else { h_align } {
            Align::Start  => 0.0,
            Align::End    => (cross_axis - child_cross).max(0.0),
            Align::Center => ((cross_axis - child_cross) * 0.5).max(0.0),
        }
    };

    // ── pass 3: compute main-axis starting offset ─────────────────────────

    let total_main: f32 = resolved_mains.iter().sum();
    let mut cursor = match if is_row { h_align } else { v_align } {
        Align::Start  => 0.0,
        Align::End    => (main_axis - total_main).max(0.0),
        Align::Center => ((main_axis - total_main) * 0.5).max(0.0),
    };

    // ── pass 4: lay out each flow child ───────────────────────────────────

    let mut result = Vec::with_capacity(children.len());

    for (node, main_size) in flow_children.iter().zip(resolved_mains.iter()) {
        let s = node.style();
        let m = resolve_rect(*s.margin(), display_size);

        let (child_w, child_h, child_origin) = if is_row {
            let margin_v = m[0] + m[1];
            let child_h  = eval(*s.height(), display_size).unwrap_or(cross_axis - margin_v);
            let child_w  = main_size - m[2] - m[3];
            let ox = content_origin.x + cursor + m[2];
            let oy = content_origin.y + align_offset(child_h + margin_v) + m[0];
            (child_w, child_h, Vec2::new(ox, oy))
        } else {
            let margin_h = m[2] + m[3];
            let child_w  = eval(*s.width(), display_size).unwrap_or(cross_axis - margin_h);
            let child_h  = main_size - m[0] - m[1];
            let ox = content_origin.x + align_offset(child_w + margin_h) + m[2];
            let oy = content_origin.y + cursor + m[0];
            (child_w, child_h, Vec2::new(ox, oy))
        };

        let available = Vec2::new(child_w.max(0.0), child_h.max(0.0));

        // layout_node expects `origin` = top-left of the slot *before* the
        // child's own margin is added, so pass content_origin + cursor offset
        // *without* the margin (layout_node re-adds the margin internally).
        // We've already applied the margin above, so pass child_origin directly
        // as the pre-margin slot origin by subtracting the margin back out:
        let slot_origin = Vec2::new(
            child_origin.x - m[2],
            child_origin.y - m[0],
        );

        result.push(layout_node(node, slot_origin, available, display_size));
        cursor += main_size;
    }

    // ── pass 5: absolute children ─────────────────────────────────────────

    for node in abs_children {
        result.push(layout_node(node, content_origin, content_size, display_size));
    }

    result
}

// ── grid (minimal: equal-width columns, wrapping rows) ───────────────────────

fn layout_grid(
    children: &[UINode],
    content_origin: Vec2,
    content_size: Vec2,
    display_size: [f32; 2],
) -> Vec<SDFElement> {
    // Simple auto-grid: sqrt(n) columns, equal cell sizes.
    let n = children.len() as f32;
    let cols = n.sqrt().ceil() as usize;
    let rows = (children.len() + cols - 1) / cols;
    let cell_w = content_size.x / cols as f32;
    let cell_h = content_size.y / rows as f32;

    children.iter().enumerate().map(|(i, node)| {
        let col = (i % cols) as f32;
        let row = (i / cols) as f32;
        let origin = Vec2::new(
            content_origin.x + col * cell_w,
            content_origin.y + row * cell_h,
        );
        layout_node(node, origin, Vec2::new(cell_w, cell_h), display_size)
    }).collect()
}

// ── text element ──────────────────────────────────────────────────────────────

fn build_text_element(
    text: &Text,
    padding: &Rect,
    parent_center: Vec2,
    parent_dimensions: Vec2,
    display_size: &[f32; 2]
) -> Option<SDFElement> {
    let line = text.font.render_glyph_line(&text.content, text.font_size).ok()?;

    let total_w = line.dimensions.x;
    let total_h = line.dimensions.y;

    // Compute the top-left of the text block relative to the parent's content box.
    let x_start = match text.horizontal_align {
        Align::Start  => parent_center.x - parent_dimensions.x * 0.5 + padding.left().eval(display_size).unwrap_or(0.0),
        Align::End    => parent_center.x + parent_dimensions.x * 0.5 - total_w - padding.right().eval(display_size).unwrap_or(0.0),
        Align::Center => parent_center.x - total_w * 0.5,
    };
    let y_start = match text.vertical_align {
        Align::Start  => parent_center.y - parent_dimensions.y * 0.5 + padding.top().eval(display_size).unwrap_or(0.0),
        Align::End    => parent_center.y + parent_dimensions.y * 0.5 - total_h - padding.bottom().eval(display_size).unwrap_or(0.0),
        Align::Center => parent_center.y - total_h * 0.5,
    };

    let mut cur_x = x_start;
    let glyph_y = y_start + total_h * 0.5;

    let glyphs: Vec<SDFElement> = line.chars
        .into_iter()
        .filter_map(|entry| {
            let advance = entry.advance;
            if let Some(shape) = entry.shape {
                let center = Vec2::new(cur_x + advance * 0.5, glyph_y);
                cur_x += advance;
                Some(SDFElement {
                    center,
                    dimensions: entry.dimensions,
                    style: SDFStyle {
                        primary_color: text.color,
                        border_color: TRANSPARENT,
                        border_width: 0.0,
                        texture: None
                    },
                    shape,
                    children: Vec::new(),
                    ..Default::default()
                })
            } else {
                cur_x += advance;
                None
            }
        })
        .collect();

    let text_center = Vec2::new(x_start + total_w * 0.5, y_start + total_h * 0.5);

    Some(SDFElement {
        center: text_center,
        dimensions: Vec2::new(total_w, total_h),
        style: SDFStyle {
            primary_color: text.color,
            border_color: TRANSPARENT,
            border_width: 0.0,
            texture: None
        },
        shape: SDFShape::Empty,
        children: glyphs,
        ..Default::default()
    })
}

// ── public convenience ────────────────────────────────────────────────────────

/// Layout a list of children directly inside a given parent node's space,
/// without creating an element for the parent itself.  Useful for updating a
/// sub-tree in place.
pub fn layout_children(
    nodes: &[UINode],
    origin: Vec2,
    available: Vec2,
    display_size: [f32; 2],
) -> Vec<SDFElement> {
    // Synthesise a default column-flex parent style and run the flex pass.
    let parent_style = Style::default(); // FlexColumn { Start, Start }
    layout_flex(&parent_style, nodes, origin, available, display_size)
}
