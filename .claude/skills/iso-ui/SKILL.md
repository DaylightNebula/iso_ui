---
name: iso-ui
description: How to work on the iso_ui crate itself (Rust SDF UI library on gearbox, anarchy and taffy) - where things live, the sibling repos, the frame order of its systems, how to build, test and visually verify changes with headless screenshots, pixel diffs and injected input, keeping immediate mode and themed entity widgets pixel-identical, internal gotchas, conventions, and keeping the README and user skills in sync. Use when changing, testing or documenting code in this repository.
---

# Working on iso_ui

iso_ui draws UI as signed distance fields in one fullscreen fragment pass. Layout is CSS flexbox
through taffy. There are three ways to build UI:
- `UINodeSDFRoot` node trees;
- entity nodes, with behavior-only and themed widgets;
- immediate mode (`Widget`/`Ui`).

The API is early and changes freely. When this skill disagrees with the code, trust the code.

## Sibling repos

`Cargo.toml` uses path dependencies, so these sit next to this repo:

| Path | What it is |
| --- | --- |
| `../gearbox` | App, plugins, render graph pins, `Graphics`/`Frame`, texture vaults, `spawn`. Has its own repo skill and user skills |
| `../anarchy` | The ECS (v2): components, resources, `Query`, `Event<E>`, `Hierarchy`. Has its own repo skill and user skills; read them for ECS behavior |
| `../shader_magician` | `magician-vgpu`: `TreeBuffer`/`ChunkedBuffer` uploads, `Pipeline`, `glam` |

A bug can live in a sibling. Fix it there and build iso_ui against it.

## Map of the crate

| Path | What lives there |
| --- | --- |
| `src/lib.rs` | `UIPlugin` (resources and system ordering), GPU setup (`init_resources`), `ui_render_pass` (merges entity elements, `UINodeSDFRoot` trees and `UIRawElements`, then uploads and draws) |
| `src/nodes/data.rs` | `UINode`, `Style` (getset accessors), `Val`, `Rect`, `Anchor`, `Display`, flex enums, `Interaction`, `Text`, `Style::scaled` |
| `src/nodes/render.rs` | Layout: the `LayoutTree` trait, `layout_tree` (builds a taffy tree, wraps anchored nodes in frames, measures text, emits `SDFElement`s in paint order), `layout_ui_nodes`. Layout tests |
| `src/nodes/events.rs` | `UINode*Event`s sent as `UINode` interactions change |
| `src/data.rs`, `src/shader.rs`, `shaders/` | `SDFElement`/`SDFShape`/`SDFStyle`, their GPU layouts, and the WGSL |
| `src/fonts/` | `SDFFont`: `measure_text`, and `render_glyph_line` (glyphs as bezier SDFs) |
| `src/widgets.rs` | Immediate mode (`WidgetPlugin`, `Widget`, `Ui`), `Theme` and its per-state color methods, `srgb`, `Tone`, and the size constants themed widgets share |
| `src/ecs/input.rs` | `UiInput` and `read_ui_input`, `apply_key`, `UiFocus`, `UiTheme`, `ThemeOverride`/`local_theme`, `UiClipboard` |
| `src/ecs/node.rs` | `Node`, `ComputedNode`, `Name`, `UiBundle`, `spawn_ui`/`spawn_ui_under`, `reserve_id`, `find_named`, `px`/`percent` |
| `src/ecs/layout.rs` | `layout_entity_ui`: snapshots the entity tree (scaled to window pixels), lays it out, writes `ComputedNode`s |
| `src/ecs/picking.rs` | `pick_ui`: `UiInput::hovered`/`captured` and each `Interaction` |
| `src/ecs/controls.rs` | Behavior-only widgets, their events and systems, `UiWidgetsPlugin`. System tests |
| `src/ecs/themed.rs` | `themed::` builders, `Label`, `Themed`, and `restyle_themed`, which restyles parts found by child position |
| `examples/` | `basic`, `layout`, `widgets` (immediate), `entity_widgets` (the same UI from themed entities), `custom_slider` (headless), `bundles` (UiBundle walkthrough) |
| `plugins/iso-ui/skills/` | User skills: `layout`, `entity-widgets` and `immediate-widgets`. Keep them in sync |
| `scripts/` | `pngdiff.py` (screenshot diffs) and `check_skills.py` (compiles the user skills' snippets) |

## How a frame runs (render graph pins)

1. **`Start`:**
   1. `read_ui_input` fills `UiInput`. A capture is cleared the frame after its button is
      let go.
   2. `widgets_begin` copies input and theme into `Widget`, hit tests last frame's windows
      and sets `UiInput::over_windows`.
   3. `pick_ui` uses last frame's `ComputedNode`s. Only nodes laid out in the latest pass
      (`ComputedNode::pass`) count.
   4. The `UiWidgetsPlugin` systems run. `unhide_scrolled` → `toggle_collapsibles` →
      `scroll_rows` is ordered.
2. **`Normal`:** user systems and immediate mode descriptions.
3. **`End`:**
   1. `widgets_end` hands the windows to the widget root and writes theme changes back to
      `UiTheme`.
   2. `restyle_themed` styles themed parts, then sets every `Label`'s text.
   3. `layout_entity_ui`.
   4. `ui_render_pass`: entity elements first, then node trees, so immediate windows draw on
      top.

Ordering against systems from other plugins works with `before`/`after` even if they're added
later, as anarchy resolves the references then.

## Build, test, verify

```sh
~/.cargo/bin/cargo build --all-targets   # no warnings expected; cargo may not be on PATH
~/.cargo/bin/cargo test                  # layout, picking, input, widget-system and theme tests
```

Visual changes need headless screenshots. View them with the `Read` tool:

```sh
GEARBOX_HEADLESS=700x560 GEARBOX_SCREENSHOT=/tmp/out.png ~/.cargo/bin/cargo run --example widgets
python3 -I scripts/pngdiff.py /tmp/before.png /tmp/out.png
```

- **Baselines first.** Take a screenshot before changing anything; it's the baseline to diff
  against. Don't `git stash` to get a "before" build: the working tree may hold uncommitted
  work that the stash takes with it.
- **Parity.** `entity_widgets` and `widgets` must render identical pixels. Diff them after any
  change to a widget's look or size.
- **Input.** Headless runs get no input. To test interaction:
  1. copy an example to `examples/zz_check.rs`;
  2. add a render system that sends `gearbox::WindowEvent(winit::event::WindowEvent::CursorMoved
     { device_id: DeviceId::dummy(), .. })` and `MouseInput` on chosen frames (read
     `Res<gearbox::FrameCount>`);
  3. run it with `GEARBOX_FRAMES=30`, then delete the file.

  Key events can't be built, so test typing through `UiInput.keys` in unit tests, or by
  writing `TextInput::text` directly.
- **Widget systems** can be tested without a GPU: build a `World`, insert a `UiInput`, and run
  the system through `SingleThreadedExecutor` (see the tests in `src/ecs/controls.rs`). Order
  collector systems with `after(system)`.
- **Startup warning.** "Missing bind group for bindless textures" can be logged on the first
  frame. It's a gearbox startup race, not your change.
- **Wasm** can't be checked without the wasm target; say so when you couldn't.

## Gotchas

- **anarchy v2:**
  - Systems return `()`; log errors with `log::error!`.
  - Components and resources need `Debug`; hand-write it with `finish_non_exhaustive()`.
  - Locks aren't reentrant: drop query guards before spawning, despawning or locking the same
    component again.
  - An entity's components are fixed at spawn.
  - `Option<&T>` query terms don't work; use `world.handle(id).get::<&T>()` and treat
    errors as absent.
- **Style accessors:** `getset` setters return `&mut Style`, so closures calling them need
  braces to return `()`. `Node` deliberately doesn't deref to `Style`, because its builders
  would shadow the getters.
- **Layout:**
  - Defaults differ from CSS: `flex_direction` is `Column`, and `Auto` margins, padding,
    borders and gaps are 0.
  - Borders take room inside the size.
  - Rounding is off, for fractional positions.
  - `Percent` is parent-relative; `PercentWidth`/`PercentHeight` are window-relative.
- **Scaling:** entity styles are scaled by the scale factor in `layout_entity_ui`
  (`Style::scaled`). `UINodeSDFRoot` trees aren't scaled, and immediate mode scales itself
  through `px()`.
- **Immediate/themed parity:**
  - Colors per state come from `Theme`'s methods, and sizes from the `pub(crate)` constants
    in `widgets.rs`.
  - When a widget changes, change both `Ui` and its `themed::` builder or restyle arm, then
    diff the two examples.
  - Themed parts are found by child index: changing a builder's children means updating its
    restyle arm too.
- **Restyle ownership:** `restyle_themed` overwrites the colors of the parts it controls every
  frame. Only `Label`s it doesn't touch keep user colors.
- **No clipping:** `Overflow::Clip` isn't implemented in the shader. `ScrollRows` and
  immediate lists hide rows instead.
- **Buffer limits:** the SDF buffers are capped (`MAX_BUFFER_ELEMENTS`, and less under
  WebGL). Every glyph is one element.

## Conventions

- **Match the surrounding code:** terse one-line bodies where they fit, `impl <T>` with a
  space, and comments only for non-obvious reasons.
- **Docs:** every public item gets a short `///` saying what it does.
- **Errors:** `anyhow::Result` in fallible helpers; no `unwrap` in per-frame library code.
- **Tests** go in `#[cfg(test)] mod tests` at the bottom of each file.

## Keeping docs and skills in sync

When public API or behavior changes:
- Update `README.md`.
- Update the matching skill in `plugins/iso-ui/skills/` (`layout`, `entity-widgets`,
  `immediate-widgets`), and this skill if internals moved.
- Check snippets compile:

  ```sh
  python3 -I scripts/check_skills.py && ~/.cargo/bin/cargo build --example zz_check; rm examples/zz_check.rs
  ```

- Check README snippets the same way, by pasting them into `examples/zz_check.rs`.
- Run `claude plugin validate .` after editing plugin files.
