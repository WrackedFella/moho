//! Modern, modular egui adapter for moho UI
//!
//! This adapter manages menus and UI state in a scalable way,
//! allowing easy addition of new menus and menu types.

use crate::prefs::Prefs;
use crate::screens::{Menu, MenuAction};
use crate::ui_state::UiStateManager;
use moho_renderer::FrameCallback;
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
}

/// Game state enum (re-exported from main crate for UI use)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Menu,
    Playing,
    ConsoleOpen,
    Paused,
}

/// Lightweight progress state used by the adapter to render an overlay.
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub title: String,
    pub percent: f32, // 0.0 ..= 1.0
    pub cancellable: bool,
    pub canceled: bool,
}

// EguiAdapter needs to be Send + Sync for use with Arc<Mutex<>> across threads
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

    /// Process menu actions and convert to UI events
    fn process_menu_action(&mut self, action: MenuAction) {
        use moho_core::events::UiEvent as CoreUiEvent;

        match action {
            MenuAction::LoadScene(path) => {
                self.emit_audio_event(UiAudioEvent::Confirm);
                self.event_bus
                    .publish(CoreUiEvent::LoadSceneRequested { path });
            }
            MenuAction::NewWorld => {
                // Open the New World menu so the user can specify params.
                self.emit_audio_event(UiAudioEvent::Confirm);
                self.show_menu("new_world");
            }
            MenuAction::GenerateWorld(spec) => {
                // User confirmed generation with a WorldSpec payload from the new_world menu
                self.emit_audio_event(UiAudioEvent::Confirm);
                self.event_bus.publish(CoreUiEvent::NewWorldRequested {
                    name: "New World".to_string(),
                    seed: spec.seed,
                    size: spec.size_xz,
                });
            }
            MenuAction::Exit => {
                self.emit_audio_event(UiAudioEvent::ButtonClick);
                self.event_bus.publish(CoreUiEvent::ExitRequested);
            }
            MenuAction::ShowMenu(name) => {
                self.emit_audio_event(UiAudioEvent::MenuNavigate);
                self.event_bus
                    .publish(CoreUiEvent::MenuShown { name: name.clone() });
            }
            MenuAction::Close => {
                self.emit_audio_event(UiAudioEvent::Cancel);
                self.hide_menus();
            }
            MenuAction::SettingsSaved(_prefs) => {
                self.emit_audio_event(UiAudioEvent::Confirm);
                self.event_bus.publish(CoreUiEvent::SettingsSaved);
            }
            MenuAction::None => {
                // No action
            }
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

    /// Emit an audio event
    fn emit_audio_event(&mut self, audio_event: UiAudioEvent) {
        use moho_core::events::AudioEvent;

        let core_event = match audio_event {
            UiAudioEvent::ButtonClick => AudioEvent::ButtonClick,
            UiAudioEvent::MenuNavigate => AudioEvent::MenuNavigate,
            UiAudioEvent::Confirm => AudioEvent::Confirm,
            UiAudioEvent::Cancel => AudioEvent::Cancel,
            UiAudioEvent::Error => AudioEvent::Error,
        };

        self.event_bus.publish(core_event);
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
        // Note: We need GameState to determine what to render.
        // For now, use the visible flag. The main app will need to pass GameState
        // or we can read it from the event bus in the future.
        // TODO: Accept GameState as parameter or store in adapter

        // Skip rendering if UI is not visible
        if !self.ui_state.visible {
            return;
        }

        // Update global visibility flag
        UI_OVERLAY_VISIBLE.store(self.ui_state.visible, Ordering::SeqCst);

        // Initialize renderer if needed
        let format = self
            .surface_config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        self.init_renderer(device, format);

        // Take input from winit integration
        let raw_input = self.take_egui_input();

        // Run egui and collect menu actions
        let mut menu_actions = Vec::new();
        let mut modal_result = crate::modal::ModalResult::None;

        let full_output = self.context.run(raw_input, |ctx| {
            // Render based on current game state
            match self.current_game_state {
                GameState::Menu => {
                    // Render menu screens
                    if let Some(screen) = self.ui_state.active_screen_mut() {
                        let items = screen.render(ctx);

                        // Collect clicked actions for processing outside the closure
                        for item in items {
                            if item.clicked && item.enabled {
                                menu_actions.push(item.action);
                            }
                        }
                    }

                    // Check if any screen wants to show a modal
                    if let Some(screen) = self.ui_state.active_screen_mut()
                        && let Some(modal) = screen.take_pending_modal()
                    {
                        self.ui_state.modal_manager.show(modal);
                    }

                    // Render modal on top of menu (if active)
                    modal_result = self.ui_state.modal_manager.render(ctx);
                }
                GameState::ConsoleOpen => {
                    // Render console overlay (game world is rendered by main renderer)
                    let console_action = self.ui_state.console.render(ctx);

                    // Process console action
                    use crate::overlays::ConsoleAction;
                    match console_action {
                        ConsoleAction::Close => {
                            use moho_core::events::UiEvent;
                            self.event_bus.publish(UiEvent::MenuHidden {
                                name: "console".to_string(),
                            });
                        }
                        ConsoleAction::Quit => {
                            use moho_core::events::UiEvent;
                            self.event_bus.publish(UiEvent::ExitRequested);
                        }
                        ConsoleAction::ToggleGodMode => {
                            use moho_core::events::DebugEvent;
                            self.event_bus
                                .publish(DebugEvent::ToggleGodMode { enabled: true });
                        }
                        ConsoleAction::ToggleNoclip => {
                            use moho_core::events::DebugEvent;
                            self.event_bus
                                .publish(DebugEvent::ToggleCollision { enabled: false });
                        }
                        ConsoleAction::SetSunDirection(yaw, pitch) => {
                            use moho_core::events::GraphicsEvent;
                            // Convert degrees to radians
                            let yaw_rad = yaw.to_radians();
                            let pitch_rad = pitch.to_radians();
                            self.event_bus.publish(GraphicsEvent::SunDirectionChanged {
                                yaw: yaw_rad,
                                pitch: pitch_rad,
                            });
                        }
                        ConsoleAction::SetTimeOfDay(time) => {
                            use moho_core::events::GraphicsEvent;
                            // Time is now in hours (0-24) and will be set directly on the game clock
                            // The sun_angle field is kept for backward compatibility but not used
                            self.event_bus
                                .publish(GraphicsEvent::TimeOfDayChanged { time, sun_angle: 0.0 });
                        }
                        ConsoleAction::None => {}
                    }
                }
                GameState::Paused => {
                    // TODO: Render pause menu overlay when implemented
                    // For now, just render a simple "Paused" message
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.heading("Paused");
                            ui.label("Press ESC to resume");
                        });
                    });
                }
                GameState::Playing => {
                    // No UI rendering when playing (game world only)
                    // This path should rarely be hit since ui_state.visible should be false
                }
            } // Progress overlay (renders above menus). Keep it simple: a centered
            // window with a progress bar and optional Cancel button. The cancel
            // flag is stored in the ProgressState so the caller can poll it.
            if let Some(progress) = self.progress.as_mut() {
                use egui::{Align2, RichText};
                egui::Area::new("progress_overlay_area".into())
                    .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new(&progress.title).heading());
                            ui.add_space(6.0);
                            ui.add(egui::ProgressBar::new(progress.percent).show_percentage());
                            ui.add_space(8.0);
                            if progress.cancellable && ui.add(egui::Button::new("Cancel")).clicked()
                            {
                                progress.canceled = true;
                            }
                        });
                    });
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

        // Process menu actions outside the egui context
        for action in menu_actions {
            self.process_menu_action(action);
        }

        // Handle platform output
        self.handle_platform_output(full_output.platform_output);

        // Render to screen
        if let Some(renderer) = &mut self.renderer {
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [surface_width, surface_height],
                pixels_per_point: self.context.pixels_per_point(),
            };

            let clipped_primitives = self
                .context
                .tessellate(full_output.shapes, full_output.pixels_per_point);

            // Update textures
            for (id, image_delta) in &full_output.textures_delta.set {
                renderer.update_texture(device, queue, *id, image_delta);
            }

            // Update GPU buffers
            renderer.update_buffers(
                device,
                queue,
                encoder,
                &clipped_primitives,
                &screen_descriptor,
            );

            // Create render pass and render
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Use unsafe transmute to satisfy egui_wgpu lifetime requirements
                let render_pass_static: &mut wgpu::RenderPass<'static> =
                    unsafe { std::mem::transmute(&mut render_pass) };
                renderer.render(render_pass_static, &clipped_primitives, &screen_descriptor);
            }

            // Free textures
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
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
