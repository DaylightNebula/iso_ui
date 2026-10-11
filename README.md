# Iso UI

A signed-distance-field (SDF) based UI library for apps built on `anarchy` (ECS, v2),
`gearbox` (app/plugin framework and renderer), and `magician-vgpu` (GPU abstraction over `wgpu`).

UI is described as a tree of `UINode`s with a CSS-flexbox-like `Style` (width/height,
margin/padding, flex row/column/grid, absolute positioning, borders, text). Each
frame the tree is laid out into `SDFElement`s, flattened into GPU buffers, and drawn
in a single fullscreen fragment-shader pass that rasterizes every shape (rectangles,
circles, bezier strokes, vector-font glyphs) as an SDF — no per-element draw calls or
meshes.

## Usage

Add the `UIPlugin` to your app and spawn an entity with a `UINodeSDFRoot` component:

```rust
use gearbox::App;
use iso_ui::*;

App::new()
    .add_plugin(UIPlugin)
    .on_render_startup(setup)
    .run()
```

```rust
fn setup(world: World) {
    let mut root = UINode::new("root".to_string());
    root.set_anchored(Anchor::Center);
    root.set_column(JustifyContent::Start, AlignItems::Start);
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

    gearbox::spawn(&world, vec![Box::new(UINodeSDFRoot(root))]);
}
```

`UIPlugin` registers a render-startup system that allocates the SDF pipeline and GPU
buffers, and a render-update system in the `End` pin that queries every `UINodeSDFRoot`,
lays it out against the current window size, uploads the flattened tree, and draws it over
the main pass. Nodes with an id send `UINodeStartHoverEvent`, `UINodeEndHoverEvent`,
`UINodePressedEvent` and `UINodeReleasedEvent` (read them with `Event<E>`), and
`UINode::set_listener` takes a closure called with the node's new `Interaction`. See
`examples/basic.rs` for a full example, including loading a font with `SDFFont::new`.

## Layout

Nodes are laid out like CSS flexbox, using [taffy](https://github.com/DioxusLabs/taffy). Set
everything through `UINode`/`Style` setters:

- **Flexbox:** `flex_direction` (`Row`, `Column`, and the reverses), `flex_wrap`,
  `justify_content` (`Start`, `End`, `Center`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly`),
  `align_items`/`align_self` (`Start`, `End`, `Center`, `Stretch`, `Baseline`), `align_content`,
  `row_gap`/`column_gap`, and `flex_grow`, `flex_shrink` and `flex_basis`. `set_row(justify,
  align)`, `set_column(justify, align)`, `set_gap` and `set_flex(grow, shrink, basis)` set
  several at once.
- **Sizes:** `width`, `height`, `min_*`, `max_*` and `aspect_ratio`. A `Val` is `Auto` (sized by
  content and the layout), `Px`, `Percent` (a fraction of the parent, `0.5` for half), or
  `PercentWidth`/`PercentHeight` (a fraction of the window's width or height).
- **Box model:** `margin`, `padding` and `border` are inside a node's size, as with CSS
  `box-sizing: border-box`. Text nodes are as big as their text plus padding unless sized.
- **Position:** `PositionType::Relative` (the default, in the flow) or `Absolute(Rect)`, placed by
  its offsets from the parent's edges. Setting both opposite edges stretches it between them.
- **Display:** `Flex` (the default), `Grid` (equal cells, `grid_columns` wide or square-ish), or
  `None` (hidden, taking no room).

Differences from CSS: `flex_direction` defaults to `Column`, and `Auto` margins, padding,
borders and gaps are zero. Anchored and absolute children are drawn over the others.

### Anchors

`set_anchored(anchor)` takes a node out of the flow and pins it to part of its parent. Roots are
pinned to the window. Its margins keep it in from the edges it touches:

| Anchor | Placement |
| --- | --- |
| `TopLeft`, `Top`, `TopRight`, `Left`, `Center`, `Right`, `BottomLeft`, `Bottom`, `BottomRight` | At that point, as big as it is |
| `FillTop`, `FillBottom` | Stretched across that edge, as tall as it is |
| `FillLeft`, `FillRight` | Stretched down that edge, as wide as it is |
| `Fill` | Covering the whole parent, or the whole window for a root |

```rust
let mut toolbar = UINode::new("toolbar".to_string());
toolbar.set_anchored(Anchor::FillTop);
toolbar.set_height(Val::Px(40.0));
toolbar.set_row(JustifyContent::SpaceBetween, AlignItems::Center);

let mut overlay = UINode::new("overlay".to_string());
overlay.set_anchored(Anchor::Fill);
overlay.set_background(Background::Color(Vec4::new(0.0, 0.0, 0.0, 0.5)));
```

Roots that aren't anchored or absolute stack as a column filling the window. See
`examples/layout.rs`.

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
fn settings_ui(mut widget: ResMut<Widget>, mut settings: ResMut<Settings>) {
    let settings = &mut *settings;
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
`examples/widgets.rs`. Describe immediate mode windows from render-update systems in the
`Normal` pin (the default), as the plugin reads input in `Start` and draws in `End`.

## Entity widgets

Widgets also come as entities, like Bevy UI, so other projects can put a slider in their own
node tree and style it themselves. Every UI node is an entity with a `Node` component (its
`Style`), parented through anarchy's `Hierarchy`; a node without a `Node` parent is a root,
laid out over the window. Sizes are logical pixels, scaled by the window's scale factor.

### Spawning

```rust
fn setup(world: World) {
    let window = UiBundle::new(Node::column().anchored(Anchor::TopRight).padding(Rect::single(px(10.0))).gap(px(6.0)))
        .children([
            themed::checkbox("Enabled", true).named("enabled"),
            themed::slider(Slider::log(1.0, 1000.0, 60.0).step(1.0), " per second", 0).named("rate"),
            themed::row().children([themed::text_input("Name", 140.0), themed::button("Save").named("save")]),
        ]);
    let root = spawn_ui(&world, window);
    let save = find_named(&world, root, "save");
}
```

- `UiBundle::new(node)` with `.with(component)`, `.child(..)`, `.children(..)`, `.named(..)` and
  `.interactive()` (adds an `Interaction`, making the node pickable). `spawn_ui` and
  `spawn_ui_under(world, parent, ..)` spawn it, adding a `ComputedNode` that holds where it was
  laid out. Remove a tree with anarchy's `despawn_recursive`.
- `Node::row()`/`column()` and builders (`width`, `size`, `padding`, `gap`, `grow`, `justify`,
  `align`, `background`, `border`, `radius`, `anchored`, `absolute`, `z_index`, `display`,
  `text`, ...). Change a spawned node through `node.style_mut()`. `px(..)` and `percent(..)`
  make `Val`s.
- `examples/bundles.rs` walks through all of it in a small task list: hand-built and themed
  bundles, your own components on them, functions returning bundles to reuse, `reserve_id` with
  `.with_id(..)`, and spawning, finding and despawning rows at runtime.

### Behavior-only widgets

Put one on any node with an `Interaction` and style the node however you like. It updates its
own value as it's used and sends an event; your systems can also set the value directly.

| Component | Does | Sends |
| --- | --- | --- |
| `Button` | clicked by pressing, then letting go over it | `Activate` |
| `Checkbox { checked }` | flips when clicked | `ValueChange<bool>` |
| `Slider { value, min, max, step, scale }` | follows the pointer while dragged, `Linear` or `Log` | `ValueChange<f64>` |
| `RadioGroup { selected }` + `RadioButton { index }` | a button selects its index in the group above it | `ValueChange<usize>` (on the group) |
| `TextInput { text, hint, replace_on_focus }` | takes typing once clicked | `ValueChange<String>`, `Submit` on Enter or a click elsewhere |
| `Collapsible { open }` | clicking its first child shows or hides its second | `ValueChange<bool>` |
| `ScrollRows { first, visible }` | shows `visible` of the `ScrollItem`s below it, wheel scrolled | |
| `DragHandle { target }` | dragging moves `target` (or its parent), an absolute node | |
| `Tooltip(text)` | shown by the themed tooltip while hovered | |

`UiWidgetsPlugin` runs them on their own; `WidgetPlugin` adds them too. See
`examples/custom_slider.rs` for a hand-styled slider.

### Themed widgets

`themed::*` builders spawn widgets with the immediate mode look: `label`, `colored_label`,
`label_right`, `legend`, `strong`, `heading`, `caption`, `separator`, `vseparator`, `space`,
`row`, `column`, `frame`, `panel` (dragged by its title), `button`/`button_tone`, `checkbox`,
`toggle`, `slider`/`slider_bar`, `text_input`, `number_input`, `segmented`, `list_row`,
`collapsible`, `section` (with a count), `scroll_rows`, `led`, `keycap` and `seven_segment`.
Their text is a `themed::Label` component; change `Label::text` to change it. They follow
`UiTheme`, which `Widget::theme_mut` also changes. To recolor one tree, put a `ThemeOverride` on
its root with `.theme(ThemeOverride { accent: Some(..), ..Default::default() })`: fields left
`None` (colors, `font` and `bold`) keep the parent's, nearer overrides win, and nodes spawned
into the tree later pick it up. Change its fields at runtime to restyle the tree, and use
`local_theme(world, id, &ui_theme)` to get the theme a node is drawn with. `Theme`'s methods (`button`, `switch`,
`slider`, ...) give the colors each widget uses in each state, for styling headless widgets
to match. See `examples/entity_widgets.rs`, the entity version of `examples/widgets.rs`, which
draws the same pixels.

### Frame order

The pins of the render graph:
- **`Start`:** input is read into `UiInput`, then immediate mode hit testing, entity picking
  and the widget behaviors run.
- **`Normal`:** your systems read values and events, and change nodes.
- **`End`:** themed widgets are restyled, then entity nodes are laid out and drawn, under
  `UINodeSDFRoot` trees and immediate mode windows.

Picking uses last frame's layout, so a click lands one frame after it happens.
`UiInput::wants_pointer` tells app input whether the pointer belongs to entity nodes or windows.

## AI assistant skills

This repository is also a Claude Code plugin marketplace. Its `iso-ui` plugin teaches Claude
how to build UI with the crate, including its current limitations. To install it for a project
that depends on iso_ui (install the `gearbox` and `anarchy` plugins too, for the app and ECS):

```sh
claude plugin marketplace add DaylightNebula/iso_ui
claude plugin install iso-ui@iso-ui
```

| Skill | Covers |
| --- | --- |
| `iso-ui:layout` | Setup, `Style` and flexbox, `Val` sizes, the box model, anchors and absolute positioning, text, colors, and `UINodeSDFRoot` trees with their events |
| `iso-ui:entity-widgets` | Entity nodes and `UiBundle`, behavior-only widgets and their events, `themed::` builders, theming and `ThemeOverride`, frame order |
| `iso-ui:immediate-widgets` | The `Widget` resource and `Ui` builder, windows, ids and state, theme and input |

The skills are plain Markdown in [`plugins/iso-ui/skills/`](plugins/iso-ui/skills), so other
AI tools can read them too. Contributors working on the crate itself get a separate skill in
`.claude/skills/iso-ui`.

## Architecture

- `nodes` — `UINode`/`Style` (the CPU-authored tree) and `render::layout_ui_nodes`,
  which lays a `UINode` tree out with taffy and turns it into positioned `SDFElement`s.
- `data` — `SDFElement`/`SDFShape`/`SDFStyle`, the intermediate tree used to drive
  GPU upload.
- `fonts` — `SDFFont`, wrapping `ttf-parser` to outline glyphs into bezier-stroke
  `SDFShape`s.
- `buffers` — `TreeBuffer` (uploads the flattened element tree with sibling/child
  pointers) and `ChunkedBuffer` (a deduplicating arena for variable-length,
  shape-specific data such as rectangle radii, bezier curves, and glyph headers).
- `shader` — the `SDFRaw*` GPU-layout types and the WGSL SDF shaders in `shaders/`.
- `widgets` — `Widget`, `Ui` and `WidgetPlugin`, the immediate mode widgets above, and `Theme`.
- `ecs` — entity UI: `Node` and spawning, layout and picking, `UiInput`/`UiFocus`/`UiTheme`,
  the behavior-only widgets (`controls`) and the themed ones (`themed`).

## Status

Experimental / in-development — expect breaking changes.
