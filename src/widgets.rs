//! Immediate mode widgets built from `UINode`s. Systems describe their windows every frame
//! through the `Widget` resource, `widgets_end` hands them to the `UIPlugin` as one node tree, and the
//! next frame's `widgets_begin` hit tests the pointer against where that tree was laid out.

use std::{ops::RangeInclusive, sync::{Arc, Mutex}};

use ahash::AHashMap;
use anarchy::{EntityBuilder, Event, EventSystemMinIDTracker, Query, Res, ResMut, WorldDatabase, macros::{Resource, system}};
use cell::{App, Graphics, Plugin, WindowDimensions, WindowEvent};
use magician_vgpu::glam::{Vec2, Vec4};
use winit::{event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta}, keyboard::{Key, NamedKey}, raw_window_handle::{HasDisplayHandle, RawDisplayHandle}};

use crate::{Align, Background, Display, PositionType, Rect, RectCorners, SDFFont, Text, UINode, UINodeSDFRoot, Val};

const FONT_SIZE: f32 = 14.0;
const SMALL_FONT_SIZE: f32 = 11.5;
const HEADING_FONT_SIZE: f32 = 22.0;
const ROW: f32 = 24.0;
const PAD: f32 = 8.0;
const GAP: f32 = 4.0;
const TEXT_INSET: f32 = 7.0;
const INDENT: f32 = 14.0;
const RADIUS: f32 = 3.0;
const LIP: f32 = 2.0;
const CHECK: f32 = 14.0;
const SLIDER: f32 = 120.0;
const GROOVE: f32 = 4.0;
const KNOB: Vec2 = Vec2::new(8.0, 16.0);
const SWITCH: Vec2 = Vec2::new(28.0, 16.0);
const SWATCH: f32 = 10.0;
const MARKER: f32 = 3.0;
const LED: f32 = 8.0;
const DIGIT: Vec2 = Vec2::new(10.0, 17.0);
const SEGMENT: f32 = 2.5;
const SCROLLBAR: f32 = 6.0;
const ROWS_PER_NOTCH: f32 = 2.0;
const PX_PER_NOTCH: f32 = 50.0;
const NONE: Vec4 = Vec4::ZERO;

/// Widget colors. They're linear, so pick them for an sRGB surface, `srgb` converts hex colors.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Window background.
    pub panel: Vec4,
    /// Window edges, separators and the lip under buttons.
    pub border: Vec4,
    /// Button tops.
    pub widget: Vec4,
    pub hover: Vec4,
    /// Recessed fields, slider grooves and display windows.
    pub well: Vec4,
    /// Background of selected list rows.
    pub selected: Vec4,
    /// Lit things: selection markers, filled grooves, switches, LEDs and display segments.
    pub accent: Vec4,
    /// Text on `accent`.
    pub on_accent: Vec4,
    pub text: Vec4,
    /// Legends, hints and disabled text.
    pub dim: Vec4,
    pub error: Vec4
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            panel: Vec4::new(0.01, 0.01, 0.01, 1.0),
            border: Vec4::new(0.073, 0.073, 0.073, 1.0),
            widget: Vec4::new(0.033, 0.033, 0.033, 1.0),
            hover: Vec4::new(0.064, 0.064, 0.064, 1.0),
            well: Vec4::new(0.004, 0.004, 0.004, 1.0),
            selected: Vec4::new(0.033, 0.089, 0.214, 1.0),
            accent: Vec4::new(0.13, 0.35, 0.9, 1.0),
            on_accent: Vec4::ONE,
            text: Vec4::new(0.694, 0.694, 0.694, 1.0),
            dim: Vec4::new(0.214, 0.214, 0.214, 1.0),
            error: Vec4::new(1.0, 0.214, 0.214, 1.0)
        }
    }
}

/// The linear color of an sRGB hex color like `0xF2A33A`.
pub fn srgb(hex: u32) -> Vec4 {
    let channel = |shift: u32| {
        let c = ((hex >> shift) & 0xFF) as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    Vec4::new(channel(16), channel(8), channel(0), 1.0)
}

/// How a button stands out: `Primary` is lit in the accent, `Danger` has error colored text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    Primary,
    Danger
}

/// Adds the `Widget` resource, drawing its widgets in `font`. Needs the `UIPlugin` to draw them.
pub struct WidgetPlugin {
    font: Arc<SDFFont>,
    bold: Option<Arc<SDFFont>>,
    theme: Theme
}

impl WidgetPlugin {
    pub fn new(font: Arc<SDFFont>) -> Self { Self { font, bold: None, theme: Theme::default() } }

    /// Draws titles, headings and `Ui::strong` text in `bold` rather than the regular font.
    pub fn with_bold(mut self, bold: Arc<SDFFont>) -> Self { self.bold = Some(bold); self }

    pub fn with_theme(mut self, theme: Theme) -> Self { self.theme = theme; self }
}

impl Plugin for WidgetPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(Widget { font: Some(self.font), bold: self.bold, theme: self.theme, ..Default::default() })
            .on_render_startup(setup_widgets)
            .on_render_update(widgets_begin)
            .on_render_update(widgets_end)
    }
}

/// Where a window goes once its size is known.
#[derive(Clone, Copy)]
enum Place {
    At(Vec2),
    /// `anchor` from (0, 0) top left to (1, 1) bottom right of the screen, `margin` pixels in from its edges.
    Anchor(Vec2, Vec2)
}

/// Which windows a window is drawn over.
#[derive(Clone, Copy, PartialEq)]
enum Layer { Hud, Panel, Popup }

/// Pointer, keyboard and widget state shared by every window, plus the windows built this frame.
#[derive(Resource, Default)]
pub struct Widget {
    font: Option<Arc<SDFFont>>,
    bold: Option<Arc<SDFFont>>,
    theme: Theme,
    clipboard: Option<Mutex<Clipboard>>,
    scale: f32,
    screen: Vec2,
    cursor: Vec2,
    cursor_delta: Vec2,
    down: bool,
    /// Left button went down this frame.
    pressed: bool,
    /// Right button went down this frame.
    right_pressed: bool,
    /// Wheel notches this frame, positive scrolls up.
    wheel: f32,
    over_ui: bool,
    /// Topmost window under the pointer.
    over_window: Option<String>,
    /// Topmost widget under the pointer.
    hot: Option<String>,
    /// Widget the held left button went down on.
    active: Option<String>,
    /// Field taking typed text, and its text so far.
    focus: Option<(String, String)>,
    /// Field Enter or a click elsewhere took focus from this frame, and its final text.
    committed: Option<(String, String)>,
    /// Where iso_ui laid out last frame's widgets.
    rects: AHashMap<String, Area>,
    open: AHashMap<String, bool>,
    /// First shown row of each list, fractional between wheel notches.
    scrolls: AHashMap<String, f32>,
    /// Where each window was dragged to.
    positions: AHashMap<String, Vec2>,
    tooltip: Option<String>,
    /// Drawn under the panels, each window after its id.
    huds: Vec<(String, UINode)>,
    panels: Vec<(String, UINode)>,
    /// Drawn over the panels.
    popups: Vec<(String, UINode)>,
    /// Last frame's tree, laid out by iso_ui since.
    laid_out: UINode,
    /// Ids of `laid_out`'s windows, in order.
    laid_out_ids: Vec<String>
}

impl Widget {
    pub fn set_font(&mut self, font: Arc<SDFFont>) { self.font = Some(font); }

    pub fn set_bold_font(&mut self, font: Arc<SDFFont>) { self.bold = Some(font); }

    pub fn theme(&self) -> &Theme { &self.theme }

    pub fn theme_mut(&mut self) -> &mut Theme { &mut self.theme }

    pub fn cursor(&self) -> Vec2 { self.cursor }

    pub fn screen(&self) -> Vec2 { self.screen }

    /// Logical pixels to window pixels.
    pub fn px(&self, v: f32) -> f32 { v * self.scale }

    /// Whether the pointer is over a window.
    pub fn over_ui(&self) -> bool { self.over_ui }

    /// Whether a widget holds the pointer, like a slider being dragged.
    pub fn using_pointer(&self) -> bool { self.active.is_some() }

    /// Whether clicks and drags belong to the UI rather than what's behind it.
    pub fn wants_pointer(&self) -> bool { self.over_ui || self.using_pointer() }

    pub fn clipboard_text(&self) -> Option<String> {
        self.clipboard.as_ref()?.lock().ok()?.text()
    }

    /// A window titled `title` that can be dragged around by its title, first shown at `pos`.
    /// `width` is in logical pixels.
    pub fn panel(&mut self, title: &str, pos: Vec2, width: f32, build: impl FnOnce(&mut Ui)) {
        self.panel_with_header(title, pos, width, |_| {}, build);
    }

    /// A `panel` with `header`'s widgets laid out right of the title.
    pub fn panel_with_header(&mut self, title: &str, pos: Vec2, width: f32, header: impl FnOnce(&mut Ui), build: impl FnOnce(&mut Ui)) {
        let title_id = format!("{title}/title");
        let pos = self.positions.entry(title.to_string()).or_insert(pos);
        if self.down && self.active.as_ref() == Some(&title_id) { *pos += self.cursor_delta; }
        let pos = *pos;
        self.window(title, Some((title_id, header)), Place::At(pos), Some(width), false, Layer::Panel, build);
    }

    /// An untitled window at `pos` drawn over the panels.
    pub fn popup(&mut self, id: &str, pos: Vec2, width: f32, build: impl FnOnce(&mut Ui)) {
        self.window(id, None::<(String, fn(&mut Ui))>, Place::At(pos), Some(width), false, Layer::Popup, build);
    }

    /// An untitled window pinned to the screen under the panels, `anchor` running from (0, 0)
    /// at the top left to (1, 1) at the bottom right and `margin` logical pixels in from the
    /// edges it's anchored to.
    pub fn hud(&mut self, id: &str, anchor: Vec2, margin: Vec2, width: f32, build: impl FnOnce(&mut Ui)) {
        let margin = margin * self.scale;
        self.window(id, None::<(String, fn(&mut Ui))>, Place::Anchor(anchor, margin), Some(width), false, Layer::Hud, build);
    }

    /// A `hud` laying its widgets out left to right, as wide as they are.
    pub fn hud_row(&mut self, id: &str, anchor: Vec2, margin: Vec2, build: impl FnOnce(&mut Ui)) {
        let margin = margin * self.scale;
        self.window(id, None::<(String, fn(&mut Ui))>, Place::Anchor(anchor, margin), None, true, Layer::Hud, build);
    }

    /// A popup at `anchor` of the screen like `hud`, drawn over everything.
    pub fn dialog(&mut self, id: &str, anchor: Vec2, width: f32, build: impl FnOnce(&mut Ui)) {
        self.window(id, None::<(String, fn(&mut Ui))>, Place::Anchor(anchor, Vec2::ZERO), Some(width), false, Layer::Popup, build);
    }

    /// `width` of `None` sizes a `row` window to its widgets.
    fn window(&mut self, id: &str, title: Option<(String, impl FnOnce(&mut Ui))>, place: Place, width: Option<f32>, row: bool, layer: Layer, build: impl FnOnce(&mut Ui)) {
        let pad = self.px(PAD);
        let inner = width.map_or(0.0, |width| self.px(width) - 2.0 * pad);
        let mut ui = Ui::new(self, id.to_string(), inner, row);
        if let Some((title_id, header)) = title {
            let bar_width = ui.width;
            let mut bar = ui.child(true, 0.0);
            header(&mut bar);
            let tools = std::mem::take(&mut bar.node);
            // the title takes what the header leaves, so the whole bar still drags the window
            let gap = if tools.children().is_empty() { 0.0 } else { bar.widget.px(GAP) };
            let rest = (bar_width - bar.extent - gap).max(0.0);
            let (tools_width, tools_height) = (bar.extent, bar.cross);
            let mut title = bar.styled_text(Some(title_id), id, bar.widget.theme.text, Align::Start, FONT_SIZE, true);
            title.set_padding(Rect::new_left(Val::Px(bar.widget.px(2.0))));
            bar.extent = 0.0;
            bar.push(title, Vec2::new(rest, bar.widget.px(ROW)));
            if !tools.children().is_empty() {
                let mut tools = tools;
                tools.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::End });
                bar.push(tools, Vec2::new(tools_width, tools_height));
            }
            let (node, height) = (bar.node, bar.cross.max(bar.widget.px(ROW)));
            ui.push(node, Vec2::new(bar_width, height));
            ui.separator();
        }
        build(&mut ui);
        let content = if row { Vec2::new(ui.extent, ui.cross) } else { Vec2::new(ui.width, ui.extent) };
        let mut node = ui.node;
        let size = content + 2.0 * pad;

        let pos = match place {
            Place::At(pos) => pos,
            Place::Anchor(anchor, margin) => (self.screen - size) * anchor + margin * (Vec2::ONE - 2.0 * anchor)
        };
        // keep the whole window on screen
        let pos = pos.min(self.screen - size).max(Vec2::ZERO);
        if row { node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start }); }
        node.set_position_type(PositionType::Absolute(Rect::new_top_left(Val::Px(pos.y), Val::Px(pos.x))));
        node.set_width(Val::Px(size.x));
        node.set_height(Val::Px(size.y));
        node.set_padding(Rect::single(Val::Px(pad)));
        fill(&mut node, self.theme.panel, self.px(RADIUS + 1.0));
        node.set_border(Val::Px(1.0));
        node.set_border_color(Some(self.theme.border));
        let window = (id.to_string(), node);
        match layer {
            Layer::Hud => self.huds.push(window),
            Layer::Panel => self.panels.push(window),
            Layer::Popup => self.popups.push(window)
        }
    }

    fn font(&self, bold: bool) -> Option<&Arc<SDFFont>> {
        if bold { self.bold.as_ref().or(self.font.as_ref()) } else { self.font.as_ref() }
    }

    fn measure(&self, text: &str, size: f32, bold: bool) -> f32 {
        self.font(bold).map_or(0.0, |font| font.measure_text(text, self.px(size)).x)
    }

    fn text_width(&self, text: &str) -> f32 { self.measure(text, FONT_SIZE, false) }

    fn is_hot(&self, id: &str) -> bool { self.hot.as_deref() == Some(id) }

    fn clicked(&self, id: &str) -> bool { self.pressed && self.is_hot(id) }

    fn dragging(&self, id: &str) -> bool { self.down && self.active.as_deref() == Some(id) }

    fn focused(&self, id: &str) -> Option<&str> {
        self.focus.as_ref().filter(|(focus, _)| focus == id).map(|(_, text)| text.as_str())
    }

    fn key(&mut self, event: &KeyEvent) {
        if self.focus.is_none() { return }
        match &event.logical_key {
            Key::Named(NamedKey::Enter) => self.committed = self.focus.take(),
            Key::Named(NamedKey::Escape) => self.focus = None,
            Key::Named(NamedKey::Backspace) => { self.focus.as_mut().unwrap().1.pop(); }
            _ => if let Some(typed) = &event.text {
                self.focus.as_mut().unwrap().1.extend(typed.chars().filter(|c| !c.is_control()));
            }
        }
    }

    /// Whether a text field has the keyboard, so typing shouldn't also drive shortcuts.
    pub fn typing(&self) -> bool { self.focus.is_some() }

    /// Whether either button went down this frame anywhere but over the window `id`, e.g. to close a popup.
    pub fn pressed_outside(&self, id: &str) -> bool {
        (self.pressed || self.right_pressed) && self.over_window.as_deref() != Some(id)
    }

    /// Records where `node` and its widgets were laid out, the last one under the pointer is on top.
    fn collect(&mut self, node: &UINode) {
        if let (Some(id), Some(area)) = (node.id(), Area::of(node)) {
            if area.contains(self.cursor) { self.hot = Some(id.clone()); }
            self.rects.insert(id.clone(), area);
        }
        for child in node.children() { self.collect(child); }
    }
}

/// Builds the widgets of one window, or of a row or list inside one.
pub struct Ui<'a> {
    widget: &'a mut Widget,
    /// Window id, prefixed to widget ids.
    scope: String,
    node: UINode,
    row: bool,
    /// Width a column's children fill.
    width: f32,
    /// Size used along the main axis so far.
    extent: f32,
    /// Largest child across the main axis.
    cross: f32,
    /// Extra text inset of list entries under a header.
    indent: f32,
    /// Id of the last widget added, what `hint` explains.
    last: Option<String>
}

impl<'a> Ui<'a> {
    fn new(widget: &'a mut Widget, scope: String, width: f32, row: bool) -> Self {
        Self { widget, scope, node: UINode::default(), row, width, extent: 0.0, cross: 0.0, indent: 0.0, last: None }
    }

    fn child(&mut self, row: bool, width: f32) -> Ui<'_> {
        let mut node = UINode::default();
        if row { node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start }); }
        Ui { widget: &mut *self.widget, scope: self.scope.clone(), node, row, width, extent: 0.0, cross: 0.0, indent: self.indent, last: None }
    }

    pub fn clipboard_text(&self) -> Option<String> { self.widget.clipboard_text() }

    pub fn theme(&self) -> &Theme { &self.widget.theme }

    pub fn is_open(&self, key: &str, default: bool) -> bool {
        self.widget.open.get(&self.id(key)).copied().unwrap_or(default)
    }

    fn id(&self, key: &str) -> String { format!("{}/{key}", self.scope) }

    fn row_height(&self) -> f32 { self.widget.px(ROW) }

    fn px(&self, v: f32) -> f32 { self.widget.px(v) }

    /// A child's width: what it needs in a row, all of a column.
    fn fill_width(&self, natural: f32) -> f32 { if self.row { natural } else { self.width } }

    fn text_width(&self, text: &str) -> f32 { self.widget.text_width(text) + 2.0 * self.px(TEXT_INSET) + self.indent }

    /// Adds `node` sized `size` after the children so far.
    fn push(&mut self, mut node: UINode, size: Vec2) {
        let gap = if self.node.children().is_empty() { 0.0 } else { self.px(GAP) };
        // iso_ui shares leftover room between zero sized children
        node.set_width(Val::Px(size.x.max(0.01)));
        node.set_height(Val::Px(size.y.max(0.01)));
        if self.row {
            node.set_margin(Rect::new_left(Val::Px(gap)));
            self.extent += gap + size.x;
            self.cross = self.cross.max(size.y);
        } else {
            node.set_margin(Rect::new_top(Val::Px(gap)));
            self.extent += gap + size.y;
        }
        self.last = node.id().cloned();
        self.node.add(node);
    }

    fn text_node(&self, id: Option<String>, content: &str, color: Vec4, align: Align) -> UINode {
        let mut node = self.styled_text(id, content, color, align, FONT_SIZE, false);
        let inset = self.px(TEXT_INSET);
        node.set_padding(Rect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(inset + self.indent), Val::Px(inset)));
        node
    }

    /// Text without insets, `size` in logical pixels.
    fn styled_text(&self, id: Option<String>, content: &str, color: Vec4, align: Align, size: f32, bold: bool) -> UINode {
        let mut node = id.map_or_else(UINode::default, UINode::new);
        node.set_text(self.widget.font(bold).cloned().map(|font| Text {
            font,
            content: content.to_string(),
            color,
            font_size: self.px(size),
            horizontal_align: align,
            vertical_align: Align::Center
        }));
        node
    }

    /// Shows `hint` next to the pointer while the widget added last is hovered.
    pub fn hint(&mut self, hint: &str) {
        if self.last.as_ref().is_some_and(|id| self.widget.is_hot(id)) { self.widget.tooltip = Some(hint.to_string()); }
    }

    /// Whether the widget added last was right clicked this frame.
    pub fn right_clicked(&self) -> bool {
        self.widget.right_pressed && self.last.as_ref().is_some_and(|id| self.widget.is_hot(id))
    }

    pub fn label(&mut self, text: &str) { self.colored_label(text, self.widget.theme.text) }

    pub fn colored_label(&mut self, text: &str, color: Vec4) {
        let node = self.text_node(None, text, color, Align::Start);
        self.push(node, Vec2::new(self.fill_width(self.text_width(text)), self.row_height()));
    }

    /// Right aligned text `width` logical pixels wide, for columns of numbers.
    pub fn label_right(&mut self, text: &str, width: f32) {
        self.colored_label_right(text, width, self.widget.theme.text);
    }

    pub fn colored_label_right(&mut self, text: &str, width: f32, color: Vec4) {
        let node = self.text_node(None, text, color, Align::End);
        self.push(node, Vec2::new(self.px(width), self.row_height()));
    }

    /// A dim name for the widgets after it, `width` logical pixels wide so a column of them lines up.
    pub fn legend(&mut self, text: &str, width: f32) {
        let mut node = self.styled_text(None, text, self.widget.theme.dim, Align::Start, FONT_SIZE, false);
        node.set_padding(Rect::new_left(Val::Px(self.indent)));
        self.push(node, Vec2::new(self.px(width), self.row_height()));
    }

    /// Text in the bold font.
    pub fn strong(&mut self, text: &str) {
        let width = self.widget.measure(text, FONT_SIZE, true) + self.indent;
        let mut node = self.styled_text(None, text, self.widget.theme.text, Align::Start, FONT_SIZE, true);
        node.set_padding(Rect::new_left(Val::Px(self.indent)));
        self.push(node, Vec2::new(self.fill_width(width), self.row_height()));
    }

    /// Large bold text, for what a window is about.
    pub fn heading(&mut self, text: &str) {
        let width = self.widget.measure(text, HEADING_FONT_SIZE, true);
        let node = self.styled_text(None, text, self.widget.theme.text, Align::Start, HEADING_FONT_SIZE, true);
        self.push(node, Vec2::new(self.fill_width(width), self.px(HEADING_FONT_SIZE * 1.4)));
    }

    /// Small text in `color`, for notes under widgets.
    pub fn caption(&mut self, text: &str, color: Vec4) {
        let width = self.widget.measure(text, SMALL_FONT_SIZE, false);
        let node = self.styled_text(None, text, color, Align::Start, SMALL_FONT_SIZE, false);
        self.push(node, Vec2::new(self.fill_width(width), self.px(SMALL_FONT_SIZE * 1.5)));
    }

    /// Empty room `px` logical pixels along the layout.
    pub fn space(&mut self, px: f32) {
        let px = self.px(px);
        let size = if self.row { Vec2::new(px, 1.0) } else { Vec2::new(1.0, px) };
        self.push(UINode::default(), size);
    }

    /// A line across a column, or a divider between the widgets of a row.
    pub fn separator(&mut self) {
        // thinner lines vanish when they land between pixel centers
        let theme = self.widget.theme;
        let mut node = UINode::default();
        node.set_background(Background::Color(theme.panel.lerp(theme.border, 0.8)));
        let size = if self.row { Vec2::new(2.0, self.row_height()) } else { Vec2::new(self.width, 2.0) };
        self.push(node, size);
    }

    /// Lays out `build`'s widgets left to right.
    pub fn row(&mut self, build: impl FnOnce(&mut Ui)) {
        let mut row = self.child(true, 0.0);
        build(&mut row);
        let (node, size) = (row.node, Vec2::new(row.extent, row.cross));
        self.push(node, Vec2::new(self.fill_width(size.x), size.y));
    }

    /// A key-like button with `text` on it, `width` wide. It sinks into its lip while held.
    fn key(&mut self, id: String, text: &str, width: f32, tone: Tone) -> bool {
        let theme = self.widget.theme;
        let (hot, held) = (self.widget.is_hot(&id), self.widget.dragging(&id) && self.widget.is_hot(&id));
        let clicked = self.widget.clicked(&id);
        let (top, ink) = match tone {
            Tone::Normal => (if hot { theme.hover } else { theme.widget }, theme.text),
            Tone::Danger => (if hot { theme.hover } else { theme.widget }, theme.error),
            Tone::Primary => (if hot { theme.accent * Vec4::new(1.2, 1.2, 1.2, 1.0) } else { theme.accent }, theme.on_accent)
        };
        let (radius, lip) = (self.px(RADIUS), self.px(LIP));
        let mut face = self.text_node(None, text, ink, Align::Center);
        fill(&mut face, top, radius);
        face.set_height(Val::Px(self.row_height() - lip));
        if held { face.set_margin(Rect::new_top(Val::Px(lip))); }

        let mut node = UINode::new(id);
        let base = if tone == Tone::Primary { theme.accent * Vec4::new(0.45, 0.45, 0.45, 1.0) } else { theme.border };
        fill(&mut node, base, radius);
        node.add(face);
        self.push(node, Vec2::new(width, self.row_height()));
        clicked
    }

    pub fn button(&mut self, text: &str) -> bool {
        self.button_tone(text, Tone::Normal)
    }

    /// A button standing out as `tone` says.
    pub fn button_tone(&mut self, text: &str, tone: Tone) -> bool {
        self.key_button(text, text, tone)
    }

    /// A `button_tone` told apart from others by `key` rather than its text.
    pub fn key_button(&mut self, key: &str, text: &str, tone: Tone) -> bool {
        let width = self.fill_width(self.text_width(text) + self.px(4.0));
        self.key(self.id(key), text, width, tone)
    }

    /// A button showing `hint` next to the pointer while hovered.
    pub fn button_hint(&mut self, text: &str, hint: &str) -> bool {
        let clicked = self.button(text);
        self.hint(hint);
        clicked
    }

    /// Text highlighted while `selected`, `key` telling it apart from others.
    pub fn selectable(&mut self, key: &str, selected: bool, text: &str) -> bool {
        self.list_row(key, selected, text, None, None)
    }

    /// A list row: an accent marker while `selected`, a `swatch` of color before `text` and dim
    /// `trailing` text at its right end.
    pub fn list_row(&mut self, key: &str, selected: bool, text: &str, swatch: Option<Vec4>, trailing: Option<&str>) -> bool {
        let id = self.id(key);
        let theme = self.widget.theme;
        let (hot, clicked) = (self.widget.is_hot(&id), self.widget.clicked(&id));
        let height = self.row_height();

        let mut node = UINode::new(id);
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        fill(&mut node, if selected { theme.selected } else if hot { theme.hover } else { NONE }, self.px(RADIUS));

        let mut marker = UINode::default();
        marker.set_width(Val::Px(self.px(MARKER)));
        marker.set_height(Val::Px(height - self.px(8.0)));
        marker.set_margin(Rect::new_left(Val::Px(self.indent + self.px(2.0))));
        fill(&mut marker, if selected { theme.accent } else { NONE }, self.px(MARKER) / 2.0);
        node.add(marker);
        let mut natural = self.indent + self.px(2.0 + MARKER);

        if let Some(color) = swatch {
            let size = self.px(SWATCH);
            let mut chip = UINode::default();
            chip.set_width(Val::Px(size));
            chip.set_height(Val::Px(size));
            chip.set_margin(Rect::new_left(Val::Px(self.px(5.0))));
            fill(&mut chip, color, self.px(2.0));
            chip.set_border(Val::Px(1.0));
            chip.set_border_color(Some(theme.border));
            node.add(chip);
            natural += self.px(5.0) + size;
        }

        let mut label = self.styled_text(None, text, if selected || hot { theme.text } else { theme.text * Vec4::new(0.85, 0.85, 0.85, 1.0) }, Align::Start, FONT_SIZE, selected);
        label.set_padding(Rect::new_left(Val::Px(self.px(6.0))));
        natural += self.widget.measure(text, FONT_SIZE, selected) + self.px(6.0);
        if let Some(trailing) = trailing {
            let mut tail = self.styled_text(None, trailing, theme.dim, Align::End, SMALL_FONT_SIZE, false);
            let width = self.widget.measure(trailing, SMALL_FONT_SIZE, false) + self.px(TEXT_INSET);
            tail.set_width(Val::Px(width));
            tail.set_padding(Rect::new_right(Val::Px(self.px(TEXT_INSET))));
            node.add(label);
            node.add(tail);
            natural += width + self.px(GAP);
        } else {
            node.add(label);
        }
        natural += self.px(TEXT_INSET);
        self.push(node, Vec2::new(self.fill_width(natural), height));
        clicked
    }

    pub fn checkbox(&mut self, value: &mut bool, text: &str) -> bool {
        let id = self.id(text);
        let clicked = self.widget.clicked(&id);
        if clicked { *value = !*value; }
        let theme = self.widget.theme;

        let size = self.px(CHECK);
        let mut check = UINode::default();
        check.set_display(Display::FlexColumn { vertical: Align::Center, horizontal: Align::Center });
        check.set_width(Val::Px(size));
        check.set_height(Val::Px(size));
        fill(&mut check, theme.well, self.px(RADIUS));
        check.set_border(Val::Px(1.0));
        check.set_border_color(Some(if self.widget.is_hot(&id) { theme.dim } else { theme.border }));
        if *value {
            let mut tick = UINode::default();
            tick.set_width(Val::Px(size / 2.0));
            tick.set_height(Val::Px(size / 2.0));
            fill(&mut tick, theme.accent, self.px(1.0));
            check.add(tick);
        }
        self.labelled(id, check, size, text);
        clicked
    }

    /// A sliding switch, lit while `value` is on.
    pub fn toggle(&mut self, value: &mut bool, text: &str) -> bool {
        let id = self.id(text);
        let clicked = self.widget.clicked(&id);
        if clicked { *value = !*value; }
        let theme = self.widget.theme;

        let size = SWITCH * self.widget.scale;
        let mut track = UINode::default();
        track.set_display(Display::FlexRow { vertical: Align::Center, horizontal: if *value { Align::End } else { Align::Start } });
        track.set_width(Val::Px(size.x));
        track.set_height(Val::Px(size.y));
        track.set_padding(Rect::single(Val::Px(self.px(2.0))));
        fill(&mut track, if *value { theme.accent } else { theme.well }, size.y / 2.0);
        track.set_border(Val::Px(1.0));
        track.set_border_color(Some(if *value { theme.accent } else { theme.border }));
        let knob_size = size.y - self.px(4.0);
        let mut knob = UINode::default();
        knob.set_width(Val::Px(knob_size));
        knob.set_height(Val::Px(knob_size));
        let knob_color = if *value { theme.on_accent } else if self.widget.is_hot(&id) { theme.text } else { theme.dim };
        fill(&mut knob, knob_color, knob_size / 2.0);
        track.add(knob);
        self.labelled(id, track, size.x, text);
        clicked
    }

    /// `control` `width` wide then `text`, clicking either hitting `id`.
    fn labelled(&mut self, id: String, control: UINode, width: f32, text: &str) {
        let mut label = self.text_node(None, text, self.widget.theme.text, Align::Start);
        label.set_width(Val::Px(self.text_width(text)));
        label.set_height(Val::Px(self.row_height()));

        let mut node = UINode::new(id);
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        node.add(control);
        node.add(label);
        self.push(node, Vec2::new(self.fill_width(width + self.text_width(text)), self.row_height()));
    }

    /// A logarithmic slider over `range`, followed by its value and `suffix`.
    pub fn log_slider<T: SliderValue>(&mut self, key: &str, value: &mut T, range: RangeInclusive<T>, enabled: bool, suffix: &str) -> bool {
        let mut changed = false;
        self.row(|ui| {
            changed = ui.log_track(key, value, range, enabled, SLIDER);
            let text = format!("{}{suffix}", value.text());
            let color = if enabled { ui.widget.theme.text } else { ui.widget.theme.dim };
            ui.colored_label(&text, color);
        });
        changed
    }

    /// Just the groove and knob of a `log_slider`, `width` logical pixels wide.
    pub fn log_track<T: SliderValue>(&mut self, key: &str, value: &mut T, range: RangeInclusive<T>, enabled: bool, width: f32) -> bool {
        let id = self.id(key);
        let theme = self.widget.theme;
        let (lo, hi) = (range.start().to_f64().ln(), range.end().to_f64().ln());
        let mut changed = false;
        if enabled && (self.widget.clicked(&id) || self.widget.dragging(&id)) && let Some(area) = self.widget.rects.get(&id) {
            let t = ((self.widget.cursor.x - area.min.x) / area.size().x).clamp(0.0, 1.0) as f64;
            let new = T::from_f64((lo + (hi - lo) * t).exp());
            changed = new != *value;
            *value = new;
        }
        let t = ((value.to_f64().ln() - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;

        let (width, groove, knob) = (self.px(width), self.px(GROOVE), KNOB * self.widget.scale);
        let before = (t * (width - knob.x)).max(0.01);
        let after = (width - knob.x - before).max(0.01);
        let groove_node = |color: Vec4, length: f32| {
            let mut node = UINode::default();
            node.set_width(Val::Px(length));
            node.set_height(Val::Px(groove));
            fill(&mut node, color, groove / 2.0);
            node
        };

        let mut track = UINode::new(id.clone());
        track.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        track.add(groove_node(if enabled { theme.accent } else { theme.border }, before));
        let mut handle = UINode::default();
        handle.set_width(Val::Px(knob.x));
        handle.set_height(Val::Px(if enabled { knob.y } else { knob.y * 0.75 }));
        let lit = enabled && (self.widget.is_hot(&id) || self.widget.dragging(&id));
        fill(&mut handle, if !enabled { theme.border } else if lit { theme.text } else { theme.hover }, self.px(2.0));
        handle.set_border(Val::Px(1.0));
        handle.set_border_color(Some(theme.border));
        track.add(handle);
        track.add(groove_node(theme.well, after));
        self.push(track, Vec2::new(width, self.row_height()));
        changed
    }

    /// A seven segment display `digits` wide showing `text` right aligned, its segments lit in the
    /// accent while `lit`. Shows digits, spaces, `-`, `E` and `r`, a `.` lighting the decimal point
    /// of the digit before it.
    pub fn seven_segment(&mut self, text: &str, digits: usize, lit: bool) {
        let theme = self.widget.theme;
        let on = if lit { theme.accent } else { theme.well.lerp(theme.accent, 0.4) };
        let off = theme.well.lerp(theme.accent, 0.05);
        let mut cells: Vec<(char, bool)> = vec![];
        for c in text.chars() {
            match (c, cells.last_mut()) {
                ('.', Some((_, dp @ false))) => *dp = true,
                ('.', _) => cells.push((' ', true)),
                (c, _) => cells.push((c, false))
            }
        }
        let cells = cells.split_off(cells.len().saturating_sub(digits));

        let (size, t, gap) = (DIGIT * self.widget.scale, self.px(SEGMENT), self.px(2.0));
        let segment = |lit: bool, w: f32, h: f32, left: f32| {
            let mut node = UINode::default();
            node.set_width(Val::Px(w));
            node.set_height(Val::Px(h));
            node.set_margin(Rect::new_left(Val::Px(left)));
            fill(&mut node, if lit { on } else { off }, t / 2.0);
            node
        };
        let half = (size.y - 3.0 * t) / 2.0;
        let mut window = UINode::default();
        window.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::End });
        let inset = self.px(6.0);
        window.set_padding(Rect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(inset), Val::Px(inset - gap)));
        for (c, dp) in std::iter::repeat_n((' ', false), digits - cells.len()).chain(cells) {
            let bits = segments(c);
            let lit = |segment: u8| bits & segment != 0;
            let mut digit = UINode::default();
            digit.set_width(Val::Px(size.x));
            digit.set_height(Val::Px(size.y));
            let bar = |segment_bit| segment(lit(segment_bit), size.x - 2.0 * t, t, t);
            let sides = |left_bit, right_bit| {
                let mut row = UINode::default();
                row.set_display(Display::FlexRow { vertical: Align::Start, horizontal: Align::Start });
                row.set_width(Val::Px(size.x));
                row.set_height(Val::Px(half));
                row.add(segment(lit(left_bit), t, half, 0.0));
                row.add(segment(lit(right_bit), t, half, size.x - 2.0 * t));
                row
            };
            digit.add(bar(SEG_A));
            digit.add(sides(SEG_F, SEG_B));
            digit.add(bar(SEG_G));
            digit.add(sides(SEG_E, SEG_C));
            digit.add(bar(SEG_D));
            window.add(digit);

            let mut point = UINode::default();
            point.set_display(Display::FlexColumn { vertical: Align::End, horizontal: Align::Center });
            point.set_width(Val::Px(t + gap));
            point.set_height(Val::Px(size.y));
            let mut dot = segment(dp, t, t, 0.0);
            dot.set_margin(Rect::new_left(Val::Px(gap / 2.0)));
            point.add(dot);
            window.add(point);
        }
        fill(&mut window, theme.well, self.px(RADIUS));
        window.set_border(Val::Px(1.0));
        window.set_border_color(Some(theme.border));
        let width = 2.0 * inset - gap + digits as f32 * (size.x + t + gap);
        self.push(window, Vec2::new(width, self.row_height()));
    }

    /// An indicator light in `color` while `lit`, with `text` after it. `key` names it for `hint`.
    pub fn led(&mut self, key: &str, lit: bool, color: Vec4, text: &str) {
        let theme = self.widget.theme;
        let (size, halo) = (self.px(LED), self.px(LED * 1.9));
        let mut glow = UINode::default();
        glow.set_display(Display::FlexColumn { vertical: Align::Center, horizontal: Align::Center });
        glow.set_width(Val::Px(halo));
        glow.set_height(Val::Px(halo));
        fill(&mut glow, if lit { color * Vec4::new(1.0, 1.0, 1.0, 0.22) } else { NONE }, halo / 2.0);
        let mut dot = UINode::default();
        dot.set_width(Val::Px(size));
        dot.set_height(Val::Px(size));
        fill(&mut dot, if lit { color } else { theme.well }, size / 2.0);
        dot.set_border(Val::Px(1.0));
        dot.set_border_color(Some(if lit { color } else { theme.border }));
        glow.add(dot);

        let mut label = self.styled_text(None, text, if lit { theme.text } else { theme.dim }, Align::Start, FONT_SIZE, false);
        label.set_padding(Rect::new_left(Val::Px(self.px(4.0))));
        let label_width = self.widget.measure(text, FONT_SIZE, false) + self.px(4.0);
        label.set_width(Val::Px(label_width));

        let mut node = UINode::new(self.id(key));
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        node.add(glow);
        node.add(label);
        self.push(node, Vec2::new(self.fill_width(halo + label_width), self.row_height()));
    }

    /// A key cap reading `text`, for showing which input does what.
    pub fn keycap(&mut self, text: &str) {
        let theme = self.widget.theme;
        let width = self.widget.measure(text, SMALL_FONT_SIZE, true) + self.px(10.0);
        let height = self.px(SMALL_FONT_SIZE + 7.0);
        let mut face = self.styled_text(None, text, theme.text, Align::Center, SMALL_FONT_SIZE, true);
        fill(&mut face, theme.widget, self.px(RADIUS));
        face.set_height(Val::Px(height - self.px(LIP)));
        let mut node = UINode::default();
        fill(&mut node, theme.border, self.px(RADIUS));
        node.add(face);
        let mut slot = UINode::default();
        slot.set_display(Display::FlexColumn { vertical: Align::Center, horizontal: Align::Start });
        node.set_width(Val::Px(width));
        node.set_height(Val::Px(height));
        slot.add(node);
        self.push(slot, Vec2::new(self.fill_width(width), self.row_height()));
    }

    /// Picks one of `options` laid out as keys in a recessed strip, the picked one raised.
    /// Returns the index clicked this frame.
    pub fn segmented(&mut self, key: &str, options: &[&str], selected: usize) -> Option<usize> {
        let theme = self.widget.theme;
        let inset = self.px(2.0);
        let height = self.row_height();
        let mut clicked = None;
        let mut strip = UINode::default();
        strip.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        strip.set_padding(Rect::single(Val::Px(inset)));
        fill(&mut strip, theme.well, self.px(RADIUS));
        strip.set_border(Val::Px(1.0));
        strip.set_border_color(Some(theme.border));
        let mut width = 2.0 * inset;
        for (idx, option) in options.iter().enumerate() {
            let id = self.id(&format!("{key}:{idx}"));
            let hot = self.widget.is_hot(&id);
            if self.widget.clicked(&id) { clicked = Some(idx); }
            let on = idx == selected;
            let color = if on || hot { theme.text } else { theme.dim };
            let mut node = self.styled_text(Some(id), option, color, Align::Center, FONT_SIZE, on);
            let option_width = self.widget.measure(option, FONT_SIZE, true) + 2.0 * self.px(TEXT_INSET);
            node.set_width(Val::Px(option_width));
            node.set_height(Val::Px(height - 2.0 * inset));
            fill(&mut node, if on { theme.hover } else { NONE }, self.px(RADIUS - 1.0));
            strip.add(node);
            width += option_width;
        }
        self.push(strip, Vec2::new(width, height));
        clicked
    }

    /// A single line text field `width` logical pixels wide, editing `value` as it's typed.
    pub fn text_field(&mut self, key: &str, value: &mut String, hint: &str, width: f32) -> bool {
        let id = self.id(key);
        if self.widget.clicked(&id) && self.widget.focused(&id).is_none() {
            self.widget.focus = Some((id.clone(), value.clone()));
        }
        let focused = self.widget.focused(&id).map(str::to_string);
        let changed = focused.as_ref().is_some_and(|text| text != value);
        if let Some(text) = focused.as_ref().filter(|_| changed) { *value = text.clone(); }

        let (shown, color) = match focused {
            Some(text) => (format!("{text}|"), self.widget.theme.text),
            None if value.is_empty() => (hint.to_string(), self.widget.theme.dim),
            None => (value.clone(), self.widget.theme.text)
        };
        self.edit_box(id, &shown, color, width);
        changed
    }

    /// A field showing `text` until clicked, then editing a fresh text that Enter or a click
    /// elsewhere hands back.
    pub fn number_field(&mut self, key: &str, text: &str, width: f32) -> Option<String> {
        let id = self.id(key);
        if self.widget.clicked(&id) && self.widget.focused(&id).is_none() {
            self.widget.focus = Some((id.clone(), String::new()));
        }
        let (shown, color) = match self.widget.focused(&id) {
            Some("") => (format!("{text}|"), self.widget.theme.dim),
            Some(typed) => (format!("{typed}|"), self.widget.theme.text),
            None => (text.to_string(), self.widget.theme.text)
        };
        self.edit_box(id.clone(), &shown, color, width);
        self.widget.committed.take_if(|(committed, _)| *committed == id).map(|(_, typed)| typed)
    }

    fn edit_box(&mut self, id: String, shown: &str, color: Vec4, width: f32) {
        let theme = self.widget.theme;
        let focused = self.widget.focused(&id).is_some();
        let hot = self.widget.is_hot(&id);
        let width = self.fill_width(self.px(width));
        // the end of long text, where typing happens
        let room = width - 2.0 * self.px(TEXT_INSET);
        let mut start = 0;
        while self.widget.text_width(&shown[start..]) > room && let Some(c) = shown[start..].chars().next() {
            start += c.len_utf8();
        }
        let mut node = self.text_node(Some(id), &shown[start..], color, Align::Start);
        fill(&mut node, theme.well, self.px(RADIUS));
        node.set_border(Val::Px(1.0));
        node.set_border_color(Some(if focused { theme.accent } else if hot { theme.dim } else { theme.border }));
        self.push(node, Vec2::new(width, self.row_height()));
    }

    /// A collapsing header row, clicking it opens or closes the section `key` names.
    pub fn header(&mut self, key: &str, text: &str, default_open: bool) {
        self.header_count(key, text, default_open, None);
    }

    /// A `header` showing how many entries its section holds at its right end.
    pub fn header_count(&mut self, key: &str, text: &str, default_open: bool, count: Option<usize>) {
        let open = self.is_open(key, default_open);
        let id = self.id(key);
        let theme = self.widget.theme;
        let hot = self.widget.is_hot(&id);
        let height = self.row_height();

        let mut node = UINode::new(id.clone());
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        fill(&mut node, if hot { theme.hover } else { NONE }, self.px(RADIUS));
        let sign_width = self.px(14.0);
        let mut sign = self.styled_text(None, if open { "\u{2212}" } else { "+" }, theme.dim, Align::Center, FONT_SIZE, true);
        sign.set_width(Val::Px(sign_width));
        sign.set_margin(Rect::new_left(Val::Px(self.indent + self.px(1.0))));
        node.add(sign);
        let mut title = self.styled_text(None, text, if hot { theme.text } else { theme.dim }, Align::Start, FONT_SIZE, true);
        title.set_padding(Rect::new_left(Val::Px(self.px(4.0))));
        node.add(title);
        let mut natural = self.indent + self.px(1.0) + sign_width + self.px(4.0) + self.widget.measure(text, FONT_SIZE, true);
        if let Some(count) = count {
            let count = count.to_string();
            let width = self.widget.measure(&count, SMALL_FONT_SIZE, false) + self.px(TEXT_INSET);
            let mut tail = self.styled_text(None, &count, theme.dim, Align::End, SMALL_FONT_SIZE, false);
            tail.set_width(Val::Px(width));
            tail.set_padding(Rect::new_right(Val::Px(self.px(TEXT_INSET))));
            node.add(tail);
            natural += width;
        }
        self.push(node, Vec2::new(self.fill_width(natural + self.px(TEXT_INSET)), height));
        if self.widget.clicked(&id) {
            self.widget.open.insert(id, !open);
        }
    }

    /// Shows up to `visible` of `rows` rows at a time, `build` adding each shown row by index,
    /// scrolled by the wheel while hovered.
    pub fn scroll_rows(&mut self, key: &str, rows: usize, visible: usize, mut build: impl FnMut(&mut Ui, usize)) {
        let id = self.id(key);
        let shown = rows.min(visible);
        let hovered = self.widget.rects.get(&id).is_some_and(|area| area.contains(self.widget.cursor));
        let wheel = if hovered { self.widget.wheel } else { 0.0 };
        let first = self.widget.scrolls.entry(id.clone()).or_default();
        *first = (*first - wheel * ROWS_PER_NOTCH).clamp(0.0, (rows - shown) as f32);
        let first = *first as usize;

        let (bar, gap) = (self.px(SCROLLBAR), self.px(GAP));
        let mut list = self.child(false, self.width - bar - gap);
        for idx in first..first + shown { build(&mut list, idx); }
        let (mut content, width, height) = (list.node, list.width, list.extent);
        content.set_width(Val::Px(width));
        content.set_height(Val::Px(height.max(0.01)));

        let mut area = UINode::new(id);
        area.set_display(Display::FlexRow { vertical: Align::Start, horizontal: Align::Start });
        area.add(content);
        if shown < rows {
            let mut track = UINode::default();
            track.set_width(Val::Px(bar));
            track.set_height(Val::Px(height));
            track.set_margin(Rect::new_left(Val::Px(gap)));
            fill(&mut track, self.widget.theme.well, bar / 2.0);
            let mut thumb = UINode::default();
            thumb.set_height(Val::Px(height * shown as f32 / rows as f32));
            thumb.set_margin(Rect::new_top(Val::Px(height * first as f32 / rows as f32)));
            fill(&mut thumb, if hovered { self.widget.theme.dim } else { self.widget.theme.border }, bar / 2.0);
            track.add(thumb);
            area.add(track);
        }
        self.push(area, Vec2::new(self.width, height));
    }

    /// A scrolling list of collapsing `sections` (title, entries, open at first), `entry` adding
    /// a row for each entry of an open section. Empty sections are left out.
    pub fn sections<T>(&mut self, key: &str, sections: &[(&str, Vec<T>, bool)], visible: usize, mut entry: impl FnMut(&mut Ui, &T)) {
        let header_key = |title: &str| format!("{key}:{title}");
        let mut rows = vec![];
        for (idx, (title, entries, open)) in sections.iter().enumerate() {
            if entries.is_empty() { continue }
            rows.push((idx, None));
            if self.is_open(&header_key(title), *open) { rows.extend((0..entries.len()).map(|i| (idx, Some(i)))); }
        }
        let indent = self.px(INDENT);
        self.scroll_rows(key, rows.len(), visible, |ui, row| {
            let (title, entries, open) = &sections[rows[row].0];
            match rows[row].1 {
                None => ui.header_count(&header_key(title), title, *open, Some(entries.len())),
                Some(i) => {
                    ui.indent += indent;
                    entry(ui, &entries[i]);
                    ui.indent -= indent;
                }
            }
        });
    }
}

// Seven segment bits, clockwise from the top then the middle bar.
const SEG_A: u8 = 1;
const SEG_B: u8 = 2;
const SEG_C: u8 = 4;
const SEG_D: u8 = 8;
const SEG_E: u8 = 16;
const SEG_F: u8 = 32;
const SEG_G: u8 = 64;

/// Segments lit to show `c`.
fn segments(c: char) -> u8 {
    match c {
        '0' | 'O' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_E | SEG_F,
        '1' => SEG_B | SEG_C,
        '2' => SEG_A | SEG_B | SEG_D | SEG_E | SEG_G,
        '3' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_G,
        '4' => SEG_B | SEG_C | SEG_F | SEG_G,
        '5' | 'S' => SEG_A | SEG_C | SEG_D | SEG_F | SEG_G,
        '6' => SEG_A | SEG_C | SEG_D | SEG_E | SEG_F | SEG_G,
        '7' => SEG_A | SEG_B | SEG_C,
        '8' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_E | SEG_F | SEG_G,
        '9' => SEG_A | SEG_B | SEG_C | SEG_D | SEG_F | SEG_G,
        'A' | 'a' => SEG_A | SEG_B | SEG_C | SEG_E | SEG_F | SEG_G,
        'B' | 'b' => SEG_C | SEG_D | SEG_E | SEG_F | SEG_G,
        'C' => SEG_A | SEG_D | SEG_E | SEG_F,
        'D' | 'd' => SEG_B | SEG_C | SEG_D | SEG_E | SEG_G,
        'E' => SEG_A | SEG_D | SEG_E | SEG_F | SEG_G,
        'F' => SEG_A | SEG_E | SEG_F | SEG_G,
        'r' => SEG_E | SEG_G,
        '-' => SEG_G,
        _ => 0
    }
}

/// Values a `Ui::log_slider` can edit.
pub trait SliderValue: Copy + PartialEq {
    fn to_f64(self) -> f64;
    fn from_f64(v: f64) -> Self;
    fn text(self) -> String;
}

impl SliderValue for u32 {
    fn to_f64(self) -> f64 { self as f64 }
    fn from_f64(v: f64) -> Self { v.round() as u32 }
    fn text(self) -> String { self.to_string() }
}

impl SliderValue for f64 {
    fn to_f64(self) -> f64 { self }
    fn from_f64(v: f64) -> Self { v }
    fn text(self) -> String { format!("{self:.2}") }
}

fn fill(node: &mut UINode, color: Vec4, radius: f32) {
    if color == NONE { return }
    node.set_background(Background::Color(color));
    node.set_border_radius(RectCorners::single(Val::Px(radius)));
}

/// Screen rectangle a node was laid out in.
#[derive(Clone, Copy)]
struct Area { min: Vec2, max: Vec2 }

impl Area {
    fn of(node: &UINode) -> Option<Self> {
        let state = *node.last_state()?;
        Some(Self { min: state.position - state.size / 2.0, max: state.position + state.size / 2.0 })
    }

    fn contains(&self, p: Vec2) -> bool { p.cmpge(self.min).all() && p.cmple(self.max).all() }

    fn size(&self) -> Vec2 { self.max - self.min }
}

/// The OS clipboard, through the window's own Wayland connection there like egui-winit.
enum Clipboard {
    #[cfg(target_os = "linux")]
    Wayland(smithay_clipboard::Clipboard),
    Arboard(arboard::Clipboard)
}

impl Clipboard {
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn new(display: Option<RawDisplayHandle>) -> Option<Self> {
        #[cfg(target_os = "linux")]
        if let Some(RawDisplayHandle::Wayland(display)) = display {
            // SAFETY: the display is the window's, which lives as long as the app
            return Some(Self::Wayland(unsafe { smithay_clipboard::Clipboard::new(display.display.as_ptr()) }))
        }
        arboard::Clipboard::new().map(Self::Arboard).inspect_err(|err| eprintln!("No clipboard: {err}")).ok()
    }

    fn text(&mut self) -> Option<String> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Wayland(clipboard) => clipboard.load().ok(),
            Self::Arboard(clipboard) => clipboard.get_text().ok()
        }
    }
}

#[system]
fn setup_widgets(widget: ResMut<Widget>, graphics: Res<Graphics>) {
    let display = graphics.window().display_handle().ok().map(|handle| handle.as_raw());
    widget.clipboard = Clipboard::new(display).map(Mutex::new);
    world.insert(EntityBuilder::default().add(UINodeSDFRoot::default()).build());
}

static WIDGET_EVENT_TRACKER: EventSystemMinIDTracker = EventSystemMinIDTracker::new();

/// Reads this frame's input and hit tests it against last frame's layout.
#[system(std::i32::MIN + 1)]
fn widgets_begin(widget: ResMut<Widget>, window_events: Event<WindowEvent>, graphics: Res<Graphics>, window: Res<WindowDimensions>) {
    widget.scale = graphics.window().scale_factor() as f32;
    widget.screen = Vec2::new(window.x as f32, window.y as f32);
    let last_cursor = widget.cursor;
    widget.pressed = false;
    widget.right_pressed = false;
    widget.wheel = 0.0;
    widget.committed = None;
    for event in window_events.read(&WIDGET_EVENT_TRACKER) {
        match &event.0 {
            winit::event::WindowEvent::CursorMoved { position, .. } => widget.cursor = Vec2::new(position.x as f32, position.y as f32),
            winit::event::WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                widget.down = *state == ElementState::Pressed;
                widget.pressed |= widget.down;
            }
            winit::event::WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => widget.right_pressed = true,
            winit::event::WindowEvent::MouseWheel { delta, .. } => widget.wheel += match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / PX_PER_NOTCH
            },
            winit::event::WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => widget.key(event),
            _ => {}
        }
    }
    widget.cursor_delta = widget.cursor - last_cursor;

    // a window over the pointer hides the widgets of the windows under it
    widget.rects.clear();
    widget.hot = None;
    widget.over_ui = false;
    widget.over_window = None;
    let laid_out = std::mem::take(&mut widget.laid_out);
    for (idx, window) in laid_out.children().iter().enumerate() {
        if Area::of(window).is_some_and(|area| area.contains(widget.cursor)) {
            widget.over_ui = true;
            widget.over_window = widget.laid_out_ids.get(idx).cloned();
            widget.hot = None;
        }
        widget.collect(window);
    }

    if widget.pressed {
        widget.active = widget.hot.clone();
        if widget.focus.as_ref().is_some_and(|(focus, _)| widget.hot.as_ref() != Some(focus)) {
            widget.committed = widget.focus.take();
        }
    }
    if !widget.down { widget.active = None; }
}

/// Hands this frame's windows to iso_ui, which lays them out and draws them.
#[system(std::i32::MAX / 2 - 1)]
fn widgets_end(widget: ResMut<Widget>, roots: Query<&mut UINodeSDFRoot>) {
    if let Some(hint) = widget.tooltip.take() {
        let pos = widget.cursor + Vec2::splat(widget.px(16.0));
        let width = widget.text_width(&hint) / widget.scale + 2.0 * (PAD + TEXT_INSET);
        widget.popup("tooltip", pos, width, |ui| ui.label(&hint));
    }
    let mut root = UINode::default();
    let (huds, panels, popups) = (std::mem::take(&mut widget.huds), std::mem::take(&mut widget.panels), std::mem::take(&mut widget.popups));
    let (ids, windows): (Vec<_>, Vec<_>) = huds.into_iter().chain(panels).chain(popups).unzip();
    root.add_all(windows.into_iter());
    for mut node in roots.as_iter() {
        node.0 = root.clone();
    }
    widget.laid_out = root;
    widget.laid_out_ids = ids;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_to_linear() {
        assert_eq!(srgb(0x000000), Vec4::new(0.0, 0.0, 0.0, 1.0));
        assert_eq!(srgb(0xFFFFFF), Vec4::ONE);
        assert!((srgb(0x808080).x - 0.2158).abs() < 1e-3);
    }

    #[test]
    fn digit_segments() {
        assert_eq!(segments('8').count_ones(), 7);
        assert_eq!(segments('1'), SEG_B | SEG_C);
        assert_eq!(segments(' '), 0);
    }
}
