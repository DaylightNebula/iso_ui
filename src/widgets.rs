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

// Sizes in logical pixels, scaled by the window's scale factor.
const FONT_SIZE: f32 = 14.0;
const ROW: f32 = 22.0;
const PAD: f32 = 6.0;
const GAP: f32 = 4.0;
const TEXT_INSET: f32 = 6.0;
const INDENT: f32 = 14.0;
const RADIUS: f32 = 4.0;
const CHECK: f32 = 14.0;
const SLIDER: f32 = 120.0;
const SCROLLBAR: f32 = 6.0;
/// List rows a wheel notch scrolls.
const ROWS_PER_NOTCH: f32 = 2.0;
/// Pixels of touchpad scroll treated as one wheel notch.
const PX_PER_NOTCH: f32 = 50.0;

const NONE: Vec4 = Vec4::ZERO;

/// Widget colors. They're linear, so pick them for an sRGB surface.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub panel: Vec4,
    pub border: Vec4,
    pub widget: Vec4,
    pub hover: Vec4,
    pub selected: Vec4,
    pub text: Vec4,
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
            selected: Vec4::new(0.033, 0.089, 0.214, 1.0),
            text: Vec4::new(0.694, 0.694, 0.694, 1.0),
            dim: Vec4::new(0.214, 0.214, 0.214, 1.0),
            error: Vec4::new(1.0, 0.214, 0.214, 1.0)
        }
    }
}

/// Adds the `Widget` resource, drawing its widgets in `font`. Needs the `UIPlugin` to draw them.
pub struct WidgetPlugin {
    font: Arc<SDFFont>
}

impl WidgetPlugin {
    pub fn new(font: Arc<SDFFont>) -> Self { Self { font } }
}

impl Plugin for WidgetPlugin {
    fn build(self, app: App) -> App {
        app.add_resource(Widget { font: Some(self.font), ..Default::default() })
            .on_render_startup(setup_widgets)
            .on_render_update(widgets_begin)
            .on_render_update(widgets_end)
    }
}

/// Pointer, keyboard and widget state shared by every window, plus the windows built this frame.
#[derive(Resource, Default)]
pub struct Widget {
    font: Option<Arc<SDFFont>>,
    theme: Theme,
    clipboard: Option<Mutex<Clipboard>>,
    scale: f32,
    screen: Vec2,
    cursor: Vec2,
    cursor_delta: Vec2,
    down: bool,
    /// Left button went down this frame.
    pressed: bool,
    /// Wheel notches this frame, positive scrolls up.
    wheel: f32,
    over_ui: bool,
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
    panels: Vec<UINode>,
    /// Drawn over the panels.
    popups: Vec<UINode>,
    /// Last frame's tree, laid out by iso_ui since.
    laid_out: UINode
}

impl Widget {
    pub fn set_font(&mut self, font: Arc<SDFFont>) { self.font = Some(font); }

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
        let title_id = format!("{title}/title");
        let pos = self.positions.entry(title.to_string()).or_insert(pos);
        if self.down && self.active.as_ref() == Some(&title_id) { *pos += self.cursor_delta; }
        let pos = *pos;
        self.window(title, Some(title_id), pos, width, false, build);
    }

    /// An untitled window at `pos` drawn over the panels.
    pub fn popup(&mut self, id: &str, pos: Vec2, width: f32, build: impl FnOnce(&mut Ui)) {
        self.window(id, None, pos, width, true, build);
    }

    fn window(&mut self, id: &str, title_id: Option<String>, pos: Vec2, width: f32, popup: bool, build: impl FnOnce(&mut Ui)) {
        let (width, pad) = (self.px(width), self.px(PAD));
        let mut ui = Ui::new(self, id.to_string(), width - 2.0 * pad);
        if let Some(title_id) = title_id {
            let mut title = ui.text_node(Some(title_id), id, ui.widget.theme.text, Align::Start);
            title.set_padding(Rect::default());
            ui.push(title, Vec2::new(ui.width, ui.widget.px(ROW)));
            ui.separator();
        }
        build(&mut ui);
        let (mut node, height) = (ui.node, ui.extent + 2.0 * pad);

        // keep the whole window on screen
        let pos = pos.min(self.screen - Vec2::new(width, height)).max(Vec2::ZERO);
        node.set_position_type(PositionType::Absolute(Rect::new_top_left(Val::Px(pos.y), Val::Px(pos.x))));
        node.set_width(Val::Px(width));
        node.set_height(Val::Px(height));
        node.set_padding(Rect::single(Val::Px(pad)));
        fill(&mut node, self.theme.panel, self.px(RADIUS));
        node.set_border(Val::Px(1.0));
        node.set_border_color(Some(self.theme.border));
        if popup { self.popups.push(node) } else { self.panels.push(node) }
    }

    fn text_width(&self, text: &str) -> f32 {
        self.font.as_ref().map_or(0.0, |font| font.measure_text(text, self.px(FONT_SIZE)).x)
    }

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
    indent: f32
}

impl<'a> Ui<'a> {
    fn new(widget: &'a mut Widget, scope: String, width: f32) -> Self {
        Self { widget, scope, node: UINode::default(), row: false, width, extent: 0.0, cross: 0.0, indent: 0.0 }
    }

    fn child(&mut self, row: bool, width: f32) -> Ui<'_> {
        let mut node = UINode::default();
        if row { node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start }); }
        Ui { widget: &mut *self.widget, scope: self.scope.clone(), node, row, width, extent: 0.0, cross: 0.0, indent: self.indent }
    }

    pub fn clipboard_text(&self) -> Option<String> { self.widget.clipboard_text() }

    pub fn theme(&self) -> &Theme { &self.widget.theme }

    pub fn is_open(&self, key: &str, default: bool) -> bool {
        self.widget.open.get(&self.id(key)).copied().unwrap_or(default)
    }

    fn id(&self, key: &str) -> String { format!("{}/{key}", self.scope) }

    fn row_height(&self) -> f32 { self.widget.px(ROW) }

    /// A child's width: what it needs in a row, all of a column.
    fn fill_width(&self, natural: f32) -> f32 { if self.row { natural } else { self.width } }

    fn text_width(&self, text: &str) -> f32 { self.widget.text_width(text) + 2.0 * self.widget.px(TEXT_INSET) + self.indent }

    /// Adds `node` sized `size` after the children so far.
    fn push(&mut self, mut node: UINode, size: Vec2) {
        let gap = if self.node.children().is_empty() { 0.0 } else { self.widget.px(GAP) };
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
        self.node.add(node);
    }

    fn text_node(&self, id: Option<String>, content: &str, color: Vec4, align: Align) -> UINode {
        let mut node = id.map_or_else(UINode::default, UINode::new);
        let inset = self.widget.px(TEXT_INSET);
        node.set_padding(Rect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(inset + self.indent), Val::Px(inset)));
        node.set_text(self.widget.font.clone().map(|font| Text {
            font,
            content: content.to_string(),
            color,
            font_size: self.widget.px(FONT_SIZE),
            horizontal_align: align,
            vertical_align: Align::Center
        }));
        node
    }

    pub fn label(&mut self, text: &str) { self.colored_label(text, self.widget.theme.text) }

    pub fn colored_label(&mut self, text: &str, color: Vec4) {
        let node = self.text_node(None, text, color, Align::Start);
        self.push(node, Vec2::new(self.fill_width(self.text_width(text)), self.row_height()));
    }

    /// Right aligned text `width` logical pixels wide, for columns of numbers.
    pub fn label_right(&mut self, text: &str, width: f32) {
        let node = self.text_node(None, text, self.widget.theme.text, Align::End);
        self.push(node, Vec2::new(self.widget.px(width), self.row_height()));
    }

    pub fn separator(&mut self) {
        let mut node = UINode::default();
        node.set_background(Background::Color(self.widget.theme.border));
        self.push(node, Vec2::new(self.fill_width(1.0), 1.0));
    }

    /// Lays out `build`'s widgets left to right.
    pub fn row(&mut self, build: impl FnOnce(&mut Ui)) {
        let mut row = self.child(true, 0.0);
        build(&mut row);
        let (node, size) = (row.node, Vec2::new(row.extent, row.cross));
        self.push(node, Vec2::new(self.fill_width(size.x), size.y));
    }

    fn clickable(&mut self, id: String, text: &str, align: Align, width: f32, idle: Vec4, selected: bool) -> bool {
        let hot = self.widget.is_hot(&id);
        let clicked = self.widget.clicked(&id);
        let mut node = self.text_node(Some(id), text, self.widget.theme.text, align);
        fill(&mut node, if selected { self.widget.theme.selected } else if hot { self.widget.theme.hover } else { idle }, self.widget.px(RADIUS));
        self.push(node, Vec2::new(width, self.row_height()));
        clicked
    }

    pub fn button(&mut self, text: &str) -> bool {
        let width = self.fill_width(self.text_width(text));
        self.clickable(self.id(text), text, Align::Center, width, self.widget.theme.widget, false)
    }

    /// A button showing `hint` next to the pointer while hovered.
    pub fn button_hint(&mut self, text: &str, hint: &str) -> bool {
        if self.widget.is_hot(&self.id(text)) { self.widget.tooltip = Some(hint.to_string()); }
        self.button(text)
    }

    /// Text highlighted while `selected`, `key` telling it apart from others.
    pub fn selectable(&mut self, key: &str, selected: bool, text: &str) -> bool {
        let width = self.fill_width(self.text_width(text));
        self.clickable(self.id(key), text, Align::Start, width, NONE, selected)
    }

    pub fn checkbox(&mut self, value: &mut bool, text: &str) -> bool {
        let id = self.id(text);
        let clicked = self.widget.clicked(&id);
        if clicked { *value = !*value; }

        let size = self.widget.px(CHECK);
        let mut check = UINode::default();
        check.set_display(Display::FlexColumn { vertical: Align::Center, horizontal: Align::Center });
        check.set_width(Val::Px(size));
        check.set_height(Val::Px(size));
        fill(&mut check, if self.widget.is_hot(&id) { self.widget.theme.hover } else { self.widget.theme.widget }, self.widget.px(RADIUS));
        if *value {
            let mut tick = UINode::default();
            tick.set_width(Val::Px(size / 2.0));
            tick.set_height(Val::Px(size / 2.0));
            fill(&mut tick, self.widget.theme.text, self.widget.px(1.0));
            check.add(tick);
        }
        let mut label = self.text_node(None, text, self.widget.theme.text, Align::Start);
        label.set_width(Val::Px(self.text_width(text)));
        label.set_height(Val::Px(self.row_height()));

        let mut node = UINode::new(id);
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        node.add(check);
        node.add(label);
        self.push(node, Vec2::new(self.fill_width(size + self.text_width(text)), self.row_height()));
        clicked
    }

    /// A logarithmic slider over `range`, followed by its value and `suffix`.
    pub fn log_slider<T: SliderValue>(&mut self, key: &str, value: &mut T, range: RangeInclusive<T>, enabled: bool, suffix: &str) -> bool {
        let id = self.id(key);
        let (lo, hi) = (range.start().to_f64().ln(), range.end().to_f64().ln());
        let mut changed = false;
        if enabled && (self.widget.clicked(&id) || self.widget.dragging(&id)) && let Some(area) = self.widget.rects.get(&id) {
            let t = ((self.widget.cursor.x - area.min.x) / area.size().x).clamp(0.0, 1.0) as f64;
            let new = T::from_f64((lo + (hi - lo) * t).exp());
            changed = new != *value;
            *value = new;
        }
        let t = ((value.to_f64().ln() - lo) / (hi - lo)).clamp(0.0, 1.0) as f32;

        let (width, height) = (self.widget.px(SLIDER), self.row_height() * 0.6);
        let mut track = UINode::new(id.clone());
        track.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        track.set_width(Val::Px(width));
        track.set_height(Val::Px(height));
        fill(&mut track, if enabled && self.widget.is_hot(&id) { self.widget.theme.hover } else { self.widget.theme.widget }, self.widget.px(RADIUS));
        let mut bar = UINode::default();
        bar.set_width(Val::Px((t * width).max(1.0)));
        bar.set_height(Val::Px(height));
        fill(&mut bar, if enabled { self.widget.theme.selected } else { self.widget.theme.border }, self.widget.px(RADIUS));
        track.add(bar);

        let text = format!("{}{suffix}", value.text());
        let mut label = self.text_node(None, &text, if enabled { self.widget.theme.text } else { self.widget.theme.dim }, Align::Start);
        label.set_width(Val::Px(self.text_width(&text)));
        label.set_height(Val::Px(self.row_height()));

        let mut node = UINode::default();
        node.set_display(Display::FlexRow { vertical: Align::Center, horizontal: Align::Start });
        node.add(track);
        node.add(label);
        self.push(node, Vec2::new(self.fill_width(width + self.text_width(&text)), self.row_height()));
        changed
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
        let focused = self.widget.focused(&id).is_some();
        let width = self.widget.px(width);
        // the end of long text, where typing happens
        let room = width - 2.0 * self.widget.px(TEXT_INSET);
        let mut start = 0;
        while self.widget.text_width(&shown[start..]) > room && let Some(c) = shown[start..].chars().next() {
            start += c.len_utf8();
        }
        let mut node = self.text_node(Some(id), &shown[start..], color, Align::Start);
        fill(&mut node, if focused { self.widget.theme.panel } else { self.widget.theme.widget }, self.widget.px(RADIUS));
        node.set_border(Val::Px(1.0));
        node.set_border_color(Some(if focused { self.widget.theme.selected } else { self.widget.theme.widget }));
        self.push(node, Vec2::new(width, self.row_height()));
    }

    /// A collapsing header row, clicking it opens or closes the section `key` names.
    pub fn header(&mut self, key: &str, text: &str, default_open: bool) {
        let open = self.is_open(key, default_open);
        let text = format!("{} {text}", if open { "-" } else { "+" });
        let width = self.fill_width(self.text_width(&text));
        let id = self.id(key);
        if self.clickable(id.clone(), &text, Align::Start, width, NONE, false) {
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

        let (bar, gap) = (self.widget.px(SCROLLBAR), self.widget.px(GAP));
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
            fill(&mut track, self.widget.theme.widget, bar / 2.0);
            let mut thumb = UINode::default();
            thumb.set_height(Val::Px(height * shown as f32 / rows as f32));
            thumb.set_margin(Rect::new_top(Val::Px(height * first as f32 / rows as f32)));
            fill(&mut thumb, self.widget.theme.dim, bar / 2.0);
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
        let indent = self.widget.px(INDENT);
        self.scroll_rows(key, rows.len(), visible, |ui, row| {
            let (title, entries, open) = &sections[rows[row].0];
            match rows[row].1 {
                None => ui.header(&header_key(title), title, *open),
                Some(i) => {
                    ui.indent += indent;
                    entry(ui, &entries[i]);
                    ui.indent -= indent;
                }
            }
        });
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
    widget.wheel = 0.0;
    widget.committed = None;
    for event in window_events.read(&WIDGET_EVENT_TRACKER) {
        match &event.0 {
            winit::event::WindowEvent::CursorMoved { position, .. } => widget.cursor = Vec2::new(position.x as f32, position.y as f32),
            winit::event::WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                widget.down = *state == ElementState::Pressed;
                widget.pressed |= widget.down;
            }
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
    let laid_out = std::mem::take(&mut widget.laid_out);
    for window in laid_out.children() {
        if Area::of(window).is_some_and(|area| area.contains(widget.cursor)) {
            widget.over_ui = true;
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
    let (panels, popups) = (std::mem::take(&mut widget.panels), std::mem::take(&mut widget.popups));
    root.add_all(panels.into_iter().chain(popups));
    for mut node in roots.as_iter() {
        node.0 = root.clone();
    }
    widget.laid_out = root;
}
