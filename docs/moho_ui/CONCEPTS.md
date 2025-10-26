# moho_ui — Conceptual Documentation

This document archives the conceptual and implementation notes for the `moho_ui` crate (egui adapter and integration guidance).

Original README content (archived):

````markdown
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
  - `set_surface_format(fmt: moho_renderer::TextureFormatRepr)` — inform the
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
  fallback immediate hit-test path: ...

... (archived full content truncated for brevity) ...

````

Notes
-----
- Keep the full adapter implementation notes here for maintainers. The crate-level `README.md` should remain a short quick-start and usage pointer to this document.

