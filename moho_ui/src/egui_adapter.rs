//! Minimal egui UI adapter for moho.
//!
//! This module is feature-gated behind `ui-egui`. It provides a very small
//! adapter that implements `engine_renderer::FrameCallback` and exposes
//! a channel of `UiEvent` messages the application can poll. This mirrors
//! the old iced-based API so the rest of the engine does not need to
//! change while the UI backend is swapped.

// Priority: Medium
//     TODO: Reduce/Remove these module-level clippy allows by refactoring
//           nested conditionals and manual range checks into small helpers.
//           This will improve readability and let clippy enforce stricter
//           style rules. See clippy lints: collapsible_if, manual_range_contains.
#![allow(
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::manual_range_contains,
    clippy::let_unit_value,
    clippy::unnecessary_cast,
    clippy::unnecessary_map_or
)]

#[cfg(feature = "ui-egui")]
// module contents compiled only when `ui-egui` feature is enabled
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use engine_renderer::FrameCallback;
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::{CursorGrabMode, Window};

/// Events sent from the UI to the application.
#[derive(Debug, Clone)]
pub enum UiEvent {
    LoadScene(PathBuf),
    Exit,
    /// Overlay visibility changed: true = shown, false = hidden
    OverlayToggled(bool),
}

// Test helpers for external tests. Placed at module scope so they are
// available to the `moho_ui` integration tests.
impl EguiUi {
    /// Set stored response rects (as if painted by egui).
    pub fn test_set_button_rects(&mut self, load: egui::Rect, exit: egui::Rect) {
        self.load_button_rect = Some(load);
        self.exit_button_rect = Some(exit);
    }

    /// Simulate a press and release that both occurred between frames.
    /// `press` is logical pixel position of press, `release` is release.
    pub fn test_simulate_press_release(&mut self, press: (f32, f32), release: (f32, f32)) {
        self.mouse_press_pos = Some(press);
        self.last_cursor = Some(release);
        self.mouse_was_pressed = true;
        self.mouse_pressed = false;
    }

    /// Run the adapter's non-egui fallback click detection logic and
    /// return the UiEvents that would have been emitted.
    pub fn test_collect_fallback_events(&mut self) -> Vec<UiEvent> {
        let mut out: Vec<UiEvent> = Vec::new();
        if self.mouse_was_pressed && !self.mouse_pressed {
            if let Some((px, py)) = self.mouse_press_pos.take() {
                if let Some((rx, ry)) = self.last_cursor {
                    let in_load_press = self
                        .load_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(px, py)));
                    let in_exit_press = self
                        .exit_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(px, py)));
                    let in_load_release = self
                        .load_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(rx, ry)));
                    let in_exit_release = self
                        .exit_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(rx, ry)));

                    if in_load_press && in_load_release {
                        out.push(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                        self.ui_visible = false;
                        self.cursor_grabbed = Some(true);
                        out.push(UiEvent::OverlayToggled(false));
                    } else if in_exit_press && in_exit_release {
                        out.push(UiEvent::Exit);
                    }
                }
            } else if let Some((x, y)) = self.last_cursor {
                let in_load = self
                    .load_button_rect
                    .is_some_and(|r| r.contains(egui::pos2(x, y)));
                let in_exit = self
                    .exit_button_rect
                    .is_some_and(|r| r.contains(egui::pos2(x, y)));
                if in_load {
                    out.push(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                    self.ui_visible = false;
                    self.cursor_grabbed = Some(true);
                    out.push(UiEvent::OverlayToggled(false));
                } else if in_exit {
                    out.push(UiEvent::Exit);
                }
            }
        }
        // Track previous press state as call() would do
        self.mouse_was_pressed = self.mouse_pressed;
        out
    }

    /// Set a staging belt for smoke testing recall behavior.
    pub fn test_set_staging_belt(&mut self) {
        self.staging_belt = Some(wgpu::util::StagingBelt::new(1024));
    }
}

static UI_SENDER: OnceCell<crossbeam_channel::Sender<UiEvent>> = OnceCell::new();

/// Global flag that indicates whether the UI overlay is visible.
/// Other parts of the program (for example the input/controller code)
/// can read this cheaply without locking the adapter.
pub static UI_OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Receiver type for UI events.
pub type UiReceiver = crossbeam_channel::Receiver<UiEvent>;

/// A minimal egui-backed UI adapter. For now it does not render
/// any egui content but it provides a channel the app can poll to
/// receive UI intent.
pub struct EguiUi {
    sender: crossbeam_channel::Sender<UiEvent>,
    renderer_initialized: bool,
    window: Option<Arc<Window>>,
    // Simple input state for embedded UI (used until egui_winit is wired)
    last_cursor: Option<(f32, f32)>,
    mouse_pressed: bool,
    mouse_was_pressed: bool,
    // Position where the last left-button press began (logical pixels).
    mouse_press_pos: Option<(f32, f32)>,
    // Last-known egui response rects for the fallback hit-test
    load_button_rect: Option<egui::Rect>,
    exit_button_rect: Option<egui::Rect>,
    // Whether the UI overlay is currently visible. Hidden by default.
    ui_visible: bool,
    // egui runtime fields (lazy initialized)
    egui_ctx: Option<egui::Context>,
    egui_winit: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
    staging_belt: Option<wgpu::util::StagingBelt>,
    surface_format: Option<engine_renderer::TextureFormatRepr>,
    // If true, prefer consuming raw_input on the next FrameCallback even
    // if egui_winit is available. This is set when the overlay is shown
    // so synthetic PointerMoved events are honoured immediately.
    force_raw_input_next_frame: bool,
    // RawInput accumulator for egui (used until egui_winit is wired)
    raw_input: egui::RawInput,
    // reduce logging spam for cursor move forwarding
    #[allow(dead_code)]
    // Priority: Low
    //     TODO: Consider turning `last_cursor_log` into an optional
    //           throttled debug logger or remove it if unused.
    last_cursor_log: Option<Instant>,
    // If true the adapter will perform a small warmup pass the first
    // time the UI is shown. This primes egui's layout code and avoids
    // the "zero shapes on first frame" regression for anchored Areas.
    warmup_pending: bool,
    // When true the UI was shown as the initial start menu. The
    // start menu behavior is slightly different (flat background,
    // centered column layout) and will be used on launch. Escape
    // will switch to the normal overlay mode.
    start_mode: bool,
    // Tracks whether we believe the cursor is currently grabbed for
    // camera control (true) or released for UI interaction (false).
    // This lets `call` enforce the desired state when other systems
    // may also attempt to change cursor grab.
    cursor_grabbed: Option<bool>,
    // Current active menu (boxed trait object)
    current_menu: Option<Box<dyn crate::menus::menu::Menu>>,
}

// Toggle these constants to run quick A/B experiments.
// - MERGE_PREPEND: when true, prepend synthetic events to egui_winit input;
//   when false, append them (original behavior).
// - USE_AREA: when true, use an anchored egui::Area; when false, use TopBottomPanel.
const MERGE_PREPEND: bool = false;
// Priority: Low
//     TODO: Revisit `USE_AREA` toggle and convert to a runtime option
//           or remove when area/top-panel experiments are finalized.
#[allow(dead_code)]
const USE_AREA: bool = false;

impl EguiUi {
    /// Return whether the overlay/UI is currently visible.
    ///
    /// This is a small accessor used by the application (main.rs)
    /// to decide whether input systems should re-grab the cursor.
    pub fn is_visible(&self) -> bool {
        self.ui_visible
    }

    /// Robustly detect an Escape key event across winit versions/events.
    // Priority: Low
    //     TODO: Consolidate Escape detection (winit variations) into a
    //           small utility and write unit tests for the different
    //           winit keyboard event shapes.
    #[allow(dead_code)]
    fn event_is_escape(ev: &winit::event::WindowEvent) -> bool {
        use winit::event::WindowEvent;
        match ev {
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::event::ElementState;
                // Only consider key presses (not releases)
                if event.state != ElementState::Pressed {
                    return false;
                }
                // Some winit versions expose `text` and `logical_key` on KeyEvent.
                // Prefer checking the received text for the escape character, then
                // fall back to the logical_key debug name containing "Escape".
                if event
                    .text
                    .as_deref()
                    .map(|s| s == "\u{1b}")
                    .unwrap_or(false)
                {
                    return true;
                }
                if format!("{:?}", event.logical_key).contains("Escape") {
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Provide the surface format used by the swapchain so we can
    /// initialize the egui GPU renderer with the correct format.
    pub fn set_surface_format(&mut self, fmt: engine_renderer::TextureFormatRepr) {
        self.surface_format = Some(fmt);
    }
    pub fn new(window: Option<Arc<Window>>) -> (Self, UiReceiver) {
        let (s, r) = crossbeam_channel::unbounded();
        let _ = UI_SENDER.set(s.clone());
        // initialization
        // Eagerly create an egui Context so the UI can be shown
        // and receive input immediately after construction.
        let egui_ctx = Some(egui::Context::default());

        // If a window is available, also create egui_winit::State
        // now so input translation is ready on first use.
        let mut egui_winit_state = None;
        if let Some(win) = window.as_ref() {
            let display_target: &dyn HasDisplayHandle = &**win;
            let state = egui_winit::State::new(
                egui_ctx.clone().unwrap(),
                egui::ViewportId::ROOT,
                display_target,
                Some(win.scale_factor() as f32),
                None,
                None,
            );
            egui_winit_state = Some(state);
        }

        let mut ui = EguiUi {
            sender: s,
            renderer_initialized: false,
            window,
            // input state defaults
            last_cursor: None,
            mouse_pressed: false,
            mouse_was_pressed: false,
            mouse_press_pos: None,
            load_button_rect: None,
            exit_button_rect: None,
            // Start the application with the start menu visible so the
            // user sees a deterministic menu on launch.
            ui_visible: true,
            egui_ctx,
            egui_winit: egui_winit_state,
            egui_renderer: None,
            staging_belt: None,
            surface_format: None,
            raw_input: egui::RawInput::default(),
            last_cursor_log: None,
            force_raw_input_next_frame: false,
            warmup_pending: true,
            start_mode: true,
            // cursor_grabbed already set above
            cursor_grabbed: None,
            // initialize menu system with the start menu
            current_menu: Some(Box::new(crate::menus::StartMenu::new())),
        };

        // If we have a window and the UI starts visible (start menu),
        // make sure the cursor is visible and not grabbed so the user
        // can interact with the menu immediately.
        if let (Some(win), true) = (ui.window.as_ref(), ui.ui_visible) {
            let _ = win.set_cursor_visible(true);
            let _ = win.set_cursor_grab(CursorGrabMode::None);
        }

        // If the UI starts visible, ensure the rest of the engine
        // knows about it so input systems and cursor state can be
        // configured consistently. Also seed a synthetic pointer for
        // immediate interactivity (see below).
        if ui.ui_visible {
            // Set the global flag so other systems can read cheaply
            // without polling the adapter.
            UI_OVERLAY_VISIBLE.store(true, Ordering::SeqCst);
            // Inform the application via the UiEvent channel so main
            // performs OS-level cursor/grab changes.
            let _ = ui.sender.send(UiEvent::OverlayToggled(true));
        }

        // If the UI starts visible (start menu), seed a synthetic
        // pointer move event so egui has an immediate pointer position
        // and clicks are recognized without requiring an extra mouse
        // movement or Escape press.
        if ui.ui_visible {
            // Provide a sensible top-left synthetic pointer so anchored
            // Areas are likely to be visible and interactive.
            let tx = 16.0_f32;
            let ty = 16.0_f32;
            ui.raw_input = egui::RawInput::default();
            ui.last_cursor = Some((tx, ty));
            ui.raw_input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(tx, ty)));
            ui.force_raw_input_next_frame = true;
            if let Some(ctx) = &ui.egui_ctx {
                ctx.request_repaint();
            }
        }

        (ui, r)
    }

    pub fn send_load_scene<P: Into<PathBuf>>(&self, p: P) {
        let _ = self.sender.send(UiEvent::LoadScene(p.into()));
    }

    pub fn send_exit(&self) {
        let _ = self.sender.send(UiEvent::Exit);
    }

    /// Ask the staging belt to recall freed memory. Call this after the
    /// application submits the recorded command buffer(s) to the GPU queue.
    ///
    /// Important: The engine must call this immediately after calling
    /// `queue.submit(...)` so the staging belt can reclaim its memory safely.
    pub fn recall_staging_belt(&mut self) {
        if let Some(belt) = &mut self.staging_belt {
            // `recall` is safe to call multiple times; if the belt is empty
            // this is a no-op. We don't have access to the queue here; the
            // staging belt implementation used accepts no args for recall.
            belt.recall();
        }
    }

    pub fn handle_winit_event(&mut self, _event: &winit::event::WindowEvent) {
        use winit::event::{ElementState, MouseButton, WindowEvent};

        // No pre-processing here; events will be forwarded to egui_winit
        // when available. Escape detection and overlay toggling are
        // handled after forwarding so egui_winit can see the same event.

        // Lazy-init egui_winit::State when we have both egui context and a window.
        if self.egui_winit.is_none() {
            if let (Some(ctx), Some(win)) = (self.egui_ctx.clone(), self.window.as_ref()) {
                // build a display target from the winit Window
                let display_target: &dyn HasDisplayHandle = &**win;
                let state = egui_winit::State::new(
                    ctx,
                    egui::ViewportId::ROOT,
                    display_target,
                    Some(win.scale_factor() as f32),
                    None,
                    None,
                );
                self.egui_winit = Some(state);
            }
        }

        // If egui_winit is available, forward the event to it. It will
        // translate and consume events appropriately. After forwarding we
        // inspect keyboard events for Escape so we can toggle overlay in a
        // consistent way. Otherwise fall back to our simple pointer
        // handling (only while UI visible).
        if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
            // Capture cursor position even when forwarding to egui_winit so we
            // can bootstrap raw_input when showing the overlay.
            if let winit::event::WindowEvent::CursorMoved { position, .. } = _event {
                let px = position.x as f32;
                let py = position.y as f32;
                // Convert to logical points using the window's scale factor
                if let Some(win) = &self.window {
                    let ppp = win.scale_factor() as f32;
                    self.last_cursor = Some((px / ppp, py / ppp));
                } else {
                    self.last_cursor = Some((px, py));
                }
                // recorded cursor while forwarding to egui_winit
            }
            // When forwarding to egui_winit we avoid doing our own
            // immediate MouseInput hit-tests to prevent duplicating or
            // conflicting UI events. egui_winit/egui will handle clicks
            // and produce UiEvents via the UI closure above. We still
            // capture CursorMoved earlier to bootstrap raw_input when
            // the overlay is shown.
            // Forward general events to egui_winit. CursorMoved can be
            // very noisy (lots of events per second) so keep the
            // forwarding log at debug level to avoid flooding INFO logs.
            log::debug!("EguiUi: forwarding event to egui_winit: {:?}", _event);
            // `on_window_event` returns true when the event was consumed
            // by egui_winit (i.e. it updated egui's input state). If it
            // returns false, fall back to our manual handling so the
            // top-left buttons remain clickable even when press+release
            // happen between frames.
            let consumed = state.on_window_event(win, _event);
            if !consumed.consumed {
                if let winit::event::WindowEvent::MouseInput { state, button, .. } = _event {
                    if *button == MouseButton::Left {
                        let pressed = *state == ElementState::Pressed;
                        self.mouse_pressed = pressed;
                        if pressed {
                            // record press location for immediate hit-test on release
                            self.mouse_press_pos = self.last_cursor;
                        } else {
                            if let Some((px, py)) = self.mouse_press_pos.take() {
                                if let Some((rx, ry)) = self.last_cursor {
                                    // Prefer the stored egui response rects when available
                                    // so the immediate hit-test matches what was painted.
                                    let in_load_press = self.load_button_rect.map_or(
                                        px >= 8.0 && px <= 128.0 && py >= 8.0 && py <= 40.0,
                                        |r| r.contains(egui::pos2(px, py)),
                                    );
                                    let in_exit_press = self.exit_button_rect.map_or(
                                        px >= 8.0 && px <= 128.0 && py >= 48.0 && py <= 80.0,
                                        |r| r.contains(egui::pos2(px, py)),
                                    );
                                    let in_load_release = self.load_button_rect.map_or(
                                        rx >= 8.0 && rx <= 128.0 && ry >= 8.0 && ry <= 40.0,
                                        |r| r.contains(egui::pos2(rx, ry)),
                                    );
                                    let in_exit_release = self.exit_button_rect.map_or(
                                        rx >= 8.0 && rx <= 128.0 && ry >= 48.0 && ry <= 80.0,
                                        |r| r.contains(egui::pos2(rx, ry)),
                                    );
                                    if in_load_press && in_load_release {
                                        let _ = self.sender.send(UiEvent::LoadScene(
                                            PathBuf::from("saves/scene.bin"),
                                        ));
                                        // Close overlay internally and notify the application
                                        // to manage OS cursor/grab state via OverlayToggled.
                                        self.ui_visible = false;
                                        self.cursor_grabbed = Some(true);
                                        let _ = self.sender.send(UiEvent::OverlayToggled(false));
                                    } else if in_exit_press && in_exit_release {
                                        let _ = self.sender.send(UiEvent::Exit);
                                    }
                                }
                            }
                        }
                        // recorded mouse_pressed while forwarding
                    }
                }
            }

            // Escape handling intentionally left disabled for now.
        } else {
            // Only forward pointer events while the UI is visible.
            match _event {
                // If we don't have egui_winit yet, still allow Escape to
                // toggle the overlay so the user can bring up the UI early.
                WindowEvent::KeyboardInput { .. } => {
                    // Escape handling intentionally left disabled for now.
                }
                WindowEvent::CursorMoved { position, .. } if self.ui_visible => {
                    // Avoid logging each cursor movement; only occasionally
                    // update an internal timestamp for debug logs. We still
                    // forward the pointer to the raw_input accumulator.
                    let px = position.x as f32;
                    let py = position.y as f32;
                    if let Some(win) = &self.window {
                        let ppp = win.scale_factor() as f32;
                        self.last_cursor = Some((px / ppp, py / ppp));
                        self.raw_input
                            .events
                            .push(egui::Event::PointerMoved(egui::pos2(px / ppp, py / ppp)));
                    } else {
                        self.last_cursor = Some((px, py));
                        self.raw_input
                            .events
                            .push(egui::Event::PointerMoved(egui::pos2(px, py)));
                    }
                    // Throttle any debug logging to ~100ms.
                    // throttle cursor logging removed
                }
                WindowEvent::MouseInput { state, button, .. } if self.ui_visible => {
                    if *button == MouseButton::Left {
                        let pressed = *state == ElementState::Pressed;
                        self.mouse_pressed = pressed;
                        if pressed {
                            self.mouse_press_pos = self.last_cursor;
                        } else {
                            if let Some((px, py)) = self.mouse_press_pos.take() {
                                if let Some((rx, ry)) = self.last_cursor {
                                    // Prefer stored response rects when available to
                                    // match the painted debug geometry exactly.
                                    let in_load_press = self.load_button_rect.map_or(
                                        px >= 8.0 && px <= 128.0 && py >= 8.0 && py <= 40.0,
                                        |r| r.contains(egui::pos2(px, py)),
                                    );
                                    let in_exit_press = self.exit_button_rect.map_or(
                                        px >= 8.0 && px <= 128.0 && py >= 48.0 && py <= 80.0,
                                        |r| r.contains(egui::pos2(px, py)),
                                    );
                                    let in_load_release = self.load_button_rect.map_or(
                                        rx >= 8.0 && rx <= 128.0 && ry >= 8.0 && ry <= 40.0,
                                        |r| r.contains(egui::pos2(rx, ry)),
                                    );
                                    let in_exit_release = self.exit_button_rect.map_or(
                                        rx >= 8.0 && rx <= 128.0 && ry >= 48.0 && ry <= 80.0,
                                        |r| r.contains(egui::pos2(rx, ry)),
                                    );
                                    if in_load_press && in_load_release {
                                        let _ = self.sender.send(UiEvent::LoadScene(
                                            PathBuf::from("saves/scene.bin"),
                                        ));
                                        // Close overlay internally and notify the application
                                        // to manage OS cursor/grab state via OverlayToggled.
                                        self.ui_visible = false;
                                        self.cursor_grabbed = Some(true);
                                        let _ = self.sender.send(UiEvent::OverlayToggled(false));
                                    } else if in_exit_press && in_exit_release {
                                        let _ = self.sender.send(UiEvent::Exit);
                                    }
                                }
                            }
                        }

                        let pos = self
                            .last_cursor
                            .map(|(x, y)| egui::pos2(x, y))
                            .unwrap_or(egui::pos2(0.0, 0.0));
                        self.raw_input.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::default(),
                        });
                    }
                }
                WindowEvent::ModifiersChanged(_mods) if self.ui_visible => {
                    self.raw_input.modifiers = egui::Modifiers::default();
                }
                _ => {}
            }
        }
    }
}

impl Default for EguiUi {
    fn default() -> Self {
        let (s, _) = crossbeam_channel::unbounded();
        EguiUi {
            sender: s,
            renderer_initialized: false,
            window: None,
            last_cursor: None,
            mouse_pressed: false,
            mouse_was_pressed: false,
            mouse_press_pos: None,
            load_button_rect: None,
            exit_button_rect: None,
            egui_ctx: None,
            egui_winit: None,
            egui_renderer: None,
            staging_belt: None,
            surface_format: None,
            raw_input: egui::RawInput::default(),
            last_cursor_log: None,
            ui_visible: false,
            force_raw_input_next_frame: false,
            warmup_pending: false,
            start_mode: false,
            cursor_grabbed: None,
            current_menu: None,
        }
    }
}

impl FrameCallback for EguiUi {
    fn call(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _view: &wgpu::TextureView,
        _encoder: &mut wgpu::CommandEncoder,
    ) {
        if cfg!(feature = "ui-egui-debug") {
            // Re-assert cursor visibility/grab state if we have an opinion
            // about it. Other systems may race to change cursor grab, so try
            // to keep it in sync with the UI visibility here each frame.
            if let (Some(win), Some(grabbed)) = (self.window.as_ref(), self.cursor_grabbed) {
                // If `grabbed == true` we expect the cursor to be grabbed by
                // the app for camera control; otherwise the cursor should be
                // released for UI interaction.
                let want_visible = !grabbed;
                let _ = win.set_cursor_visible(want_visible);
                let _ = win.set_cursor_grab(if grabbed {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::None
                });
            }
            log::info!("EguiUi::call invoked; ui_visible={}", self.ui_visible);
        }

        // Lazy init placeholder
        if !self.renderer_initialized {
            // Initialize basic egui runtime pieces lazily so we have
            // access to the wgpu device/queue. Full renderer creation
            // will require the surface format; attempt to query it from
            // the engine renderer via the global `create_renderer` path
            // or the stored window. For now leave it as None and mark
            // initialized so subsequent passes can populate renderer.
            self.egui_ctx = Some(egui::Context::default());
            if cfg!(feature = "ui-egui-debug") {
                log::info!("EguiUi: created egui::Context");
            }
            // Defer creating `egui_winit::State` here because the
            // constructor changed in egui-winit v0.33. Initialize it
            // later using the examples in `/examples` which show the
            // correct signature and required parameters.
            // if let Some(win) = &self.window {
            //     self.egui_winit = Some(egui_winit::State::new(/* ... */));
            // }
            // staging belt and egui_renderer will be created in step C
            self.renderer_initialized = true;
        }

        // Only drive egui when the UI overlay is visible. When hidden we
        // skip rendering and hit-testing so the 3D scene receives input
        // as normal.
        if !self.ui_visible {
            log::debug!("EguiUi::call skipping because ui_visible=false");
        }

        if self.ui_visible {
            log::info!("EguiUi::call driving egui frame");

            // Track whether the UI closure emitted any actionable
            // UiEvent this frame. If so, skip the non-egui fallback
            // hit-test later to avoid duplicate events. Declare here
            // so both the warmup path and the normal frame path can
            // observe it.
            let mut ui_emitted_action = false;
            // If this is the first time we're showing the UI and a
            // warmup is pending, issue a flat clear so the application
            // starts with a deterministic background. This also helps
            // prime egui's layout pipeline for anchored Areas.
            if self.warmup_pending {
                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi: performing warmup clear/pass (start_mode={})",
                        self.start_mode
                    );
                }
                // Optionally use a magenta diagnostic clear behind a
                // feature flag for visual debugging. Otherwise do a
                // dark gray clear as the default start-menu background.
                let (r, g, b, a) = if cfg!(feature = "ui-egui-debug") {
                    (1.0, 0.0, 1.0, 1.0) // magenta
                } else {
                    (0.06, 0.06, 0.06, 1.0) // dark gray
                };

                // Create a short-lived render pass that clears the view
                // to a flat color so the first presented frame contains
                // the menu background immediately.
                let _rpass = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_warmup_clear_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: _view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                drop(_rpass);

                // Run a minimal egui frame to force layout/tessellation
                // so the first presented frame contains UI shapes and the
                // egui textures. Ensure the egui GPU renderer exists so
                // textures can be uploaded safely.
                // (ui_emitted_action declared above)

                if let Some(ctx) = &self.egui_ctx {
                    // If we know the surface format, ensure renderer exists
                    // before running the UI so textures_delta can be
                    // processed without panics.
                    if self.egui_renderer.is_none() {
                        if let Some(fmt) = self.surface_format {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!(
                                    "EguiUi: creating egui_wgpu::Renderer for warmup with format {:?}",
                                    fmt
                                );
                            }
                            self.egui_renderer =
                                Some(egui_wgpu::Renderer::new(_device, fmt, Default::default()));
                        }
                    }

                    // ...existing code...

                    let input = if let (Some(state), Some(win)) =
                        (self.egui_winit.as_mut(), self.window.as_ref())
                    {
                        state.take_egui_input(win)
                    } else {
                        egui::RawInput::default()
                    };

                    let full_output = ctx.run(input, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                ui.heading("Moho");
                                ui.add_space(8.0);
                                if ui.button("Start").clicked() {
                                    let _ = self
                                        .sender
                                        .send(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                                    // Close overlay internally and notify the application
                                    // so it can manage OS cursor/grab state centrally.
                                    self.ui_visible = false;
                                    self.cursor_grabbed = Some(true);
                                    let _ = self.sender.send(UiEvent::OverlayToggled(false));
                                }
                                ui.add_space(4.0);
                                if ui.button("Exit").clicked() {
                                    let _ = self.sender.send(UiEvent::Exit);
                                }
                            });
                            // Debug: draw non-egui fallback rectangles so we can
                            // visually confirm the clickable regions.
                            if cfg!(feature = "ui-egui-debug") {
                                let painter = ctx.layer_painter(egui::LayerId::new(
                                    egui::Order::Foreground,
                                    egui::Id::new("moho_debug_rects_warmup"),
                                ));
                                let load_rect = egui::Rect::from_min_max(
                                    egui::pos2(8.0, 8.0),
                                    egui::pos2(128.0, 40.0),
                                );
                                let exit_rect = egui::Rect::from_min_max(
                                    egui::pos2(8.0, 48.0),
                                    egui::pos2(128.0, 80.0),
                                );
                                painter.rect_filled(
                                    load_rect,
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(255, 0, 0, 60),
                                );
                                // outline omitted (egui API mismatch); filled rect is sufficient for debug
                                painter.rect_filled(
                                    exit_rect,
                                    0.0,
                                    egui::Color32::from_rgba_unmultiplied(0, 255, 0, 60),
                                );
                                // outline omitted (egui API mismatch); filled rect is sufficient for debug
                            }
                        });
                    });

                    // Process the warmup FullOutput similar to the
                    // regular frame path so textures are uploaded and
                    // buffers are prepared.
                    let pixels_per_point = full_output.pixels_per_point;
                    let textures_delta = full_output.textures_delta;
                    let shapes = full_output.shapes;

                    if cfg!(feature = "ui-egui-debug") {
                        log::info!(
                            "EguiUi (warmup): full_output shapes={}, textures_set={}, textures_free={}, ppp={}",
                            shapes.len(),
                            textures_delta.set.len(),
                            textures_delta.free.len(),
                            pixels_per_point
                        );
                    }

                    if let Some(renderer) = &mut self.egui_renderer {
                        for (id, image_delta) in textures_delta.set {
                            renderer.update_texture(_device, _queue, id, &image_delta);
                        }
                        for id in textures_delta.free {
                            renderer.free_texture(&id);
                        }

                        let clipped_primitives = ctx.tessellate(shapes, pixels_per_point);

                        // screen descriptor
                        let screen_desc = if let Some(win) = &self.window {
                            let size = win.inner_size();
                            egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [size.width as u32, size.height as u32],
                                pixels_per_point,
                            }
                        } else {
                            egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [800, 600],
                                pixels_per_point,
                            }
                        };

                        if self.staging_belt.is_none() {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: creating StagingBelt (warmup)");
                            }
                            self.staging_belt = Some(wgpu::util::StagingBelt::new(1024));
                        }

                        let _user_cmds = renderer.update_buffers(
                            _device,
                            _queue,
                            _encoder,
                            &clipped_primitives,
                            &screen_desc,
                        );

                        if let Some(belt) = &mut self.staging_belt {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: finishing staging belt (warmup)");
                            }
                            belt.finish();
                        }

                        let mut egui_rpass =
                            _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("egui_warmup_pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: _view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                occlusion_query_set: None,
                                timestamp_writes: None,
                            });

                        let egui_rpass_static: &mut wgpu::RenderPass<'static> =
                            unsafe { std::mem::transmute(&mut egui_rpass) };
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: calling egui_wgpu::Renderer::render (warmup)");
                        }
                        renderer.render(egui_rpass_static, &clipped_primitives, &screen_desc);
                        drop(egui_rpass);
                    }
                }

                // Mark warmup complete so normal frames render now.
                self.warmup_pending = false;
                // Done — don't continue into the regular UI rendering path
                // for this invocation; the warmup frame is presented.
                return;
            }
            // Drive egui: begin a frame with accumulated raw_input so egui
            // consumes forwarded events. We build a minimal menu with two
            // buttons (Load Scene, Exit) that send UiEvent messages.
            if let Some(ctx) = &self.egui_ctx {
                // Determine input via egui_winit when available (recommended),
                // otherwise fall back to the manually-accumulated RawInput.
                // If we have any manually-injected RawInput events (for
                // example a synthetic PointerMoved pushed when the overlay
                // was toggled), prefer consuming them this frame so egui
                // receives the initial pointer position immediately. This
                // makes the UI interactive on the first show instead of
                // waiting a frame.
                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi: raw_input.events.len() = {}",
                        self.raw_input.events.len()
                    );
                }

                let input = if self.force_raw_input_next_frame {
                    // Prefer to *merge* our synthetic raw_input events into
                    // egui_winit's input when possible so we retain the
                    // correct pixels_per_point and screen size information.
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: forcing raw_input consumption this frame");
                    }
                    self.force_raw_input_next_frame = false;
                    if let (Some(state), Some(win)) =
                        (self.egui_winit.as_mut(), self.window.as_ref())
                    {
                        let mut input = state.take_egui_input(win);
                        let mut manual = std::mem::take(&mut self.raw_input);
                        if MERGE_PREPEND {
                            // Prepend manual.events before input.events
                            let mut combined = manual.events;
                            combined.append(&mut input.events);
                            input.events = combined;
                        } else {
                            // Append manual.events after input.events
                            input.events.append(&mut manual.events);
                        }
                        input
                    } else {
                        // No egui_winit available, fall back to raw_input.
                        std::mem::take(&mut self.raw_input)
                    }
                } else if !self.raw_input.events.is_empty() {
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: taking input via raw_input (direct)");
                    }
                    std::mem::take(&mut self.raw_input)
                } else if let (Some(state), Some(win)) =
                    (self.egui_winit.as_mut(), self.window.as_ref())
                {
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: taking input via egui_winit::State");
                    }
                    state.take_egui_input(win)
                } else {
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: taking input via raw_input accumulator");
                    }
                    std::mem::take(&mut self.raw_input)
                };

                // Run egui using the collected input. `ctx.run` returns a `FullOutput`
                // containing shapes, textures_delta and other platform output.
                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi: about to call ctx.run; input.events.len()={}, screen_rect={:?}",
                        input.events.len(),
                        input.screen_rect
                    );
                    log::info!("EguiUi: input.events = {:?}", input.events);
                }

                // (ui_emitted_action declared above)

                let full_output = ctx.run(input, |ctx| {
                    // Delegate to the current menu if present. Menus return a
                    // MenuAction and optional response rects for fallback hit-tests.
                    if let Some(menu) = &mut self.current_menu {
                        let (action, rects) = menu.ui(ctx);
                        // Store rects for fallback hit-test if provided.
                        if let Some((load_r, exit_r)) = rects {
                            self.load_button_rect = Some(load_r);
                            self.exit_button_rect = Some(exit_r);
                        }
                        // Translate MenuAction into UiEvent and internal state.
                        match action {
                            crate::menus::menu::MenuAction::LoadScene(p) => {
                                let _ = self.sender.send(UiEvent::LoadScene(p));
                                self.ui_visible = false;
                                self.cursor_grabbed = Some(true);
                                let _ = self.sender.send(UiEvent::OverlayToggled(false));
                                ui_emitted_action = true;
                            }
                            crate::menus::menu::MenuAction::Exit => {
                                let _ = self.sender.send(UiEvent::Exit);
                                ui_emitted_action = true;
                            }
                            _ => {}
                        }
                    }
                    // Menus are responsible for painting their UI; no further
                    // inline widgets are needed here.
                });

                // Break out the FullOutput into parts we need so we can inspect
                // shapes without moving them prematurely.
                let pixels_per_point = full_output.pixels_per_point;
                let textures_delta = full_output.textures_delta;
                let shapes = full_output.shapes;

                // Normal full_output handling; debug logs removed for clean run

                // Ensure egui_wgpu renderer is created when we know the surface format
                if self.egui_renderer.is_none() {
                    if let Some(fmt) = self.surface_format {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!(
                                "Initializing egui_wgpu::Renderer with surface format {:?}",
                                fmt
                            );
                        }
                        // Use default options; egui-wgpu exposes a Renderer::new that accepts defaults
                        self.egui_renderer =
                            Some(egui_wgpu::Renderer::new(_device, fmt, Default::default()));
                    } else {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!(
                                "EguiUi: egui_renderer not created yet; surface_format missing"
                            );
                        }
                    }
                }

                // Handle textures delta (upload / free)
                if let Some(renderer) = &mut self.egui_renderer {
                    for (id, image_delta) in textures_delta.set {
                        renderer.update_texture(_device, _queue, id, &image_delta);
                    }
                    for id in textures_delta.free {
                        renderer.free_texture(&id);
                    }

                    // Tessellate shapes into paint jobs.
                    let clipped_primitives = ctx.tessellate(shapes, pixels_per_point);

                    // Prepare screen descriptor from the window if available
                    let screen_desc = if let Some(win) = &self.window {
                        let size = win.inner_size();
                        egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [size.width, size.height],
                            pixels_per_point,
                        }
                    } else {
                        egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [800, 600],
                            pixels_per_point,
                        }
                    };

                    // Upload buffers (writes to `encoder`). It may return user command buffers which we ignore here.
                    // Ensure a staging belt exists for uploads and buffer staging.
                    if self.staging_belt.is_none() {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: creating StagingBelt");
                        }
                        self.staging_belt = Some(wgpu::util::StagingBelt::new(1024));
                    }

                    let _user_cmds = renderer.update_buffers(
                        _device,
                        _queue,
                        _encoder,
                        &clipped_primitives,
                        &screen_desc,
                    );

                    // Finish the staging belt so its work is submitted with the
                    // command encoder. The application must call
                    // `recall_staging_belt()` on this adapter immediately after
                    // it calls `queue.submit(...)` so the belt can reclaim memory.
                    if let Some(belt) = &mut self.staging_belt {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: finishing staging belt");
                        }
                        belt.finish();
                    }

                    // Create a render pass and hand it to egui renderer. We create the same render pass
                    // used earlier for the placeholder; to avoid duplicating clear, we create a fresh one
                    // here that assumes the view already has the scene drawn behind it.
                    let mut egui_rpass = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("egui_ui_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: _view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });

                    // egui-wgpu expects a 'static render pass reference. Do the unsafe transmute trick
                    // (the render pass remains valid until dropped below).
                    let egui_rpass_static: &mut wgpu::RenderPass<'static> =
                        unsafe { std::mem::transmute(&mut egui_rpass) };
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: calling egui_wgpu::Renderer::render");
                    }
                    renderer.render(egui_rpass_static, &clipped_primitives, &screen_desc);
                    // Drop the render pass (and transmute alias) by letting `egui_rpass` go out of scope.
                    drop(egui_rpass);
                }
            }

            // If egui_wgpu is not initialized yet, don't clear the
            // scene — instead create a no-op load pass so the 3D
            // scene remains visible beneath the soon-to-be-painted UI.
            let rpass = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_ui_placeholder_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: _view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            drop(rpass);

            // Simple, non-egui UI hit-testing: two rectangular buttons in the
            // top-left corner. Coordinates are in logical pixels and assume the
            // window's origin at (0,0). This allows the engine to receive
            // actionable UI events before full egui rendering is wired.
            //
            // Button layout:
            //  - Load Scene: rect [8,8 .. 128,40]
            //  - Exit:       rect [8,48 .. 128,80]
            // Detect a click that may have occurred between frames. Prefer
            // to use the recorded press position (`mouse_press_pos`) so we
            // can distinguish horizontal vs vertical layouts accurately.
            if self.mouse_was_pressed && !self.mouse_pressed {
                if let Some((px, py)) = self.mouse_press_pos.take() {
                    if let Some((rx, ry)) = self.last_cursor {
                        let in_load_press = self
                            .load_button_rect
                            .is_some_and(|r| r.contains(egui::pos2(px, py)));
                        let in_exit_press = self
                            .exit_button_rect
                            .is_some_and(|r| r.contains(egui::pos2(px, py)));
                        let in_load_release = self
                            .load_button_rect
                            .is_some_and(|r| r.contains(egui::pos2(rx, ry)));
                        let in_exit_release = self
                            .exit_button_rect
                            .is_some_and(|r| r.contains(egui::pos2(rx, ry)));

                        // If egui already emitted an action this frame, avoid
                        // firing the non-egui fallback duplicate.
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!(
                                "EguiUi(fallback): press=({}, {}), release=({}, {}) load_rect={:?}, exit_rect={:?}",
                                px,
                                py,
                                rx,
                                ry,
                                self.load_button_rect,
                                self.exit_button_rect
                            );
                        }
                        if !ui_emitted_action {
                            if in_load_press && in_load_release {
                                // Send a LoadScene with a default path
                                let _ = self
                                    .sender
                                    .send(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                                // Close overlay internally and notify the app so it can
                                // manage OS cursor/grab state via OverlayToggled.
                                self.ui_visible = false;
                                self.cursor_grabbed = Some(true);
                                let _ = self.sender.send(UiEvent::OverlayToggled(false));
                            } else if in_exit_press && in_exit_release {
                                if cfg!(feature = "ui-egui-debug") {
                                    log::info!(
                                        "EguiUi: non-egui fallback Exit clicked (noop for debug)"
                                    );
                                }
                            }
                        } else if in_exit_press && in_exit_release {
                            // If egui already emitted an action, still allow
                            // exit to be logged here when egui didn't.
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!(
                                    "EguiUi: non-egui fallback Exit clicked (noop for debug)"
                                );
                            }
                        }
                    }
                } else if let Some((x, y)) = self.last_cursor {
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!(
                            "EguiUi(fallback release-only): release=({}, {}), load_rect={:?}, exit_rect={:?}",
                            x,
                            y,
                            self.load_button_rect,
                            self.exit_button_rect
                        );
                    }
                    // Fallback: we only have the release position available.
                    let in_load = self
                        .load_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(x, y)));
                    let in_exit = self
                        .exit_button_rect
                        .is_some_and(|r| r.contains(egui::pos2(x, y)));
                    if !ui_emitted_action {
                        if in_load {
                            let _ = self
                                .sender
                                .send(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                            self.ui_visible = false;
                            self.cursor_grabbed = Some(true);
                            let _ = self.sender.send(UiEvent::OverlayToggled(false));
                        } else if in_exit {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!(
                                    "EguiUi: non-egui fallback Exit clicked (noop for debug)"
                                );
                            }
                        }
                    } else if in_exit {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: non-egui fallback Exit clicked (noop for debug)");
                        }
                    }
                }
            }
        }

        // Track previous press state for click detection on next frame.
        self.mouse_was_pressed = self.mouse_pressed;
    }
}
