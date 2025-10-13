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
    NewWorld,
    ShowMenu(String),
    Exit,
    /// Overlay visibility changed: true = shown, false = hidden
    OverlayToggled(bool),
}

// Test helpers for external tests. These helpers are useful for the
// integration tests but should not be part of the public production API.
// Expose them only when running tests or when the `ui-egui-test` feature
// is enabled for developer convenience.
#[cfg(any(test, feature = "ui-egui-test"))]
impl EguiUi {
    /// Set stored response rects (as if painted by egui). Order: continue, newworld, settings, exit
    pub fn test_set_button_rects(&mut self, cont: egui::Rect, neww: egui::Rect, set: egui::Rect, exit: egui::Rect) {
        self.menu_items = vec![
            crate::menus::menu::MenuItem { action: crate::menus::menu::MenuAction::LoadScene(PathBuf::from("saves/scene.bin")), rect: Some(cont), enabled: true, clicked: false },
            crate::menus::menu::MenuItem { action: crate::menus::menu::MenuAction::NewWorld, rect: Some(neww), enabled: true, clicked: false },
            crate::menus::menu::MenuItem { action: crate::menus::menu::MenuAction::ShowMenu("settings".to_string()), rect: Some(set), enabled: true, clicked: false },
            crate::menus::menu::MenuItem { action: crate::menus::menu::MenuAction::Exit, rect: Some(exit), enabled: true, clicked: false },
        ];
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
                    // Iterate items and check which one contains both press and release
                    for item in &self.menu_items {
                        if let Some(r) = item.rect {
                            let in_press = r.contains(egui::pos2(px, py));
                            let in_release = r.contains(egui::pos2(rx, ry));
                            if in_press && in_release && item.enabled {
                                match &item.action {
                                    crate::menus::menu::MenuAction::LoadScene(p) => {
                                        out.push(UiEvent::LoadScene(p.clone()));
                                        // Also indicate the overlay was toggled off like the real fallback
                                        out.push(UiEvent::OverlayToggled(false));
                                    }
                                    crate::menus::menu::MenuAction::NewWorld => {
                                        out.push(UiEvent::NewWorld);
                                        out.push(UiEvent::OverlayToggled(false));
                                    }
                                    crate::menus::menu::MenuAction::Exit => {
                                        out.push(UiEvent::Exit);
                                    }
                                    _ => {}
                                }
                                break;
                            }
                        }
                    }
                }
            } else if let Some((x, y)) = self.last_cursor {
                for item in &self.menu_items {
                    if let Some(r) = item.rect {
                        if r.contains(egui::pos2(x, y)) && item.enabled {
                            match &item.action {
                                crate::menus::menu::MenuAction::LoadScene(p) => {
                                    out.push(UiEvent::LoadScene(p.clone()));
                                    self.ui_visible = false;
                                    self.cursor_grabbed = Some(true);
                                    out.push(UiEvent::OverlayToggled(false));
                                }
                                crate::menus::menu::MenuAction::NewWorld => {
                                    out.push(UiEvent::NewWorld);
                                    self.ui_visible = false;
                                    self.cursor_grabbed = Some(true);
                                    out.push(UiEvent::OverlayToggled(false));
                                }
                                crate::menus::menu::MenuAction::Exit => {
                                    out.push(UiEvent::Exit);
                                }
                                _ => {}
                            }
                            break;
                        }
                    }
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

/// Convenience factory that mirrors the previous public API: construct an
/// `EguiUi` and its associated `UiReceiver` channel. This centralizes
/// adapter construction so callers (for example `main.rs`) don't need to
/// know the internal constructor details.
pub fn build_adapter(window: Option<Arc<Window>>) -> (EguiUi, UiReceiver) {
    EguiUi::new(window)
}

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
    // Last-known menu items with optional response rects for the fallback hit-test
    menu_items: Vec<crate::menus::menu::MenuItem>,
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
    // Last known pixels_per_point from the most recent egui frame (for coord transforms)
    last_pixels_per_point: Option<f32>,
    // Last seen winit device id for the pointer (used when synthesizing events)
    last_device_id: Option<winit::event::DeviceId>,
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
    // Frame counter for diagnostics (monotonic per-call)
    frame_id: u64,
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
            menu_items: Vec::new(),
            // Start the application with the start menu visible so the
            // user sees a deterministic menu on launch.
            ui_visible: true,
            egui_ctx,
            egui_winit: egui_winit_state,
            egui_renderer: None,
            staging_belt: None,
            surface_format: None,
            raw_input: egui::RawInput::default(),
            last_pixels_per_point: None,
            last_cursor_log: None,
            force_raw_input_next_frame: false,
            warmup_pending: true,
            start_mode: true,
            // cursor_grabbed already set above
            cursor_grabbed: None,
            // initialize menu system with the start menu
            current_menu: Some(Box::new(crate::menus::StartMenu::new())),
            frame_id: 0,
            last_device_id: None,
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

    pub fn handle_winit_event(&mut self, event: &winit::event::WindowEvent) {
        // Always forward all winit events to egui_winit::State, as in eframe/egui examples.
        if self.egui_winit.is_none() {
            if let (Some(ctx), Some(win)) = (self.egui_ctx.clone(), self.window.as_ref()) {
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
        if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
            let _ = state.on_window_event(win, event);
        }
    }
}

impl EguiUi {
    /// Handle input using fallback system when egui_winit doesn't consume events
    fn handle_fallback_input(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::{ElementState, MouseButton, WindowEvent};
        
        match event {
            WindowEvent::MouseInput { device_id, state, button, .. } => {
                if *button == MouseButton::Left {
                    let pressed = *state == ElementState::Pressed;
                    self.mouse_pressed = pressed;
                    self.last_device_id = Some(device_id.clone());
                    
                    if pressed {
                        self.mouse_press_pos = self.last_cursor;
                        log::trace!("handle_fallback_input: MouseInput Pressed - pos={:?}", self.mouse_press_pos);
                    } else {
                        // Handle release - check for menu actions
                        if let Some((px, py)) = self.mouse_press_pos.take() {
                            if let Some((rx, ry)) = self.last_cursor {
                                if let Some(action) = self.detect_menu_action_press_release(px, py, rx, ry) {
                                    log::info!("EguiUi(fallback): dispatching {:?}", action);
                                    self.dispatch_menu_action(action);
                                }
                            }
                        }
                        log::trace!("handle_fallback_input: MouseInput Released - pos={:?}", self.last_cursor);
                    }
                }
            }
            _ => {}
        }
    }

    /// Dispatch a menu action and handle UI state changes
    fn dispatch_menu_action(&mut self, action: crate::menus::menu::MenuAction) {
        match action {
            crate::menus::menu::MenuAction::LoadScene(p) => {
                let _ = self.sender.send(UiEvent::LoadScene(p));
                self.ui_visible = false;
                self.cursor_grabbed = Some(true);
                let _ = self.sender.send(UiEvent::OverlayToggled(false));
            }
            crate::menus::menu::MenuAction::NewWorld => {
                let _ = self.sender.send(UiEvent::NewWorld);
                self.ui_visible = false;
                self.cursor_grabbed = Some(true);
                let _ = self.sender.send(UiEvent::OverlayToggled(false));
            }
            crate::menus::menu::MenuAction::Exit => {
                let _ = self.sender.send(UiEvent::Exit);
            }
            _ => {}
        }
    }

    /// Detect a menu action when both press and release positions are known.
    /// Returns the first matching MenuAction (cloned) if any item contains
    /// both positions and is enabled.
    fn detect_menu_action_press_release(
        &self,
        px: f32,
        py: f32,
        rx: f32,
        ry: f32,
    ) -> Option<crate::menus::menu::MenuAction> {
            log::trace!(
                "detect_menu_action_press_release: press=({},{}) release=({},{}) pixels_per_point={:?}",
                px,
                py,
                rx,
                ry,
                self.last_pixels_per_point
            );
        for item in &self.menu_items {
            if !item.enabled {
                continue;
            }
            if let Some(r) = item.rect {
                let press_in = r.contains(egui::pos2(px, py));
                let release_in = r.contains(egui::pos2(rx, ry));
                log::trace!(
                    "detect_press_release: action={:?} rect={:?} press=({}, {}) in={} release=({}, {}) in={}",
                    item.action,
                    r,
                    px,
                    py,
                    press_in,
                    rx,
                    ry,
                    release_in
                );
                if press_in && release_in {
                    return Some(item.action.clone());
                }
            }
        }
        None
    }

    /// Detect a menu action when only the release position is available.
    fn detect_menu_action_release_only(&self, x: f32, y: f32) -> Option<crate::menus::menu::MenuAction> {
            log::trace!(
                "detect_menu_action_release_only: release=({},{}) pixels_per_point={:?}",
                x,
                y,
                self.last_pixels_per_point
            );
        for item in &self.menu_items {
            if !item.enabled {
                continue;
            }
            if let Some(r) = item.rect {
                let in_release = r.contains(egui::pos2(x, y));
                log::trace!(
                    "detect_release_only: action={:?} rect={:?} release=({}, {}) in={}",
                    item.action,
                    r,
                    x,
                    y,
                    in_release
                );
                if in_release {
                    return Some(item.action.clone());
                }
            }
        }
        None
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
            menu_items: Vec::new(),
            egui_ctx: None,
            egui_winit: None,
            egui_renderer: None,
            staging_belt: None,
            surface_format: None,
            raw_input: egui::RawInput::default(),
            last_cursor_log: None,
            last_pixels_per_point: None,
            ui_visible: false,
            force_raw_input_next_frame: false,
            warmup_pending: false,
            start_mode: false,
            cursor_grabbed: None,
            current_menu: None,
            frame_id: 0,
            last_device_id: None,
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

            let mut ui_emitted_action = false;

            if self.warmup_pending {
                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi: performing warmup clear/pass (start_mode={})",
                        self.start_mode
                    );
                }
                let (r, g, b, a) = if cfg!(feature = "ui-egui-debug") {
                    (1.0, 0.0, 1.0, 1.0)
                } else {
                    (0.06, 0.06, 0.06, 1.0)
                };
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
                self.warmup_pending = false;
                return;
            }

            // Always use egui_winit::State.take_egui_input as in egui/eframe examples.
            let input = if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
                state.take_egui_input(win)
            } else {
                egui::RawInput::default()
            };

                if let Some(ctx) = &self.egui_ctx {
                // Use a temporary variable to capture menu items and actions
                let mut clicked_actions = Vec::new();
                
                let full_output = ctx.run(input, |ctx| {
                    // --- Begin menu/overlay/debug logic (moved inside closure) ---
                    // Render the menu and get menu items
                    let items = if let Some(menu) = self.current_menu.as_mut() {
                        menu.ui(ctx)
                    } else {
                        Vec::new()
                    };
                    
                    // Check for clicked items - collect actions to handle outside closure
                    for item in &items {
                        if item.clicked {
                            log::debug!("Menu button clicked: {:?}", item.action);
                            ui_emitted_action = true;
                            clicked_actions.push(item.action.clone());
                        }
                    }
                    
                    // Store the items 
                    self.menu_items = items;
                    
                    // Debug overlay: draw menu item rects and canonical press/release positions
                    if cfg!(feature = "ui-egui-debug") {
                        let layer_id = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("moho_debug_overlay"));
                        let painter = ctx.layer_painter(layer_id);
                        for it in &self.menu_items {
                            if let Some(r) = it.rect {
                                let col = if it.enabled {
                                    egui::Color32::from_rgb(0x00, 0xC8, 0x8A)
                                } else {
                                    egui::Color32::from_rgb(0x80, 0x80, 0x80)
                                };
                                let fill = egui::Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 48);
                                painter.rect_filled(r, 4.0, fill);
                                let label = format!("{:?}", it.action);
                                painter.text(r.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, label, egui::FontId::default(), egui::Color32::WHITE);
                            }
                        }
                        if let Some((px, py)) = self.mouse_press_pos {
                            let pos = egui::pos2(px, py);
                            painter.circle_filled(pos, 6.0, egui::Color32::from_rgb(0xFF, 0x60, 0x60));
                            painter.circle_stroke(pos, 6.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                            painter.text(pos + egui::vec2(8.0, -8.0), egui::Align2::LEFT_CENTER, format!("press: ({:.0},{:.0})", px, py), egui::FontId::monospace(12.0), egui::Color32::WHITE);
                        }
                        if let Some((cx, cy)) = self.last_cursor {
                            let pos = egui::pos2(cx, cy);
                            painter.circle_filled(pos, 4.0, egui::Color32::from_rgb(0x40, 0x80, 0xFF));
                            painter.circle_stroke(pos, 4.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                            painter.text(pos + egui::vec2(8.0, 8.0), egui::Align2::LEFT_CENTER, format!("cursor: ({:.0},{:.0})", cx, cy), egui::FontId::monospace(12.0), egui::Color32::WHITE);
                        }
                        let info = format!("frame={} press={:?} cursor={:?}", self.frame_id, self.mouse_press_pos, self.last_cursor);
                        let info_pos = egui::pos2(8.0, 8.0);
                        let bg = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 160);
                        let rect = egui::Rect::from_min_size(info_pos, egui::vec2(350.0, 32.0));
                        painter.rect_filled(rect, 4.0, bg);
                        painter.text(info_pos + egui::vec2(6.0, 6.0), egui::Align2::LEFT_TOP, info, egui::FontId::monospace(12.0), egui::Color32::WHITE);
                    }
                    // --- End menu/overlay/debug logic ---
                });

                // Handle clicked actions outside the closure
                for action in clicked_actions {
                    match action {
                        crate::menus::menu::MenuAction::LoadScene(p) => {
                            let _ = self.sender.send(UiEvent::LoadScene(p));
                            self.ui_visible = false;
                            self.cursor_grabbed = Some(true);
                            let _ = self.sender.send(UiEvent::OverlayToggled(false));
                        }
                        crate::menus::menu::MenuAction::NewWorld => {
                            let _ = self.sender.send(UiEvent::NewWorld);
                            self.ui_visible = false;
                            self.cursor_grabbed = Some(true);
                            let _ = self.sender.send(UiEvent::OverlayToggled(false));
                        }
                        crate::menus::menu::MenuAction::Exit => {
                            let _ = self.sender.send(UiEvent::Exit);
                        }
                        _ => {}
                    }
                }

                // Log platform_output for the frame so we can see if egui
                // reported pointer/button interactions or requested commands
                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi(frame={}): full_output.platform_output present",
                        self.frame_id,
                    );
                }

                // Break out the FullOutput into parts we need so we can inspect
                // shapes without moving them prematurely.
                let pixels_per_point = full_output.pixels_per_point;
                // Persist pixels_per_point for diagnostics and potential transforms
                self.last_pixels_per_point = Some(pixels_per_point);
                log::trace!("EguiUi: pixels_per_point = {}", pixels_per_point);
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

        }

        // Debug-only: request a repaint each frame so the debug overlay
        // updates continuously and reflects canonical vs adapter positions
        // even if the application's normal repaint scheduling is coarser.
        if cfg!(feature = "ui-egui-debug") {
            if let Some(ctx) = &self.egui_ctx {
                ctx.request_repaint();
            }
        }

        // Track previous press state for click detection on next frame.
        self.mouse_was_pressed = self.mouse_pressed;
    }
}
