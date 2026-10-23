//! Flagstone — the strategy game binary.
//!
//! This is the main executable that wires together the engine crates
//! ([`moho_core`], [`moho_renderer`], [`moho_audio`], [`flagstone_ui`]) into a
//! runnable application: [`App`] implements [`moho_app::Game`] and `main` hands
//! it to [`moho_app::run`].

use flagstone_game::scene::SceneEntities;
use moho_core::prefs::Prefs;
use moho_input::action_map;
use std::sync::Arc;
use std::sync::Mutex;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
mod input_dispatcher;
use crate::input_dispatcher::InputDispatcher;
mod input_event;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

// Core game state module
mod game_state;

// Application initialization modules
mod app;

mod save;

enum GenerationMsg {
    Progress(f32),
    Completed {
        scene_bytes: Vec<u8>,
        spec: flagstone_game::scene_builders::WorldSpec,
        terrain_config: flagstone_game::scene_builders::TerrainConfig,
        // Empty grid — streaming populates it on demand. Boxed to reduce enum
        // variant size (VoxelGrid is large; the other variants are cheap).
        grid: Box<flagstone_voxel::VoxelGrid>,
    },
    Canceled,
    Failed(String),
}

/// Conservatively forward a wheel delta to the game's input channel.
/// Returns true if the event was forwarded.
pub(crate) fn forward_wheel_if_allowed(
    ui_adapter: &std::sync::Arc<std::sync::Mutex<flagstone_ui::EguiAdapter>>,
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

/// A renderer change queued during a tick and applied by the next frame, the only
/// place the renderer is reachable.
enum RenderRequest {
    AddPointLight {
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
    },
    ShadowQuality(u8),
    SsaoQuality(u8),
    ViewHeight(Option<f32>),
    Vsync(bool),
}

struct App {
    entities: SceneEntities,
    scene: moho_renderer::Scene,
    terrain_material_idx: u32,
    terrain_palette: app::world_geometry::TerrainPalette,
    /// Material table index of the cut-face caps' colour.
    cut_face_material_idx: u32,
    cut_caps: app::cut_caps::CutCaps,
    view_level: app::view_level::ViewLevel,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),

    // Light propagation system (owns the voxel grid internally)
    light_system: Option<flagstone_voxel::LightSystem>,

    // Game state management
    game_state: crate::game_state::GameState,

    // Runtime state (initialized after window creation)
    window: Option<Arc<Window>>,
    mesh_handle: u32,
    cube_mesh_handle: u32,

    // Renderer changes queued by the tick, applied by `frame`
    pending_render: Vec<RenderRequest>,

    /// Frame cap `frame` hands to the loop once `frame_cap_changed`; `None` is no cap.
    pending_frame_cap: Option<std::num::NonZeroU32>,
    frame_cap_changed: bool,

    saves_dir: std::path::PathBuf,

    // Set when the UI asks to quit; `frame` autosaves and leaves the loop
    exit_requested: bool,

    // Set when the UI asks for the main menu; `frame` autosaves, then shows it
    main_menu_requested: bool,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Event collection channels (for events that need to mutate App state)
    ui_event_rx: std::sync::mpsc::Receiver<moho_core::events::UiEvent>,
    audio_event_rx: std::sync::mpsc::Receiver<moho_core::events::AudioEvent>,
    graphics_event_rx: std::sync::mpsc::Receiver<moho_core::events::GraphicsEvent>,
    world_event_rx: std::sync::mpsc::Receiver<flagstone_voxel::WorldEvent>,
    debug_event_rx: std::sync::mpsc::Receiver<moho_core::events::DebugEvent>,

    // UI adapter
    ui_adapter: Option<Arc<Mutex<flagstone_ui::EguiAdapter>>>,

    // Debug state
    debug_mode: u32,

    // Input dispatcher (routes events to prioritized subscribers)
    dispatcher: InputDispatcher,

    // Camera control
    simulation: flagstone_game::simulation::SimulationController,

    // Keybinds and preferences
    prefs: Prefs,
    /// Where the app saves prefs. `None` for a headless `App`, so its console prefs
    /// commands never write the user's config file; a Settings screen opened there
    /// still saves to `Prefs::config_path()`, so tests that save from it set a path.
    prefs_path: Option<std::path::PathBuf>,

    pending_audio_settings: Option<moho_audio::AudioSettings>,

    // Grouped sub-systems
    physics: app::physics_controller::PhysicsController,
    generation: app::generation_job::WorldGenerationJob,
    input: app::input_state::InputState,
    chunk_streamer: Option<app::chunk_streamer::ChunkStreamer>,
    /// Cached player chunk position used to gate per-frame LOD transition scans.
    lod_player_chunk_cache: glam::IVec3,

    /// The player's gameplay pawn (inventory, mining). Deliberately separate
    /// from `simulation.player_controller`, which is camera/movement-only —
    /// see `flagstone_game::pawn::Pawn`'s doc comment.
    pawn: flagstone_game::pawn::Pawn,

    /// The window's physical size, which pointer positions are measured in.
    viewport: glam::UVec2,
    /// The surface under the last strategy-view click, `None` when it hit nothing.
    pointer_hit: Option<flagstone_game::raycast::SurfaceHit>,
}

impl App {
    /// Writes the running prefs to `prefs_path`; does nothing without one. A failed
    /// write is logged: the running prefs stay in effect either way.
    fn save_prefs(&self) {
        if let Some(path) = &self.prefs_path
            && let Err(e) = self.prefs.save_to(path)
        {
            tracing::warn!(error = %e, path = %path.display(), "Failed to save prefs");
        }
    }

    /// Makes `prefs` (just saved by the Settings screen) the running prefs and
    /// rebinds input from them and queues their audio levels for the next frame.
    /// Does not save: the screen already wrote the file.
    fn adopt_settings(&mut self, prefs: Prefs) {
        let config = crate::app::config::AppConfig::from_prefs_struct(prefs);
        self.input.actions = crate::app::initializer::build_actions(&config);
        self.pending_audio_settings = Some(moho_audio::AudioSettings::from_prefs(&config.prefs));
        if config.prefs.vsync() != self.prefs.vsync() {
            self.pending_render
                .push(RenderRequest::Vsync(config.prefs.vsync()));
        }
        self.set_pending_frame_cap(config.prefs.frame_cap());
        self.prefs = config.prefs;
    }

    fn set_pending_frame_cap(&mut self, cap: Option<std::num::NonZeroU32>) {
        self.pending_frame_cap = cap;
        self.frame_cap_changed = true;
    }

    fn from_config(config: crate::app::config::AppConfig) -> Self {
        let initialized = crate::app::initializer::AppInitializer::new(config)
            .build()
            .expect("Failed to initialize application");

        // Create a minimal voxel grid and light system for testing frame loop integration
        // TODO: Replace with actual terrain grid when scene generation is integrated
        let voxel_grid = flagstone_voxel::VoxelGrid::new(16);
        let mut scene = initialized.scene;
        let terrain_palette = app::world_geometry::TerrainPalette::new(
            &voxel_grid.material_registry,
            &mut scene.material_table,
            initialized.terrain_material_idx,
        );
        let light_system = flagstone_voxel::LightSystem::with_default_budget(
            voxel_grid,
            initialized.event_bus.clone(),
        );
        tracing::info!("Created LightSystem for frame loop integration");

        Self {
            entities: initialized.entities,
            scene,
            terrain_material_idx: initialized.terrain_material_idx,
            terrain_palette,
            cut_face_material_idx: initialized.cut_face_material_idx,
            cut_caps: app::cut_caps::CutCaps::default(),
            view_level: app::view_level::ViewLevel::default(),
            camera: initialized.camera,

            light_system: Some(light_system),

            game_state: crate::game_state::GameState::Menu, // Start in menu
            window: None,
            mesh_handle: 0,
            cube_mesh_handle: 0,
            pending_render: Vec::new(),
            pending_frame_cap: None,
            frame_cap_changed: false,
            saves_dir: std::path::PathBuf::from("saves"),
            exit_requested: false,
            main_menu_requested: false,
            event_bus: initialized.event_bus,

            ui_event_rx: initialized.ui_event_rx,
            audio_event_rx: initialized.audio_event_rx,
            graphics_event_rx: initialized.graphics_event_rx,
            world_event_rx: initialized.world_event_rx,
            debug_event_rx: initialized.debug_event_rx,

            ui_adapter: None,

            debug_mode: 0,

            dispatcher: InputDispatcher::new(),

            simulation: initialized.simulation,

            pending_audio_settings: Some(moho_audio::AudioSettings::from_prefs(&initialized.prefs)),
            prefs: initialized.prefs,
            prefs_path: Some(Prefs::config_path()),

            physics: app::physics_controller::PhysicsController::new(),
            generation: app::generation_job::WorldGenerationJob::new(),
            input: app::input_state::InputState::new(initialized.actions),
            chunk_streamer: None,
            lod_player_chunk_cache: glam::IVec3::splat(i32::MIN),

            pawn: flagstone_game::pawn::Pawn::default(),

            viewport: glam::UVec2::ZERO,
            pointer_hit: None,
        }
    }

    /// An `App` with no window, renderer, UI or audio, built from default
    /// prefs so tests never read `config/prefs.ini`.
    #[cfg(test)]
    fn headless() -> Self {
        let mut app = Self::from_config(crate::app::config::AppConfig::builder().build());
        app.prefs_path = None;
        app
    }

    fn generate_new_world(
        &mut self,
        spec: flagstone_game::scene_builders::WorldSpec,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let saves_dir = self.saves_dir.clone();
        crate::app::world_generator::generate_new_world(self, spec, &saves_dir)
    }

    fn auto_save_on_shutdown(
        &mut self,
        clock: &moho_app::GameClock,
        lights: &[moho_render_api::LightDesc],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let saves_dir = self.saves_dir.clone();
        crate::app::autosave::auto_save_on_shutdown(self, clock, &saves_dir, lights)
    }

    fn load_scene<P: AsRef<std::path::Path>>(
        &mut self,
        clock: &mut moho_app::GameClock,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        crate::app::scene_loader::load_scene(self, clock, path.as_ref())
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
        self.physics.spawn_player(spawn_pos);

        // Align the simulation Y to match physics spawn (avoids terrain clipping).
        let (yaw, pitch) = self.simulation.yaw_pitch();
        self.physics.set_player_look(yaw, pitch);
        self.simulation
            .set_position_yaw_pitch(spawn_pos, yaw, pitch);

        tracing::info!(
            count = self.entities.chunks.len(),
            "Physics initialized for loaded world"
        );
    }

    /// Handle keyboard input for camera controls
    fn handle_keyboard_input(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;

        if let PhysicalKey::Code(keycode) = event.physical_key {
            match keycode {
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
                // A held Escape repeats; acting on repeats would flicker between pause and play.
                KeyCode::Escape if pressed && !event.repeat => self.escape_pressed(),
                _ => {}
            }
        }
    }

    /// Escape: close the console, pause play, or resume from the pause window.
    /// On Settings (reached from the pause window) it does nothing.
    fn escape_pressed(&mut self) {
        use crate::game_state::GameState;
        match self.game_state {
            GameState::ConsoleOpen => self.exit_console(),
            GameState::Playing => self.toggle_pause(),
            GameState::Paused if self.home_screen_showing() => self.toggle_pause(),
            _ => {}
        }
    }

    /// Whether Escape pauses or resumes here, so it must reach `escape_pressed` before
    /// the UI: egui swallows it once a pause-window button has keyboard focus (Tab).
    fn escape_skips_the_ui(&self) -> bool {
        use crate::game_state::GameState;
        match self.game_state {
            GameState::Playing => true,
            GameState::Paused => self.home_screen_showing(),
            GameState::Menu | GameState::ConsoleOpen => false,
        }
    }

    /// Runs a pausing or resuming Escape press ahead of the UI; true when it did.
    fn take_escape_before_ui(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                    state: ElementState::Pressed,
                    repeat: false,
                    ..
                },
            ..
        } = event
        else {
            return false;
        };
        if !self.escape_skips_the_ui() {
            return false;
        }
        self.escape_pressed();
        true
    }

    /// Whether the current state's home screen (the pause window while paused) is
    /// the active screen; true when no UI is attached.
    fn home_screen_showing(&self) -> bool {
        let home = self.game_state.home_screen();
        self.ui_adapter.as_ref().is_none_or(|adapter| {
            adapter
                .lock()
                .is_ok_and(|adapter| adapter.active_screen_name() == home)
        })
    }

    fn resume_game(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::resume_game(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::debug!(error = %e, "Cannot resume game"),
        }
    }

    fn handle_device_input(&mut self, event: &DeviceEvent) {
        if self.game_state != crate::game_state::GameState::Playing
            || !self.input.focused
            || self.simulation.camera_mode() != flagstone_game::controller::CameraMode::FirstPerson
        {
            return;
        }
        action_map::handle_device_event(&mut self.input.actions, event);
    }

    /// Grab and hide the cursor for game mode
    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window {
            window.set_cursor_visible(false);

            // Try to grab the cursor - confined mode keeps it in window
            let _ = window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
        }
    }

    /// Release the cursor for menu mode
    fn release_cursor(&mut self) {
        if let Some(window) = &self.window {
            window.set_cursor_visible(true);
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
    }

    fn hide_menu(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::hide_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot hide menu"),
        }
    }

    fn show_menu(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::show_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot show menu"),
        }
    }

    /// Enter console mode (opens debug console over game)
    fn enter_console(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::enter_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot enter console"),
        }
    }

    /// Exit console mode (return to playing)
    fn exit_console(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::exit_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::warn!(error = %e, "Cannot exit console"),
        }
    }

    /// Toggle pause state
    fn toggle_pause(&mut self) {
        use flagstone_ui::StateTransitionCoordinator;

        match StateTransitionCoordinator::toggle_pause(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => tracing::debug!(error = %e, "Cannot toggle pause"),
        }
    }

    /// Apply a state transition with all its side effects.
    ///
    /// This method centralizes all the boilerplate for state transitions:
    /// - Update game_state
    /// - Release every held input action
    /// - Update UI visibility and state
    /// - Show specific menu if requested
    fn apply_transition(&mut self, actions: flagstone_ui::StateTransitionActions) {
        tracing::info!(
            from = ?self.game_state,
            to = ?actions.new_state,
            "State transition"
        );

        // Update core state
        self.game_state = actions.new_state;

        // Input stops reaching the action map outside play; held actions would stick.
        self.input.actions.release_all();

        // Update UI visibility and state
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            use flagstone_ui::UI_OVERLAY_VISIBLE;
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

        self.apply_cursor();
    }

    fn apply_cursor(&mut self) {
        if flagstone_ui::app_state::game_state::cursor_grabbed(
            self.game_state,
            self.simulation.camera_mode(),
        ) {
            self.grab_cursor();
        } else {
            self.release_cursor();
        }
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

/// Window, loop rate and audio settings for the strategy game.
fn app_config() -> moho_app::AppConfig {
    moho_app::AppConfig {
        window_title: "Flagstone".into(),
        loop_config: moho_app::LoopConfig::new(flagstone_game::TICK_HZ),
        init_audio: true,
        toggle_keys: None,
    }
}

fn main() {
    init_logging();
    let mut app = App::from_config(crate::app::config::AppConfig::from_prefs());
    app.input.gamepads = moho_input::gamepad::Gamepads::new();

    if let Err(e) = moho_app::run(app, app_config()) {
        tracing::error!(error = %e, "Application failed");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use moho_audio::AudioCategory;

    fn prefs_with_sfx(volume: f32) -> Prefs {
        let mut prefs = Prefs::default();
        *prefs.sound_effect_volume_mut() = volume;
        prefs
    }

    fn pending_sfx(app: &App) -> Option<f32> {
        app.pending_audio_settings
            .as_ref()
            .map(|s| s.effective_volume(&AudioCategory::SoundEffect))
    }

    #[test]
    fn adopt_settings_queues_audio_settings_from_prefs() {
        let mut app = App::headless();
        app.adopt_settings(prefs_with_sfx(10.0));
        app.pending_audio_settings = None;

        app.adopt_settings(prefs_with_sfx(3.0));

        let volume = pending_sfx(&app).expect("adopting settings queues audio settings");
        assert!((volume - 0.3).abs() < 1e-6, "volume={volume}");
    }

    #[test]
    fn frame_without_a_save_leaves_audio_settings_unqueued() {
        use moho_app::{FrameContext, Game, GameClock, LoopConfig};
        let mut app = App::headless();
        app.pending_audio_settings = None;
        let clock = GameClock::default();
        let tick = LoopConfig::new(flagstone_game::TICK_HZ).tick_length();
        let mut ctx = FrameContext::new(tick, &clock, None, None);

        Game::frame(&mut app, &mut ctx, 0.0);

        assert!(app.pending_audio_settings.is_none());
    }

    #[test]
    fn app_config_ticks_at_60hz() {
        let config = app_config();

        assert_eq!(config.loop_config.tick_hz, flagstone_game::TICK_HZ);
        assert_eq!(
            config.loop_config.tick_length(),
            std::time::Duration::from_secs(1) / 60
        );
    }

    #[test]
    fn app_config_window_is_titled_flagstone() {
        assert_eq!(app_config().window_title, "Flagstone");
    }
}
