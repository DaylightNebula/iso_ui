# Iso UI

A signed-distance-field (SDF) based UI library for apps built on `anarchy` (ECS),
`cell` (app/plugin framework), and `magician-vgpu` (GPU abstraction over `wgpu`).

UI is described as a tree of `UINode`s with a CSS-flexbox-like `Style` (width/height,
margin/padding, flex row/column/grid, absolute positioning, borders, text). Each
frame the tree is laid out into `SDFElement`s, flattened into GPU buffers, and drawn
in a single fullscreen fragment-shader pass that rasterizes every shape (rectangles,
circles, bezier strokes, vector-font glyphs) as an SDF — no per-element draw calls or
meshes.

## Usage

Add the `UIPlugin` to your app and spawn an entity with a `UINodeSDFRoot` component:

```rust
use cell::App;
use iso_ui::*;

App::new()
    .add_plugin(UIPlugin)
    .on_render_startup(setup)
    .run()
```

```rust
#[system]
fn setup() {
    let mut root = UINode::new("root".to_string());
    root.set_display(Display::FlexColumn { vertical: Align::Start, horizontal: Align::Start });
    root.set_width(Val::PercentWidth(0.5));
    root.set_height(Val::PercentHeight(0.5));
    root.set_background(Background::Color(Vec4::new(0.05, 0.05, 0.05, 1.0)));
    root.set_border(Val::Px(1.0));
    root.set_border_radius(RectCorners::single(Val::Px(15.0)));

    let mut label = UINode::new("label".to_string());
    label.set_text(Some(Text {
        font: my_sdf_font.clone(),
        content: "Hello World!".into(),
        color: Vec4::ONE,
        font_size: 24.0,
        horizontal_align: Align::Start,
        vertical_align: Align::Start
    }));

    root.add(label);

    world.insert(
        EntityBuilder::default()
            .add(UINodeSDFRoot(root))
            .build()
    );
}
```

`UIPlugin` registers a render-startup system that allocates the SDF pipeline and GPU
buffers, and a render-update system that queries every `UINodeSDFRoot`, lays it out
against the current window size, uploads the flattened tree, and draws it. See
`examples/basic.rs` for a full example, including loading a font with `SDFFont::new`.

## Layout

- `Val`: `Auto`, `Px(f32)`, `PercentWidth(f32)`, `PercentHeight(f32)`.
- `Display`: `FlexColumn`, `FlexRow` (each with `Align` on both axes), or `Grid`
  (auto square-ish grid).
- `PositionType`: `Relative` (default, participates in flex flow) or
  `Absolute(Rect)` (positioned relative to the display root).
- `margin`, `padding`, `border`, `border_radius`, `border_color`, `background`,
  `aspect_ratio`, `min/max_width/height` are all set via `UINode`/`Style` setters.

Text is set with `Style::set_text(Some(Text { .. }))`; each character is rendered as
a vector-glyph `SDFElement` outlined from the loaded TTF via `SDFFont`.

## Widgets

`widgets` is an immediate mode widget library on top of the node tree. Add `WidgetPlugin`
alongside `UIPlugin`, then describe windows every frame from any render-update system through
the `Widget` resource. Widgets return whether they were clicked or changed:

```rust
App::new()
    .add_plugin(UIPlugin)
    .add_plugin(WidgetPlugin::new(Arc::new(SDFFont::new(font_bytes)?)))
    .on_render_update(settings_ui)
    .run()
```

```rust
#[system]
fn settings_ui(widget: ResMut<Widget>, settings: ResMut<Settings>) {
    widget.panel("Settings", Vec2::splat(10.0), 320.0, |ui| {
        ui.checkbox(&mut settings.enabled, "Enabled");
        ui.log_slider("rate", &mut settings.rate, 1..=1000, true, " per second");
        ui.row(|ui| {
            ui.text_field("name", &mut settings.name, "Name", 140.0);
            if ui.button("Save") { /* ... */ }
        });
    });
}
```

- Windows: `Widget::panel` (titled, dragged by its title, `panel_with_header` adds widgets right
  of the title), `Widget::popup` (untitled, drawn over panels), `Widget::hud`/`hud_row` (pinned to
  an anchor of the screen, drawn under panels, `hud_row` as wide as its widgets) and
  `Widget::dialog` (a popup at a screen anchor). All are kept on screen and sized to their content.
- Text on `Ui`: `label`, `colored_label`, `label_right`/`colored_label_right`, `legend` (dim, fixed
  width so a column of them lines up), `strong` and `heading` (bold font), `caption` (small).
- Controls on `Ui`: `button`, `button_tone`/`key_button` (`Tone::Primary` lit in the accent,
  `Tone::Danger` in the error color), `selectable`, `list_row` (selection marker, color swatch and
  trailing text), `checkbox`, `toggle` (sliding switch), `segmented` (one of several options),
  `log_slider` and `log_track` (just the groove and knob, `u32` or `f64`), `text_field` (edits as
  typed) and `number_field` (hands back the typed text on Enter or a click elsewhere).
- Readouts on `Ui`: `seven_segment` (a digit display drawn from rectangles, cheaper than glyphs),
  `led` (indicator light) and `keycap` (shows which input does what).
- Layout on `Ui`: `row`, `space`, `separator`, `header`/`header_count`, `scroll_rows` and
  `sections` (collapsing groups). `hint` shows a tooltip while the widget added last is hovered.
- Lists only lay out their visible rows. The SDF buffers hold a few thousand elements and each
  glyph is one, so keep long content in `scroll_rows`/`sections`.
- `Widget::over_ui`/`wants_pointer` tell app input whether the pointer belongs to the UI, and
  `Widget::clipboard_text` reads the OS clipboard.
- Sizes follow the window's scale factor. Colors come from `Theme` (`WidgetPlugin::with_theme` or
  `Widget::theme_mut`), in linear space for an sRGB surface, `srgb(0xRRGGBB)` converts hex colors.
  `WidgetPlugin::with_bold` sets the font for titles, headings and `strong` text.

Widgets are drawn from last frame's layout, so a click lands one frame after it happens. See
`examples/widgets.rs`.

## Architecture

- `nodes` — `UINode`/`Style` (the CPU-authored tree) and `render::layout_ui_nodes`,
  which turns a `UINode` tree into positioned `SDFElement`s.
- `data` — `SDFElement`/`SDFShape`/`SDFStyle`, the intermediate tree used to drive
  GPU upload.
- `fonts` — `SDFFont`, wrapping `ttf-parser` to outline glyphs into bezier-stroke
  `SDFShape`s.
- `buffers` — `TreeBuffer` (uploads the flattened element tree with sibling/child
  pointers) and `ChunkedBuffer` (a deduplicating arena for variable-length,
  shape-specific data such as rectangle radii, bezier curves, and glyph headers).
- `shader` — the `SDFRaw*` GPU-layout types and the WGSL SDF shaders in `shaders/`.
- `widgets` — `Widget`, `Ui` and `WidgetPlugin`, the immediate mode widgets above.

## Status

Experimental / in-development — expect breaking changes. `Background::Image` and
input event dispatch (`UINodePressedEvent` and friends) are scaffolded but not yet
wired up.
