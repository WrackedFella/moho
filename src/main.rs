//! Moho — a voxel game engine and application binary.
//!
//! This is the main executable that wires together the engine crates
//! ([`moho_core`], [`moho_renderer`], [`moho_audio`], [`moho_ui`]) into a
//! runnable application via [`winit`]'s event loop.

use moho_core::prefs::Prefs;
use moho_game::scene::SceneEntities;
use moho_input::action_map::{self, ActionFrame};
use moho_ui::actions::StrategyAction;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, StartCause, WindowEvent};
mod input_dispatcher;
use crate::input_dispatcher::InputDispatcher;
mod input_event;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

// Core game state module
mod game_state;

// Application initialization modules
mod app;

mod save;

enum GenerationMsg {
    Progress(f32),
    Completed {
        scene_bytes: Vec<u8>,
        spec: moho_game::scene_builders::WorldSpec,
        terrain_config: moho_game::scene_builders::TerrainConfig,
        // Empty grid — streaming populates it on demand. Boxed to reduce enum
        // variant size (VoxelGrid is large; the other variants are cheap).
        grid: Box<moho_core::voxel::VoxelGrid>,
    },
    Canceled,
    Failed(String),
}

/// Conservatively forward a wheel delta to the game's input channel.
/// Returns true if the event was forwarded.
pub(crate) fn forward_wheel_if_allowed(
    ui_adapter: &std::sync::Arc<std::sync::Mutex<moho_ui::EguiAdapter>>,
    tx: &std::sync::mpsc::Sender<crate::input_event::InputEvent>,
    delta_y: f32,
) -> bool {
    // Conservative: if we can't acquire the lock, do not forward.
    match ui_adapter.try_lock() {
        Ok(a) if a.is_visible() => return false,
        Err(_) => return false,
        _ => {}
    }

    tx.send(crate::input_event::InputEvent::MouseWheel { delta_y })
        .is_ok()
}

// Combined state to handle lifetimes properly
struct WindowRenderer {
    window: Arc<Window>,
    renderer: Box<dyn moho_renderer::RendererBackend>,
    mesh_handle: u32,
    cube_mesh_handle: u32,
}

// Application state structure that implements ApplicationHandler
struct App {
    entities: SceneEntities,
    scene: moho_renderer::Scene,
    terrain_material_idx: u32,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),

    // Light propagation system (owns the voxel grid internally)
    light_system: Option<moho_core::voxel::LightSystem>,

    // Game state management
    game_state: crate::game_state::GameState,

    // Runtime state (initialized after window creation)
    window_renderer: Option<WindowRenderer>,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Event collection channels (for events that need to mutate App state)
    ui_event_rx: std::sync::mpsc::Receiver<moho_core::events::UiEvent>,
    audio_event_rx: std::sync::mpsc::Receiver<moho_core::events::AudioEvent>,
    graphics_event_rx: std::sync::mpsc::Receiver<moho_core::events::GraphicsEvent>,
    world_event_rx: std::sync::mpsc::Receiver<moho_core::events::WorldEvent>,
    debug_event_rx: std::sync::mpsc::Receiver<moho_core::events::DebugEvent>,

    // Audio system (not thread-safe, stays on main thread)
    audio_system: Option<moho_audio::AudioSystem>,

    // UI adapter
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,

    // Debug state
    debug_mode: u32,

    // Input dispatcher (routes events to prioritized subscribers)
    dispatcher: InputDispatcher,

    // Camera control
    simulation: moho_game::simulation::SimulationController,

    // Keybinds and preferences
    prefs: Prefs,

    // Frame timing
    frame_duration: Duration,
    last_frame: Instant,

    // Grouped sub-systems
    physics: app::physics_controller::PhysicsController,
    generation: app::generation_job::WorldGenerationJob,
    input: app::input_state::InputState,
    chunk_streamer: Option<app::chunk_streamer::ChunkStreamer>,
    /// Cached player chunk position used to gate per-frame LOD transition scans.
    lod_player_chunk_cache: glam::IVec3,

    /// The player's gameplay pawn (inventory, mining). Deliberately separate
    /// from `simulation.player_controller`, which is camera/movement-only —
    /// see `moho_game::pawn::Pawn`'s doc comment.
    pawn: moho_game::pawn::Pawn,
}

impl App {
    /// Saves prefs together with the current bindings.
    fn save_prefs(&mut self) -> std::io::Result<()> {
        self.prefs
            .set_bindings(self.input.actions.bindings().to_section());
        self.prefs.save()
    }

    fn from_config(config: crate::app::config::AppConfig) -> Self {
        let initialized = crate::app::initializer::AppInitializer::new(config)
            .build()
            .expect("Failed to initialize application");

        // Create a minimal voxel grid and light system for testing frame loop integration
        // TODO: Replace with actual terrain grid when scene generation is integrated
        let voxel_grid = moho_core::voxel::VoxelGrid::new(16);
        let light_system = moho_core::voxel::LightSystem::with_default_budget(
            voxel_grid,
            initialized.event_bus.clone(),
        );
        tracing::info!("Created LightSystem for frame loop integration");

        Self {
            entities: initialized.entities,
            scene: initialized.scene,
            terrain_material_idx: initialized.terrain_material_idx,
            camera: initialized.camera,

            light_system: Some(light_system),

            game_state: crate::game_state::GameState::Menu, // Start in menu
            window_renderer: None,
            event_bus: initialized.event_bus,

            ui_event_rx: initialized.ui_event_rx,
            audio_event_rx: initialized.audio_event_rx,
            graphics_event_rx: initialized.graphics_event_rx,
            world_event_rx: initialized.world_event_rx,
            debug_event_rx: initialized.debug_event_rx,

            audio_system: initialized.audio_system,

            ui_adapter: None,

            debug_mode: 0,

            dispatcher: InputDispatcher::new(),

            simulation: initialized.simulation,

            prefs: initialized.prefs,

            frame_duration: initialized.frame_duration,
            last_frame: initialized.last_frame,

            physics: app::physics_controller::PhysicsController::new(),
            generation: app::generation_job::WorldGenerationJob::new(),
            input: app::input_state::InputState::new(initialized.actions),
            chunk_streamer: None,
            lod_player_chunk_cache: glam::IVec3::splat(i32::MIN),

            pawn: moho_game::pawn::Pawn::default(),
        }
    }

    /// An `App` with no window, renderer, UI or audio, built from default
    /// prefs so tests never read `config/prefs.ini`.
    #[cfg(test)]
    fn headless() -> Self {
        Self::from_config(
            crate::app::config::AppConfig::builder()
                .init_audio(false)
                .build(),
        )
    }

    fn setup_renderer_and_ui(
        &mut self,
        window: Arc<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::app::renderer_setup::setup_renderer_and_ui(self, window)
    }

    fn generate_new_world(
        &mut self,
        spec: moho_game::scene_builders::WorldSpec,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::app::world_generator::generate_new_world(self, spec, std::path::Path::new("saves"))
    }

    fn auto_save_on_shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        crate::app::autosave::auto_save_on_shutdown(self, std::path::Path::new("saves"))
    }

    fn load_scene<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::app::scene_loader::load_scene(self, path.as_ref())
    }

    /// Place the character after a scene is loaded.
    ///
    /// Expects physics to be already reset and the loaded chunks registered
    /// by the loader.
    fn setup_physics_for_loaded_world(&mut self) {
        // Spawn the physics character at the saved player position so the KCC
        // doesn't immediately override the restored camera on the first frame.
        let saved_pos = self.simulation.position();
        let saved_x = saved_pos.x as i32;
        let saved_z = saved_pos.z as i32;
        let terrain_y = self
            .light_system
            .as_ref()
            .and_then(|ls| ls.grid().get_height(saved_x, saved_z))
            .unwrap_or(10) as f32;

        let spawn_pos = glam::Vec3::new(saved_pos.x, terrain_y + 3.0, saved_pos.z);
        if let Some(ref mut pw) = self.physics.world {
            pw.add_character(spawn_pos);
        }

        // Align the simulation Y to match physics spawn (avoids terrain clipping).
        let (yaw, pitch) = self.simulation.yaw_pitch();
        self.simulation
            .set_position_yaw_pitch(spawn_pos, yaw, pitch);

        tracing::info!(
            count = self.entities.chunks.len(),
            "Physics initialized for loaded world"
        );
    }

    fn update_controller_input(&mut self, frame: &ActionFrame<StrategyAction>) {
        let is_active = |action: StrategyAction| frame.held(action);

        // Calculate forward/backward
        let forward = if is_active(StrategyAction::MoveForward) {
            1.0
        } else {
            0.0
        } - if is_active(StrategyAction::MoveBack) {
            1.0
        } else {
            0.0
        };

        // Calculate left/right (A is left, so negative)
        let right = if is_active(StrategyAction::MoveRight) {
            1.0
        } else {
            0.0
        } - if is_active(StrategyAction::MoveLeft) {
            1.0
        } else {
            0.0
        };

        // Calculate up/down - only in first person mode
        let up = if self.simulation.camera_mode() == moho_game::controller::CameraMode::FirstPerson
        {
            (if is_active(StrategyAction::Ascend) {
                1.0
            } else {
                0.0
            }) - (if is_active(StrategyAction::Descend) {
                1.0
            } else {
                0.0
            })
        } else {
            0.0 // No up/down in isometric mode
        };

        // Check if sprint is active (Shift)
        let sprint = is_active(StrategyAction::Sprint);

        self.simulation.controller_input.forward = forward;
        self.simulation.controller_input.right = right;
        self.simulation.controller_input.up = up;
        self.simulation.controller_input.sprint = sprint;
        // Zoom is handled separately via mouse wheel input
        self.simulation.controller_input.zoom_delta = 0.0;

        // Track jump key for physics KCC
        // A tap shorter than one tick still jumps.
        self.physics.jump_pressed =
            frame.held(StrategyAction::Jump) || frame.pressed(StrategyAction::Jump);
    }

    fn handle_keyboard_input(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;

        if let PhysicalKey::Code(keycode) = event.physical_key {
            match keycode {
                KeyCode::Backquote => {
                    // Backtick (`) toggles console
                    if pressed {
                        use crate::game_state::GameState;
                        match self.game_state {
                            GameState::Playing => self.enter_console(),
                            GameState::ConsoleOpen => self.exit_console(),
                            _ => {}
                        }
                    }
                }
                KeyCode::F3 => {
                    // F3 toggles the debug HUD overlay
                    if pressed
                        && let Some(ui_adapter) = &self.ui_adapter
                        && let Ok(mut adapter) = ui_adapter.lock()
                    {
                        adapter.toggle_debug_hud();
                    }
                }
                KeyCode::F4 => {
                    // F4 toggles the chunk-boundary debug minimap
                    if pressed
                        && let Some(ui_adapter) = &self.ui_adapter
                        && let Ok(mut adapter) = ui_adapter.lock()
                    {
                        adapter.toggle_overlay("chunk_debug");
                    }
                }
                // Escape closes console if open, otherwise opens menu
                KeyCode::Escape if pressed => {
                    use crate::game_state::GameState;
                    match self.game_state {
                        GameState::ConsoleOpen => self.exit_console(),
                        GameState::Playing => {
                            // ESC to show menu
                            self.show_menu();

                            // Also explicitly request the adapter show the "start" menu
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut adapter) = ui_adapter.lock()
                            {
                                adapter.show_menu("start");
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_device_input(&mut self, event: &DeviceEvent) {
        // Only process input in game mode and first person camera mode
        if self.game_state != crate::game_state::GameState::Playing
            || self.simulation.camera_mode() != moho_game::controller::CameraMode::FirstPerson
        {
            return;
        }
        action_map::handle_device_event(&mut self.input.actions, event);
    }

    /// Grab and hide the cursor for game mode
    fn grab_cursor(&mut self) {
        if let Some(ref wr) = self.window_renderer {
            wr.window.set_cursor_visible(false);

            // Try to grab the cursor - confined mode keeps it in window
            let _ = wr
                .window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| wr.window.set_cursor_grab(CursorGrabMode::Locked));
        }
    }

    /// Release the cursor for menu mode
    fn release_cursor(&mut self) {
        if let Some(ref wr) = self.window_renderer {
            wr.window.set_cursor_visible(true);
            let _ = wr.window.set_cursor_grab(CursorGrabMode::None);
        }
    }

    fn hide_menu(&mut self) {
        use moho_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::hide_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot hide menu"),
        }
    }

    fn show_menu(&mut self) {
        use moho_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::show_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot show menu"),
        }
    }

    /// Enter console mode (opens debug console over game)
    fn enter_console(&mut self) {
        use moho_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::enter_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot enter console"),
        }
    }

    /// Exit console mode (return to playing)
    fn exit_console(&mut self) {
        use moho_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::exit_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot exit console"),
        }
    }

    /// Toggle pause state
    #[allow(dead_code)]
    fn toggle_pause(&mut self) {
        use moho_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::toggle_pause(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::debug!(error = %e, "Cannot toggle pause"),
        }
    }

    /// Apply a state transition with all its side effects.
    ///
    /// This method centralizes all the boilerplate for state transitions:
    /// - Update game_state
    /// - Update UI visibility and state
    /// - Handle cursor grab/release
    /// - Show specific menu if requested
    fn apply_transition(&mut self, actions: moho_ui::StateTransitionActions) {
        tracing::info!(
            from = ?self.game_state,
            to = ?actions.new_state,
            "State transition"
        );

        // Update core state
        self.game_state = actions.new_state;

        // Update UI visibility and state
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            use moho_ui::UI_OVERLAY_VISIBLE;
            adapter.set_visible(actions.ui_visible);

            adapter.set_game_state(actions.new_state);

            // Update atomic flag for UI visibility
            UI_OVERLAY_VISIBLE.store(actions.ui_visible, std::sync::atomic::Ordering::SeqCst);

            // Show specific menu if requested
            if let Some(menu_name) = actions.show_menu {
                adapter.show_menu(menu_name);
            }

            tracing::debug!(
                visible = actions.ui_visible,
                state = ?actions.new_state,
                "UI updated"
            );
        }

        // Handle cursor state
        if actions.cursor_grabbed {
            self.grab_cursor();
        } else {
            self.release_cursor();
        }
    }

    /// Handle audio events from the UI or game
    fn handle_audio_event(&mut self, event: moho_audio::AudioEvent) {
        if let Some(ref mut audio) = self.audio_system
            && let Err(e) = audio.handle_event(event)
        {
            // Don't spam errors for missing audio files during development
            tracing::debug!(error = %e, "Audio event failed");
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_manager = app::event_loop::WindowManager::new();
        if let Err(e) = window_manager.handle_resumed(self, event_loop) {
            tracing::error!(error = %e, "Window resume failed");
            event_loop.exit();
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        let frame_processor = app::event_loop::FrameProcessor::new();
        let event_processor = app::event_loop::EventProcessor::new();
        let generation_processor = app::event_loop::GenerationProcessor::new();

        // Check if it's time to process a frame
        if !frame_processor.should_process_frame(self) {
            frame_processor.update_control_flow(self, event_loop);
            return;
        }

        // Process complete frame (timing, game state, lighting)
        frame_processor.process_frame(self, event_loop);

        // Process all event types from event bus
        event_processor.process_ui_events(self, event_loop);
        event_processor.process_audio_events(self);
        event_processor.process_graphics_events(self);
        event_processor.process_world_events(self);
        event_processor.process_input_events(self);
        event_processor.process_debug_events(self);

        // Check for generation cancellation from UI
        event_processor.check_generation_cancel(self);

        // Poll async generation if running
        generation_processor.poll_generation(self);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Handle Tab key BEFORE dispatcher to prevent UI from consuming it
        if let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Tab),
                    state: ElementState::Pressed,
                    ..
                },
            ..
        } = &event
            && self.game_state == crate::game_state::GameState::Playing
        {
            match self.simulation.camera_mode() {
                moho_game::controller::CameraMode::FirstPerson => {
                    // Switch to RTS first so look_at runs in Isometric mode,
                    // setting rts_look_target without touching FPS yaw/pitch.
                    self.simulation
                        .set_camera_mode(moho_game::controller::CameraMode::Isometric);
                    self.simulation.look_at(self.simulation.position());
                }
                moho_game::controller::CameraMode::Isometric => {
                    self.simulation
                        .set_camera_mode(moho_game::controller::CameraMode::FirstPerson);
                }
            }
            tracing::info!(
                mode = ?self.simulation.camera_mode(),
                "Switched to camera mode"
            );
            return; // Don't dispatch Tab further
        }

        // Focus loss must release held actions whatever the UI or game state does.
        if matches!(event, WindowEvent::Focused(false)) {
            action_map::handle_window_event(&mut self.input.actions, &event);
        }

        // Dispatch the event to registered subscribers (UI first). If consumed,
        // skip further application-level handling.
        if self.dispatcher.dispatch(&event) {
            return;
        }

        // Hotkeys run first; an event that leaves or enters play does not reach the action map.
        let was_playing = self.game_state == crate::game_state::GameState::Playing;
        let window_event_handler = app::event_loop::WindowEventHandler::new();
        window_event_handler.handle_window_event(self, event_loop, &event);
        if was_playing && self.game_state == crate::game_state::GameState::Playing {
            action_map::handle_window_event(&mut self.input.actions, &event);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let window_event_handler = app::event_loop::WindowEventHandler::new();
        window_event_handler.handle_device_event(self, &event);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        tracing::info!("Shutting down application...");
        // WorldGenerationJob::drop handles cancel + join automatically.
    }
}

/// Installs the global subscriber, which also forwards `log` records from dependencies.
fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
}

fn main() {
    init_logging();
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::from_config(crate::app::config::AppConfig::from_prefs());

    // Run the modern event loop with ApplicationHandler
    let _ = event_loop.run_app(&mut app);
}
