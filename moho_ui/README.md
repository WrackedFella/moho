moho_ui — egui overlay adapter

Overview

`moho_ui` provides a small embedded egui adapter that implements the engine's
`FrameCallback` and exposes a channel of `UiEvent` messages the application
(main) can poll. It is designed as a minimal integration layer so the rest of
the engine can be mostly UI-backend-agnostic.

This README documents how to use the adapter, the important implementation
notes and quirks we discovered while integrating egui v0.33, and tips to avoid
subtle coordinate / timing problems.

Quick start

- Build and run the app with egui and debug helpers:

```powershell
cargo run -p moho --features "backend-wgpu ui-egui ui-egui-debug"
```

- The adapter constructs an egui `Context` and (when a `winit::Window` is
available) an `egui_winit::State`. It exposes a channel receiver you can poll
for `UiEvent` messages.

Key API (adapter surface)

- Type: `moho_ui::egui_adapter::EguiUi`
- Construction: `EguiUi::new(window: Option<Arc<winit::window::Window>>) -> (EguiUi, UiReceiver)`
  - Returns the adapter and a `crossbeam_channel::Receiver<UiEvent>`.
- Methods on `EguiUi`:
  - `set_surface_format(fmt: engine_renderer::TextureFormatRepr)` — inform the
    adapter about the swapchain format so the egui GPU renderer can be
    initialized.
  - `recall_staging_belt(&mut self)` — IMPORTANT: call this on the adapter
    immediately after the engine calls `queue.submit(...)`. This lets the
    internal `wgpu::util::StagingBelt` reclaim its memory safely.
  - `send_load_scene(path)` / `send_exit()` — convenience helpers that send
    `UiEvent::LoadScene` / `UiEvent::Exit` on the internal channel.
  - `is_visible()` — whether the overlay is currently visible.

Events sent by the UI

`UiEvent` enum (sent from adapter -> main):
- `LoadScene(PathBuf)` — Request to load a scene. Adapter uses a default path
  in the small demo UI; real integrations should wire this to a file chooser
  or similar.
- `Exit` — Request to exit the application.
- `OverlayToggled(bool)` — Overlay shown/hidden. The adapter sends this so the
  application (main) can perform OS-level cursor visibility/grab changes.

Important integration notes and quirks

1) Centralize OS cursor/grab state in `main`
- The adapter intentionally does not directly change OS cursor visibility or
  grab in most places. Instead it sends `UiEvent::OverlayToggled(bool)` and
  expects `main` to apply cursor visibility/grab changes. This avoids races
  and duplicate side-effects when multiple systems attempt to change the
  cursor state.

2) Staging belt lifecycle
- `EguiUi` uses a `wgpu::util::StagingBelt` to upload egui textures/buffers.
  The adapter calls `staging_belt.finish()` before returning from its
  `FrameCallback`. The engine MUST call `EguiUi::recall_staging_belt()` right
  after `queue.submit(...)` to let the belt reclaim memory. Failure to do so
  will cause memory to accumulate.

3) Input translation and synthetic pointer
- When a `Window` is available, the adapter prefers `egui_winit::State` to
  translate winit events into egui `RawInput`. If egui_winit isn't used (or
  when we want to ensure manually-injected events are processed immediately),
  the adapter falls back to `egui::RawInput` and will set
  `force_raw_input_next_frame` so synthetic events are consumed on the next
  frame.
- When showing the overlay (for example the start menu) the adapter seeds a
  synthetic `PointerMoved` at a sensible top-left position (16,16) so the UI
  is interactive immediately — this avoids UI requiring one extra real mouse
  move to become responsive.

4) Warmup pass to avoid "zero shapes on first frame"
- To avoid an egui regression where anchored Areas can produce no shapes on
  the first frame, the adapter performs a short warmup frame (a color clear
  and a minimal egui frame) the first time the overlay is shown. This is
  feature-guarded and can be observed with `ui-egui-debug`.

5) Clicks that occur entirely between frames (press+release) — fallback
   hit-testing
- To make the simple top-left menu reliably clickable even when press and
  release happen between frames (which can occur when the app runs at a
  lower frame-rate or the user clicks fast), the adapter implements a
  fallback immediate hit-test path:
  - When events are forwarded to `egui_winit`, it prefers to let egui
    consume events. However `egui_winit` can return "not consumed" for
    some events. In that case the adapter records press locations and, on
    release, performs an immediate hit-test and may send UI events.
  - The fallback uses the exact `egui::Rect` recorded from the last egui
    frame (stored as `load_button_rect` / `exit_button_rect`). This keeps the
    fallback geometry identical to the painted debug rectangles and avoids
    hard-coded geometry mismatches.

6) Coordinate space (logical vs device pixels)
- `egui::Response.rect` and egui logical coordinates are in "points" (logical
  pixels). winit reports cursor positions in physical/device pixels. The
  adapter converts winit positions to logical points by dividing by
  `window.scale_factor()` when capturing cursor positions so hit-tests use the
  same coordinate space. If you change how/where cursor positions are
  recorded, ensure you preserve this conversion.

7) Debugging helpers (feature `ui-egui-debug`)
- Compile/run with `ui-egui-debug` to enable:
  - Extra INFO logs from the adapter.
  - A magenta warmup clear (visual diagnostic) when the overlay first shows.
  - Painted translucent red/green rectangles showing the exact egui
    `Response.rect` for the Load/Exit buttons.
  - Extra diagnostic logs showing `load_rect`, `exit_rect` and the recorded
    cursor positions used by the fallback hit-test.

Root cause we found and fix summary

- Symptom: Clicking the Exit area sometimes triggered Load Scene even though
  the debug rectangles matched the visual UI.

- Short diagnosis: The non-egui fallback used a different geometry/coordinate
  space than the painted egui widgets — at one point the fallback used hard
  coded rectangles while the painted UI moved/laid out buttons based on
  egui's layout (and egui rectangles were in logical points). This mismatch
  caused the fallback to classify a click using a different box than the one
  the user saw.

- Fixes applied:
  1. Always record the egui `Response.rect` for buttons each frame and use
     those stored rects in the fallback hit-tests. This removes duplicated
     geometry definitions and makes the fallback match exactly what egui
     painted.
  2. Ensure cursor positions are converted to logical points with
     `window.scale_factor()` where we capture them. This keeps both sides of
     the containment test in the same coordinate space.
  3. Centralize OS cursor/grab side-effects in `main` by having the adapter
     send `UiEvent::OverlayToggled(bool)` instead of directly setting
     cursor/grab in multiple places. This prevents races and inconsistent
     cursor state.
  4. Add diagnostic logging (and debug rectangles) to help spot remaining
     mismatches quickly.

Known remaining quirks and recommendations

- The fallback path exists to be robust while egui is being integrated. If
  you fully trust egui_winit in your platform/versions you can remove the
  fallback code — but keep in mind press+release between frames can still
  happen and may be surprising without the fallback.

- Keep `recall_staging_belt()` usage in your submit path. It's easy to
  forget and will silently leak staging memory.

- If you port or update egui/egui-winit/egui-wgpu versions, re-check the
  `egui_winit::State` constructor and any `egui_wgpu::Renderer` API — egui
  and friends have historically changed their initialization signatures
  between minor versions.

- When adding new top-left widgets, remember the fallback relies on the
  `Response.rect`s being stored before any immediate hit-tests happen. Don't
  move rect storage to a conditional branch that runs only when the widget
  is clicked.

8) egui Slider width control quirk
- **Problem**: `egui::Slider` does not respect `ui.add_sized(...)` for its width
  the way `TextEdit` and other widgets do. Using `add_sized(egui::vec2(width, height), Slider::new(...))`
  will NOT make the slider's draggable bar expand to fill the specified width.
  
- **Root cause**: egui's `Slider` widget has internal sizing logic that uses
  `ui.spacing().slider_width` to determine the width of the draggable bar portion.
  The slider ignores the outer `add_sized` constraint and instead consults this
  spacing property.

- **Solution**: To make a slider wider than the default, you must set
  `ui.spacing_mut().slider_width` to the desired width (minus space for the
  value display and padding, typically ~60px) before adding the slider:
  
  ```rust
  ui.horizontal(|ui| {
      ui.allocate_ui_with_layout(egui::vec2(label_width, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
          ui.label("World Size");
      });
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
          let slider_width = field_width * 1.5; // e.g., 540px if field_width is 360px
          ui.spacing_mut().slider_width = slider_width - 60.0; // Account for value display box
          ui.add(egui::Slider::new(&mut self.size_xz, 64..=256).min_decimals(0));
      });
  });
  ```

- **Example**: In `NewWorldMenu`, we wanted the World Size slider to be 50% wider
  than the text fields (360px → 540px). Using `add_sized` alone did not work.
  Setting `ui.spacing_mut().slider_width = 480.0` (540px - 60px) before adding
  the slider successfully made the draggable bar expand to the desired width.

- **Tip**: The subtracted padding (60px in our case) accounts for egui's internal
  spacing, the value display box on the right, and margins. You may need to adjust
  this value if your slider uses custom formatters or different styling.

How to run with debug output (PowerShell)

```powershell
# run with egui + debug helper logs
cargo run -p moho --features "backend-wgpu ui-egui ui-egui-debug"
```

Development notes / future improvements

- Replace the manual fallback hit-test with a dedicated input-glue layer that
  consistently converts winit events to egui `RawInput` and guarantees
  event ordering. This will reduce the need for ad-hoc press-tracking.
- Add a small integration test harness (headless or offscreen) that simulates
  press+release sequences to validate fallback behavior.
- Consider exposing a small utility that returns the last-stored button
  rects for unit tests (currently they are stored on the adapter struct).

If you want, I can:
- Add a short unit/integration test that exercises the press+release fallback
  logic.
- Add the README content to the top-level docs or `docs/` folder instead.

Recent input & UI notes

- InputDispatcher: the app now uses a small prioritized dispatcher to route `WindowEvent`s. The settings keybind capture registers at a higher priority so it can intercept events while the settings menu is listening.
- Shared mapping: the physical key → binding mapping lives in the workspace crate `moho_input` to avoid duplication between the binary and `moho_ui`.
- Wheel policy: mouse-wheel events are forwarded to the game only when the UI overlay is hidden. The forwarder uses a conservative `try_lock()` behavior and will *not* forward if the UI lock cannot be obtained.

These items are small, self-contained, and were added to make keybind capture and UI/game input composition predictable.

---
Small note: this adapter intentionally contains a few pragmatic
engineering choices (warmup pass, synthetic pointer injection, fallback
hit-tests) to make the UI feel responsive in a minimal embedding. When you
fully wire a richer UI and input system, a simpler integration (pure
`egui_winit` + no fallback) may be preferable.
