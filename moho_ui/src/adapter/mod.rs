//! Modern, modular egui adapter for moho UI
//!
//! This adapter manages menus and UI state in a scalable way,
//! allowing easy addition of new menus and menu types.
mod event_routing;
mod gpu_ops;
mod rendering;

use crate::prefs::Prefs;
use crate::screens::{Menu, MenuAction};
use crate::ui_state::UiStateManager;
use moho_renderer::FrameCallback;
pub use moho_types::GameState;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event::WindowEvent;
use winit::window::Window;

/// UI events sent to the application
#[derive(Debug, Clone)]
pub enum UiEvent {
    LoadScene(PathBuf),
    /// Start generation of a new world with provided parameters
    NewWorld(moho_core::scene_builders::WorldSpec),
    ShowMenu(String),
    Exit,
    OverlayToggled(bool),
    SettingsSaved(Prefs),
    AudioEvent(UiAudioEvent),
}

/// Audio events from UI interactions
#[derive(Debug, Clone)]
pub enum UiAudioEvent {
    ButtonClick,
    MenuNavigate,
    Confirm,
    Cancel,
    Error,
}

pub type UiReceiver = crossbeam_channel::Receiver<UiEvent>;

/// Global flag for UI overlay visibility
pub static UI_OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Modern egui adapter that manages multiple menus
pub struct EguiAdapter {
    // egui integration
    context: egui::Context,
    winit_state: Option<egui_winit::State>,
    renderer: Option<egui_wgpu::Renderer>,

    // UI state management (extracted from adapter)
    ui_state: UiStateManager,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Window reference
    window: Option<Arc<Window>>,

    // Game state (used to determine what to render)
    // This is updated by the main app before each frame
    current_game_state: GameState,

    // Optional progress overlay state. When Some, the adapter will render a
    // simple modal progress overlay showing percent complete and an optional
    // cancel button. The main application can control this by locking the
    // adapter and calling the helper methods below.
    pub progress: Option<ProgressState>,

    // Rendering
    surface_config: Option<wgpu::SurfaceConfiguration>,

    // Hover SFX tracking: stores the action key of the last-hovered menu item
    // so we only fire MenuNavigate once per new hover, not every frame.
    last_hovered_key: Option<String>,

    // Whether menu music is currently playing (to avoid redundant start/stop)
    menu_music_playing: bool,
}

/// Re-export from moho_types \u2014 single source of truth for game states.\npub use moho_types::GameState;\n\n/// Lightweight progress state used by the adapter to render an overlay.
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub title: String,
    pub percent: f32, // 0.0 ..= 1.0
    pub cancellable: bool,
    pub canceled: bool,
}

// SAFETY: EguiAdapter is always accessed under Arc<Mutex<EguiAdapter>>, so
// only one thread holds &mut EguiAdapter at a time. The non-Send/Sync field is
// `dyn Modal` (inside UiStateManager), which is heap-allocated screen state that
// is created, used, and dropped on the main thread. No EguiAdapter field is ever
// accessed concurrently — the Mutex provides the needed exclusion.
// This impl is required because `dyn Modal` lacks a `Send` bound.
unsafe impl Send for EguiAdapter {}
unsafe impl Sync for EguiAdapter {}

impl EguiAdapter {
    /// Create a new adapter with event bus
    pub fn new(window: Option<Arc<Window>>, event_bus: Arc<moho_core::EventBus>) -> Self {
        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());

        // Initialize winit state if we have a window
        let winit_state = window.as_ref().map(|w| {
            egui_winit::State::new(
                context.clone(),
                egui::ViewportId::ROOT,
                w.as_ref(),
                None,
                None,
                None,
            )
        });

        Self {
            context,
            winit_state,
            renderer: None,
            ui_state: UiStateManager::new(),
            event_bus,
            window,
            current_game_state: GameState::Menu,
            surface_config: None,
            progress: None,
            last_hovered_key: None,
            menu_music_playing: false,
        }
    }

    /// Update the current game state (called by main app each frame)
    pub fn set_game_state(&mut self, state: GameState) {
        // Reset console state when entering ConsoleOpen to prevent immediate close
        if state == GameState::ConsoleOpen && self.current_game_state != GameState::ConsoleOpen {
            self.ui_state.console.reset_on_open();
        }
        self.current_game_state = state;
    }

    /// Add a new menu to the manager
    pub fn add_menu(&mut self, name: String, menu: Box<dyn Menu>) {
        self.ui_state.add_screen(name, menu);
    }

    /// Show a specific menu
    pub fn show_menu(&mut self, name: &str) {
        self.ui_state.show_screen(name);
    }

    /// Hide all menus
    pub fn hide_menus(&mut self) {
        self.ui_state.hide_all();
    }

    /// Handle window events (mouse, keyboard).
    /// Returns true if the event was consumed by egui/winit state.
    pub fn handle_winit_event(&mut self, event: &WindowEvent) -> bool {
        if let Some(state) = &mut self.winit_state {
            // on_window_event returns an EventResponse; use its consumed flag
            let resp = state.on_window_event(self.window.as_ref().unwrap(), event);
            return resp.consumed;
        }
        false
    }

    /// Return true if the active menu/screen captures raw input events.
    pub fn active_screen_captures_input(&self) -> bool {
        if let Some(screen) = self.ui_state.active_screen() {
            return screen.captures_raw_input();
        }
        false
    }

    /// Try to let the active menu/screen process raw WindowEvent input.
    /// Returns true if the event was consumed.
    pub fn try_handle_screen_input(&mut self, event: &WindowEvent) -> bool {
        if let Some(screen) = self.ui_state.active_screen_mut() {
            return screen.handle_raw_input(event);
        }
        false
    }

    /// Returns whether the UI is currently visible
    pub fn is_visible(&self) -> bool {
        self.ui_state.visible
    }

    /// Sets the UI visibility
    pub fn set_visible(&mut self, visible: bool) {
        self.ui_state.visible = visible;
    }

    /// Set surface format for renderer initialization
    pub fn set_surface_format(&mut self, format: wgpu::TextureFormat) {
        self.surface_config = Some(wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        });
    }

    /// Initialize the wgpu renderer
    fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.renderer.is_none() {
            let renderer =
                egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
            self.renderer = Some(renderer);
            log::info!("Initializing egui_wgpu::Renderer with format {:?}", format);
        }
    }

    /// Start showing a progress overlay with the given title. This will
    /// overwrite any existing progress state.
    pub fn start_progress(&mut self, title: impl Into<String>, cancellable: bool) {
        self.progress = Some(ProgressState {
            title: title.into(),
            percent: 0.0,
            cancellable,
            canceled: false,
        });
        self.ui_state.visible = true;
    }

    /// Update the visible progress fraction (0.0 ..= 1.0)
    pub fn set_progress(&mut self, percent: f32) {
        if let Some(p) = &mut self.progress {
            p.percent = percent.clamp(0.0, 1.0);
        }
    }

    /// Mark the progress overlay finished and remove it from view.
    pub fn finish_progress(&mut self) {
        self.progress = None;
    }

    /// Check whether the user clicked cancel on the progress overlay. This
    /// returns true once and clears the flag; it is safe for the caller to
    /// poll periodically while a long-running task is in progress.
    pub fn take_progress_canceled(&mut self) -> bool {
        if let Some(p) = &mut self.progress {
            let was = p.canceled;
            p.canceled = false;
            was
        } else {
            false
        }
    }

    /// Get input from egui_winit
    fn take_egui_input(&mut self) -> egui::RawInput {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.take_egui_input(window.as_ref())
        } else {
            egui::RawInput::default()
        }
    }

    /// Handle platform output from egui
    fn handle_platform_output(&mut self, platform_output: egui::PlatformOutput) {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.handle_platform_output(window.as_ref(), platform_output);
        }
    }

    /// Recall staging belt for memory management
    pub fn recall_staging_belt(&mut self) {
        // This is called by the main application after rendering
        // No-op for now, but could be used for cleanup
    }

    /// Update overlay HUD data for the current frame.
    pub fn update_hud_data(&mut self, data: crate::overlays::HudData) {
        self.ui_state.overlay_manager.update_data(data);
    }

    /// Toggle the debug HUD overlay (F3).
    pub fn toggle_debug_hud(&mut self) {
        self.ui_state.overlay_manager.toggle("debug");
    }
}

impl FrameCallback for EguiAdapter {
    fn call(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        surface_width: u32,
        surface_height: u32,
    ) {
        // Update global visibility flag (tracks menu visibility for input routing)
        UI_OVERLAY_VISIBLE.store(self.ui_state.visible, Ordering::SeqCst);

        // Render when menus are visible OR during any gameplay state (for overlays)
        let needs_render = self.ui_state.visible || self.current_game_state != GameState::Menu;
        if !needs_render {
            return;
        }

        // Initialize renderer if needed
        let format = self
            .surface_config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        self.init_renderer(device, format);

        // Take input from winit integration
        let raw_input = self.take_egui_input();

        // Run egui and collect menu actions + modal result
        let mut menu_actions = Vec::new();
        let mut new_hovered_key: Option<String> = None;
        let mut modal_result = crate::modal::ModalResult::None;

        let full_output = self.context.run(raw_input, |ctx| {
            // Render based on current game state (delegates to rendering module)
            let result = rendering::render_game_state(
                ctx,
                &mut self.ui_state,
                self.current_game_state,
                &self.event_bus,
            );
            menu_actions.extend(result.actions);
            new_hovered_key = result.hovered_key;

            // Check if any screen wants to show a modal
            if let Some(screen) = self.ui_state.active_screen_mut()
                && let Some(modal) = screen.take_pending_modal()
            {
                self.ui_state.modal_manager.show(modal);
            }

            // Render modal on top of menu (if active)
            if self.current_game_state == GameState::Menu {
                modal_result = self.ui_state.modal_manager.render(ctx);
            }

            // Render progress overlay (if present)
            if let Some(progress) = self.progress.as_mut() {
                rendering::render_progress_overlay(ctx, progress);
            }
        });

        // Handle modal result
        use crate::modal::ModalResult;
        match modal_result {
            ModalResult::Confirm => {
                if let Some(screen) = self.ui_state.active_screen_mut() {
                    screen.on_modal_confirm();
                }
            }
            ModalResult::Cancel => {
                if let Some(screen) = self.ui_state.active_screen_mut() {
                    screen.on_modal_cancel();
                }
            }
            ModalResult::None => {}
        }

        // Hover SFX: emit MenuNavigate exactly once when hover changes to a new item
        if new_hovered_key != self.last_hovered_key && new_hovered_key.is_some() {
            event_routing::emit_audio_event(&self.event_bus, UiAudioEvent::MenuNavigate);
        }
        self.last_hovered_key = new_hovered_key;

        // Process menu actions (delegates to event_routing module)
        for action in &menu_actions {
            event_routing::process_menu_action(action, &self.event_bus);
        }

        // Handle Close action locally (hide menus)
        for action in &menu_actions {
            if matches!(action, MenuAction::Close) {
                self.hide_menus();
                break;
            }
        }

        // Background music: start when entering start menu, stop when leaving
        event_routing::update_menu_music(
            &menu_actions,
            &self.event_bus,
            &mut self.menu_music_playing,
        );

        // Handle platform output
        self.handle_platform_output(full_output.platform_output);

        // Render to GPU (delegates to gpu_ops module)
        if let Some(renderer) = &mut self.renderer {
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [surface_width, surface_height],
                pixels_per_point: self.context.pixels_per_point(),
            };

            let clipped_primitives = self
                .context
                .tessellate(full_output.shapes, full_output.pixels_per_point);

            // Update textures (delegate)
            gpu_ops::update_textures(renderer, device, queue, &full_output.textures_delta);

            // Update buffers (delegate)
            gpu_ops::update_buffers(
                renderer,
                device,
                queue,
                encoder,
                &clipped_primitives,
                &screen_descriptor,
            );

            // Execute render pass (delegate)
            gpu_ops::execute_render_pass(
                renderer,
                encoder,
                view,
                &clipped_primitives,
                &screen_descriptor,
            );

            // Free textures (delegate)
            gpu_ops::free_textures(renderer, &full_output.textures_delta.free);
        }
    }
}

/// Build adapter function
pub fn build_adapter(
    window: Option<Arc<Window>>,
    event_bus: Arc<moho_core::EventBus>,
) -> EguiAdapter {
    EguiAdapter::new(window, event_bus)
}
