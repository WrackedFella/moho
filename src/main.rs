use crossbeam_channel::{Receiver, unbounded};
use legion::World;
use moho_ui::prefs::Prefs;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, StartCause, WindowEvent};
mod input_dispatcher;
use crate::input_dispatcher::InputDispatcher;
mod input_event;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

// Core game state and input routing modules
mod game_state;
mod input_routing;

// Application initialization modules
mod app;

mod save;

enum GenerationMsg {
    Progress(f32),
    Completed {
        scene_bytes: Vec<u8>,
        spec: moho_core::scene_builders::WorldSpec,
    },
    Canceled,
    Failed(String),
}

/// Conservatively forward a wheel delta to the game's input channel.
/// Returns true if the event was forwarded.
pub(crate) fn forward_wheel_if_allowed(
    ui_adapter: &std::sync::Arc<std::sync::Mutex<moho_ui::EguiAdapter>>,
    tx: &crossbeam_channel::Sender<crate::input_event::InputEvent>,
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
    world: World,
    scene: moho_renderer::Scene,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),

    // Game state management (replaces old AppMode)
    game_state: crate::game_state::GameState,
    input_router: crate::input_routing::InputRouter,

    // Runtime state (initialized after window creation)
    window_renderer: Option<WindowRenderer>,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Event collection channels (for events that need to mutate App state)
    ui_event_rx: crossbeam_channel::Receiver<moho_core::events::UiEvent>,
    audio_event_rx: crossbeam_channel::Receiver<moho_core::events::AudioEvent>,
    graphics_event_rx: crossbeam_channel::Receiver<moho_core::events::GraphicsEvent>,

    // Audio system (not thread-safe, stays on main thread)
    audio_system: Option<moho_audio::AudioSystem>,

    // UI components
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,
    last_world_spec: Option<moho_core::scene_builders::WorldSpec>,
    // Async generation plumbing (only used when UI is present)
    generation_receiver: Option<Receiver<GenerationMsg>>,
    generation_handle: Option<std::thread::JoinHandle<()>>,
    generation_cancel: Option<Arc<AtomicBool>>,

    // Input dispatcher (routes events to prioritized subscribers)
    dispatcher: InputDispatcher,

    // Channel for simplified input events forwarded to the game when UI doesn't consume them
    unconsumed_input_tx: Option<crossbeam_channel::Sender<crate::input_event::InputEvent>>,
    unconsumed_input_rx: Option<crossbeam_channel::Receiver<crate::input_event::InputEvent>>,

    // Camera control (moved into simulation)
    simulation: moho_sim::SimulationController,
    #[allow(dead_code)]
    mouse_sensitivity: f32,
    input_system: moho_core::input::InputSystem,

    // Keybinds
    prefs: Prefs,

    // Keyboard state tracking (stores active key codes with modifiers)
    active_keys: std::collections::HashSet<(u32, u8)>,

    // Frame timing
    frame_duration: Duration,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        // Load application configuration
        let config = crate::app::config::AppConfig::from_prefs();

        // Initialize all systems using the builder
        let initialized = crate::app::initializer::AppInitializer::new(config)
            .build()
            .expect("Failed to initialize application");

        Self {
            world: initialized.world,
            scene: initialized.scene,
            camera: initialized.camera,
            game_state: crate::game_state::GameState::Menu, // Start in menu
            input_router: crate::input_routing::InputRouter::new(),
            window_renderer: None,
            event_bus: initialized.event_bus,

            ui_event_rx: initialized.ui_event_rx,
            audio_event_rx: initialized.audio_event_rx,
            graphics_event_rx: initialized.graphics_event_rx,

            audio_system: initialized.audio_system,

            ui_adapter: None,
            generation_receiver: None,
            generation_handle: None,
            generation_cancel: None,

            dispatcher: InputDispatcher::new(),

            unconsumed_input_tx: None,
            unconsumed_input_rx: None,

            // Camera control (moved into simulation)
            simulation: initialized.simulation,
            mouse_sensitivity: initialized.mouse_sensitivity,
            input_system: initialized.input_system,

            // Store prefs and keyboard state
            prefs: initialized.prefs,
            last_world_spec: None,
            active_keys: std::collections::HashSet::new(),

            frame_duration: initialized.frame_duration,
            last_frame: initialized.last_frame,
        }
    }

    fn setup_renderer_and_ui(
        &mut self,
        window: Arc<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create renderer - leak the Arc to get a 'static reference
        // This is acceptable for a main application window that lives for the program duration
        let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
        let mut renderer = moho_renderer::create_renderer(Some(window_ref))?;

        // Create sphere mesh data using the proper sphere geometry
        let (vertices, normals, indices) = moho_core::actors::Sphere::unit_sphere_indexed(16, 16);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);

        let (cube_vertices, cube_normals, cube_indices) =
            moho_core::actors::Cube::unit_cube_indexed();
        let cube_mesh_handle =
            renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

        // UI setup
        {
            let adapter = moho_ui::build_adapter(Some(window.clone()), self.event_bus.clone());
            let ui_adapter = Arc::new(Mutex::new(adapter));

            if let Ok(mut a) = ui_adapter.lock() {
                a.set_surface_format(wgpu::TextureFormat::Bgra8UnormSrgb);
            }

            renderer.set_frame_callback_arc(Some(
                ui_adapter.clone() as Arc<Mutex<dyn moho_renderer::FrameCallback>>
            ));

            // Store adapter
            self.ui_adapter = Some(ui_adapter.clone());

            // Register KeybindCapture subscriber at higher priority so it can intercept
            // events while the active screen is actively listening for raw input.
            let kb_adapter = ui_adapter.clone();
            self.dispatcher.register(200, move |event: &WindowEvent| {
                if let Ok(mut a) = kb_adapter.lock() {
                    // Quick check then forward to screen input handler
                    if a.active_screen_captures_input() {
                        return a.try_handle_screen_input(event);
                    }
                }
                false
            });

            // Register UI adapter as a normal UI subscriber
            let ui_adapter_clone = ui_adapter.clone();
            self.dispatcher.register(100, move |event: &WindowEvent| {
                if let Ok(mut a) = ui_adapter_clone.lock() {
                    a.handle_winit_event(event)
                } else {
                    false
                }
            });

            // Create channel for simplified input events (e.g., mouse wheel) that
            // the game will process if the UI doesn't consume them.
            let (tx, rx) = crossbeam_channel::unbounded::<crate::input_event::InputEvent>();
            self.unconsumed_input_tx = Some(tx.clone());
            self.unconsumed_input_rx = Some(rx);

            // Prepare a ui_adapter clone to check visibility before forwarding wheel
            let ui_adapter_for_forward = ui_adapter.clone();

            // Register a low-priority subscriber that forwards mouse wheel events into
            // the channel so the game can act on them later (e.g., scroll-to-zoom).
            // We only forward when the UI overlay is not visible. Note: the
            // dispatcher already ensures this subscriber is only called when higher
            // priority handlers did not consume the event (i.e., egui didn't want it).
            self.dispatcher.register(0, move |event: &WindowEvent| {
                use winit::event::WindowEvent as WEvent;
                if let WEvent::MouseWheel { delta, .. } = event {
                    // Use the conservative try_lock approach: if we cannot acquire the
                    // lock for any reason, treat as UI-visible and do not forward.
                    match ui_adapter_for_forward.try_lock() {
                        Ok(a) if a.is_visible() => return false,
                        Err(_) => return false,
                        _ => {}
                    }

                    // Convert delta to a simple numeric pair and forward via helper
                    let delta_y = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_x, y) => *y,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };

                    if crate::forward_wheel_if_allowed(&ui_adapter_for_forward, &tx, delta_y) {
                        return true;
                    }
                }
                false
            });
        }

        // Store everything together in the WindowRenderer
        self.window_renderer = Some(WindowRenderer {
            window,
            renderer,
            mesh_handle,
            cube_mesh_handle,
        });

        Ok(())
    }

    fn generate_new_world(
        &mut self,
        spec: moho_core::scene_builders::WorldSpec,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Starting async generation for spec={:?}", spec);

        {
            use std::path::PathBuf;

            // Prepare communication channel and cancellation flag
            let (tx, rx) = unbounded::<GenerationMsg>();
            let cancel_flag = Arc::new(AtomicBool::new(false));

            // Start UI progress overlay immediately
            if let Some(ui_adapter) = &self.ui_adapter
                && let Ok(mut a) = ui_adapter.lock()
            {
                a.start_progress(format!("Generating {}", spec.name), true);
                a.set_progress(0.01);
            }

            // Save shared state into local variables for the thread.
            let spec_for_thread = spec;
            let cancel_clone = cancel_flag.clone();
            let sender = tx.clone();

            // Spawn the generation thread
            let handle = std::thread::Builder::new()
                .name("world-generator".to_string())
                .spawn(move || {
                    // Report initial progress
                    let _ = sender.send(GenerationMsg::Progress(0.02));

                    // Local world and scene used for generation
                    let mut local_world = World::default();
                    // Step 1: clear/build
                    if cancel_clone.load(Ordering::Relaxed) {
                        let _ = sender.send(GenerationMsg::Canceled);
                        return;
                    }
                    // Use the supplied WorldSpec seed (if any) when generating
                    // terrain so different seeds produce different worlds.
                    let mut terrain_config = moho_core::scene_builders::TerrainConfig::default();
                    if let Some(s) = spec_for_thread.seed {
                        terrain_config.seed = s as u32; // truncate to u32
                    }
                    // Map UI's size_xz (full width in blocks) into the terrain config.
                    terrain_config.world_size = spec_for_thread.size_xz;
                    moho_core::scene_builders::voxel_terrain_scene_with_config(
                        &mut local_world,
                        &terrain_config,
                    );
                    let _ = sender.send(GenerationMsg::Progress(0.6));

                    if cancel_clone.load(Ordering::Relaxed) {
                        let _ = sender.send(GenerationMsg::Canceled);
                        return;
                    }

                    // Encode scene bytes. Provide a sensible default camera for
                    // newly generated worlds so the app has a starting
                    // viewpoint instead of relying on previous controller state.
                    let local_scene = moho_renderer::Scene::new();
                    // Place camera above world center looking slightly down
                    let camera_height = 24.0f32;
                    let camera_position = glam::Vec3::new(0.0, camera_height, 0.0);
                    // yaw = 0.0 (look along +Z), pitch negative to look downward
                    let camera_yaw = 0.0f32;
                    let camera_pitch = -0.4f32;
                    let scene_bytes = match local_scene.encode_to_bytes(
                        &local_world,
                        Some((camera_position, camera_yaw, camera_pitch)),
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            let _ =
                                sender.send(GenerationMsg::Failed(format!("encode failed: {}", e)));
                            return;
                        }
                    };

                    let _ = sender.send(GenerationMsg::Progress(0.9));

                    // Ensure saves directory exists and persist the envelope
                    let saves_dir = PathBuf::from("saves");
                    if !saves_dir.exists()
                        && let Err(e) = std::fs::create_dir_all(&saves_dir)
                    {
                        let _ = sender.send(GenerationMsg::Failed(format!("mkdir failed: {}", e)));
                        return;
                    }
                    let save_path = saves_dir.join("scene.bin");
                    if let Err(e) =
                        save::write_scene_with_metadata(&save_path, &scene_bytes, &spec_for_thread)
                    {
                        let _ = sender.send(GenerationMsg::Failed(format!("write failed: {}", e)));
                        return;
                    }

                    let _ = sender.send(GenerationMsg::Progress(1.0));
                    let _ = sender.send(GenerationMsg::Completed {
                        scene_bytes,
                        spec: spec_for_thread,
                    });
                })?;

            // Store receiver, handle and cancel flag so the main loop can poll it
            self.generation_receiver = Some(rx);
            self.generation_handle = Some(handle);
            self.generation_cancel = Some(cancel_flag);

            Ok(())
        }
    }

    fn auto_save_on_shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Auto-saving on shutdown...");

        // Ensure saves directory exists
        let saves_dir = std::path::Path::new("saves");
        if !saves_dir.exists() {
            std::fs::create_dir_all(saves_dir)?;
        }

        // Save the current scene with camera data. When the UI is enabled
        // we prefer to write the envelope format (magic + metadata + scene)
        // so Continue/Load operations can read the WorldSpec metadata. Use
        // a minimal "autosave" WorldSpec when no explicit metadata is
        // available.
        let save_path = saves_dir.join("scene.bin");
        let (yaw, pitch) = self.simulation.yaw_pitch();
        let camera_data = Some((self.simulation.position(), yaw, pitch));

        // Encode scene bytes and write envelope. Prefer the last known
        // WorldSpec (e.g. from a loaded or generated scene) so autosaves
        // preserve original metadata; fall back to a minimal spec.
        let scene_bytes = self.scene.encode_to_bytes(&self.world, camera_data)?;
        let spec = self
            .last_world_spec
            .clone()
            .unwrap_or(moho_core::scene_builders::WorldSpec {
                name: "autosave".to_string(),
                seed: None,
                size_xz: 64,
                day_length_seconds: 600.0,
                night_length_seconds: 420.0,
                initial_time_of_day: 6.0,
            });
        save::write_scene_with_metadata(&save_path, &scene_bytes, &spec)?;
        log::info!(
            "Auto-saved scene (envelope) to {:?} (spec={:?})",
            save_path,
            spec.name
        );

        Ok(())
    }

    fn load_scene<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Loading scene from: {:?}", path.as_ref());

        // Check if the file exists
        if !path.as_ref().exists() {
            return Err(format!("Scene file does not exist: {:?}", path.as_ref()).into());
        }

        // Clear the existing world
        self.world.clear();

        // Load the scene from file with metadata support
        {
            let (spec, scene_bytes) = save::read_scene_and_metadata(&path)?;
            // Remember the WorldSpec from the loaded file so autosaves and
            // subsequent writes preserve the original metadata.
            self.last_world_spec = Some(spec.clone());
            log::info!("Loaded WorldSpec from save: {:?}", spec);
            let camera_data = self.scene.load_from_bytes(&scene_bytes, &mut self.world)?;
            log::info!("Scene loaded successfully from {:?}", path.as_ref());
            // Restore camera handled below using camera_data
            if let Some((position, yaw, pitch)) = camera_data {
                self.simulation.set_position_yaw_pitch(position, yaw, pitch);
                // Clear any pending input so the restored camera
                // orientation isn't immediately overridden by
                // accumulated mouse deltas or smoothing state.
                self.input_system.clear_pending_input();
                log::info!(
                    "Restored camera position: {:?}, yaw: {:.2}, pitch: {:.2}",
                    position,
                    yaw,
                    pitch
                );
            } else {
                log::info!("No camera data found in scene file, keeping current position");
            }
        }

        // Request a redraw to show the loaded scene
        if let Some(ref wr) = self.window_renderer {
            wr.window.request_redraw();
        }

        // Switch to game mode and hide menu
        self.game_state = crate::game_state::GameState::Playing;
        self.hide_menu();

        log::info!("Scene loading complete - switched to game mode");
        Ok(())
    }

    /// Update controller input from keyboard state
    fn update_controller_input(&mut self) {
        // Helper to check if a binding is currently active
        let is_active = |binding: &moho_ui::prefs::Binding| -> bool {
            self.active_keys.contains(&(binding.code, binding.mods))
        };

        // Calculate forward/backward
        let forward = if is_active(&self.prefs.key_w) {
            1.0
        } else {
            0.0
        } - if is_active(&self.prefs.key_s) {
            1.0
        } else {
            0.0
        };

        // Calculate left/right (A is left, so negative)
        let right = if is_active(&self.prefs.key_d) {
            1.0
        } else {
            0.0
        } - if is_active(&self.prefs.key_a) {
            1.0
        } else {
            0.0
        };

        // Calculate up/down - only in first person mode
        let up = if self.simulation.camera_mode() == moho_core::controller::CameraMode::FirstPerson
        {
            (if is_active(&self.prefs.key_up) {
                1.0
            } else {
                0.0
            }) - (if is_active(&self.prefs.key_down) {
                1.0
            } else {
                0.0
            })
        } else {
            0.0 // No up/down in isometric mode
        };

        self.simulation.controller_input.forward = forward;
        self.simulation.controller_input.right = right;
        self.simulation.controller_input.up = up;
    }

    /// Handle keyboard input for camera controls
    fn handle_keyboard_input(&mut self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;

        if let PhysicalKey::Code(keycode) = event.physical_key {
            // Handle global hotkeys first (work in any state)
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
                    return; // Don't process further
                }
                KeyCode::Escape => {
                    // Escape closes console if open, otherwise opens menu
                    if pressed {
                        use crate::game_state::GameState;
                        match self.game_state {
                            GameState::ConsoleOpen => {
                                self.exit_console();
                                return;
                            }
                            GameState::Playing => {
                                // ESC to show menu
                                self.show_menu();

                                // Also explicitly request the adapter show the "start" menu
                                if let Some(ui_adapter) = &self.ui_adapter
                                    && let Ok(mut adapter) = ui_adapter.lock()
                                {
                                    adapter.show_menu("start");
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            // Only process game input in Playing mode
            if self.game_state != crate::game_state::GameState::Playing {
                return;
            }

            // Convert physical keycode to our binding code using shared helper
            let pk = event.physical_key;
            let code = moho_input::physical_key_to_binding_code(pk);

            // No longer using modifiers
            let mods = 0u8;

            if code != 0 || mods != 0 {
                let key_binding = (code, mods);
                if pressed {
                    self.active_keys.insert(key_binding);
                } else {
                    self.active_keys.remove(&key_binding);
                }
            }

            // Handle special keys (only in Playing state at this point)
            if keycode == KeyCode::Tab && pressed {
                // Toggle camera mode
                let new_mode = match self.simulation.camera_mode() {
                    moho_core::controller::CameraMode::FirstPerson => {
                        moho_core::controller::CameraMode::Isometric
                    }
                    moho_core::controller::CameraMode::Isometric => {
                        moho_core::controller::CameraMode::FirstPerson
                    }
                };
                self.simulation.set_camera_mode(new_mode);
                log::info!(
                    "Switched to camera mode: {:?}",
                    self.simulation.camera_mode()
                );
            }
        }
    }

    /// Handle mouse motion for camera look
    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        // Only process input in game mode and first person camera mode
        if self.game_state != crate::game_state::GameState::Playing
            || self.simulation.camera_mode() != moho_core::controller::CameraMode::FirstPerson
        {
            return;
        }
        self.input_system.collect_mouse_delta((-delta.0, -delta.1));
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
        use moho_types::StateTransitionCoordinator;

        match StateTransitionCoordinator::hide_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => log::warn!("Cannot hide menu: {}", e),
        }
    }

    fn show_menu(&mut self) {
        use moho_types::StateTransitionCoordinator;

        match StateTransitionCoordinator::show_menu(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => log::warn!("Cannot show menu: {}", e),
        }
    }

    /// Enter console mode (opens debug console over game)
    fn enter_console(&mut self) {
        use moho_types::StateTransitionCoordinator;

        match StateTransitionCoordinator::enter_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => log::warn!("{}", e),
        }
    }

    /// Exit console mode (return to playing)
    fn exit_console(&mut self) {
        use moho_types::StateTransitionCoordinator;

        match StateTransitionCoordinator::exit_console(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => log::warn!("{}", e),
        }
    }

    /// Toggle pause state
    #[allow(dead_code)]
    fn toggle_pause(&mut self) {
        use moho_types::StateTransitionCoordinator;

        match StateTransitionCoordinator::toggle_pause(self.game_state) {
            Ok(actions) => self.apply_transition(actions),
            Err(e) => log::debug!("{}", e),
        }
    }

    /// Apply a state transition with all its side effects.
    ///
    /// This method centralizes all the boilerplate for state transitions:
    /// - Update game_state
    /// - Update input_router
    /// - Update UI visibility and state
    /// - Handle cursor grab/release
    /// - Show specific menu if requested
    fn apply_transition(&mut self, actions: moho_types::StateTransitionActions) {
        log::info!(
            "State transition: {:?} -> {:?}",
            self.game_state,
            actions.new_state
        );

        // Update core state
        self.game_state = actions.new_state;
        self.input_router.update_for_state(actions.new_state);

        // Update UI visibility and state
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            adapter.set_visible(actions.ui_visible);

            // Convert moho_types::GameState to moho_ui::GameState
            let ui_state = match actions.new_state {
                moho_types::GameState::Menu => moho_ui::GameState::Menu,
                moho_types::GameState::Playing => moho_ui::GameState::Playing,
                moho_types::GameState::ConsoleOpen => moho_ui::GameState::ConsoleOpen,
                moho_types::GameState::Paused => moho_ui::GameState::Paused,
            };
            adapter.set_game_state(ui_state);

            // Update atomic flag for UI visibility
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(actions.ui_visible, std::sync::atomic::Ordering::SeqCst);

            // Show specific menu if requested
            if let Some(menu_name) = actions.show_menu {
                adapter.show_menu(menu_name);
            }

            log::debug!(
                "UI updated: visible={}, state={:?}",
                actions.ui_visible,
                ui_state
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
            log::debug!("Audio event failed: {}", e);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_manager = app::event_loop::WindowManager::new();
        if let Err(e) = window_manager.handle_resumed(self, event_loop) {
            log::error!("{}", e);
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
        event_processor.process_input_events(self);

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
        // Dispatch the event to registered subscribers (UI first). If consumed,
        // skip further application-level handling.
        if self.dispatcher.dispatch(&event) {
            return;
        }

        let window_event_handler = app::event_loop::WindowEventHandler::new();
        window_event_handler.handle_window_event(self, event_loop, event);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let window_event_handler = app::event_loop::WindowEventHandler::new();
        window_event_handler.handle_device_event(self, event);
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::new();

    // Run the modern event loop with ApplicationHandler
    let _ = event_loop.run_app(&mut app);
}
