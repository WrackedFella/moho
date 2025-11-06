#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use crossbeam_channel::{Receiver, unbounded};
#[cfg(feature = "backend-wgpu")]
use legion::World;
#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use moho_ui::prefs::Prefs;
#[cfg(feature = "backend-wgpu")]
use std::sync::Arc;
#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use std::sync::Mutex;
#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "backend-wgpu")]
use std::time::{Duration, Instant};
#[cfg(feature = "backend-wgpu")]
use winit::application::ApplicationHandler;
#[cfg(feature = "backend-wgpu")]
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, StartCause, WindowEvent};
#[cfg(feature = "ui-egui")]
mod input_dispatcher;
#[cfg(feature = "ui-egui")]
use crate::input_dispatcher::InputDispatcher;
#[cfg(feature = "ui-egui")]
mod input_event;
#[cfg(feature = "backend-wgpu")]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(feature = "backend-wgpu")]
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(feature = "backend-wgpu")]
use winit::window::{CursorGrabMode, Window, WindowAttributes, WindowId};

// Import shared types
use moho_types::{AppState as SharedAppState, GameState};

// Core game state and input routing modules
#[cfg(feature = "backend-wgpu")]
mod game_state;
#[cfg(feature = "backend-wgpu")]
mod input_routing;

// Application initialization modules
#[cfg(feature = "backend-wgpu")]
mod app;

mod save;

#[cfg(feature = "ui-egui")]
enum GenerationMsg {
    Progress(f32),
    Completed {
        scene_bytes: Vec<u8>,
        spec: moho_core::scene_builders::WorldSpec,
    },
    Canceled,
    Failed(String),
}

#[cfg(feature = "ui-egui")]
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
#[cfg(feature = "backend-wgpu")]
struct WindowRenderer {
    window: Arc<Window>,
    renderer: Box<dyn moho_renderer::RendererBackend>,
    mesh_handle: u32,
    cube_mesh_handle: u32,
}

// Application state structure that implements ApplicationHandler
#[cfg(feature = "backend-wgpu")]
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
    #[cfg(feature = "ui-egui")]
    ui_event_rx: crossbeam_channel::Receiver<moho_core::events::UiEvent>,
    audio_event_rx: crossbeam_channel::Receiver<moho_core::events::AudioEvent>,
    graphics_event_rx: crossbeam_channel::Receiver<moho_core::events::GraphicsEvent>,

    // Audio system (not thread-safe, stays on main thread)
    audio_system: Option<moho_audio::AudioSystem>,

    // UI components
    #[cfg(feature = "ui-egui")]
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,
    #[cfg(feature = "ui-egui")]
    last_world_spec: Option<moho_core::scene_builders::WorldSpec>,
    // Async generation plumbing (only used when UI is present)
    #[cfg(feature = "ui-egui")]
    generation_receiver: Option<Receiver<GenerationMsg>>,
    #[cfg(feature = "ui-egui")]
    generation_handle: Option<std::thread::JoinHandle<()>>,
    #[cfg(feature = "ui-egui")]
    generation_cancel: Option<Arc<AtomicBool>>,

    // Input dispatcher (routes events to prioritized subscribers)
    #[cfg(feature = "ui-egui")]
    dispatcher: InputDispatcher,

    // Channel for simplified input events forwarded to the game when UI doesn't consume them
    #[cfg(feature = "ui-egui")]
    unconsumed_input_tx: Option<crossbeam_channel::Sender<crate::input_event::InputEvent>>,
    #[cfg(feature = "ui-egui")]
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

#[cfg(feature = "backend-wgpu")]
impl App {
    fn new() -> Self {
        // Load application configuration
        #[cfg(feature = "ui-egui")]
        let config = crate::app::config::AppConfig::from_prefs();
        #[cfg(not(feature = "ui-egui"))]
        let config = crate::app::config::AppConfig::default();
        
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

            #[cfg(feature = "ui-egui")]
            ui_event_rx: initialized.ui_event_rx,
            audio_event_rx: initialized.audio_event_rx,
            graphics_event_rx: initialized.graphics_event_rx,

            audio_system: initialized.audio_system,

            #[cfg(feature = "ui-egui")]
            ui_adapter: None,
            #[cfg(feature = "ui-egui")]
            generation_receiver: None,
            #[cfg(feature = "ui-egui")]
            generation_handle: None,
            #[cfg(feature = "ui-egui")]
            generation_cancel: None,

            #[cfg(feature = "ui-egui")]
            dispatcher: InputDispatcher::new(),

            #[cfg(feature = "ui-egui")]
            unconsumed_input_tx: None,
            #[cfg(feature = "ui-egui")]
            unconsumed_input_rx: None,

            // Camera control (moved into simulation)
            simulation: initialized.simulation,
            mouse_sensitivity: initialized.mouse_sensitivity,
            input_system: initialized.input_system,

            // Store prefs and keyboard state
            prefs: initialized.prefs,
            #[cfg(feature = "ui-egui")]
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
        #[cfg(feature = "ui-egui")]
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

        // Only start async generation when UI/adapter present; otherwise fall back
        // to the synchronous path for headless builds.
        #[cfg(not(feature = "ui-egui"))]
        {
            // Fallback synchronous path
            log::info!("No UI adapter available - falling back to sync generation");
            // Clear the existing world
            self.world.clear();
            moho_core::scene_builders::voxel_terrain_scene(&mut self.world);
            // Persist immediately
            let saves_dir = std::path::Path::new("saves");
            if !saves_dir.exists() {
                std::fs::create_dir_all(saves_dir)?;
            }
            let save_path = saves_dir.join("scene.bin");
            let (yaw, pitch) = self.simulation.yaw_pitch();
            let camera_data = Some((self.simulation.position(), yaw, pitch));
            let scene_bytes = self.scene.encode_to_bytes(&self.world, camera_data)?;
            save::write_scene_with_metadata(&save_path, &scene_bytes, &spec)?;
            self.game_state = crate::game_state::GameState::Playing;
            self.hide_menu();
            return Ok(());
        }

        #[cfg(feature = "ui-egui")]
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

        #[cfg(feature = "ui-egui")]
        {
            // Encode scene bytes and write envelope. Prefer the last known
            // WorldSpec (e.g. from a loaded or generated scene) so autosaves
            // preserve original metadata; fall back to a minimal spec.
            let scene_bytes = self.scene.encode_to_bytes(&self.world, camera_data)?;
            let spec =
                self.last_world_spec
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
        }

        #[cfg(not(feature = "ui-egui"))]
        {
            // Legacy path - write raw scene bytes without envelope
            self.scene
                .save_to_file(&save_path, &self.world, camera_data)?;
            log::info!("Auto-saved scene to {:?}", save_path);
        }

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

        // Load the scene from file. Use the envelope-aware loader when UI
        // feature is enabled so we can read the WorldSpec metadata.
        #[cfg(feature = "ui-egui")]
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

        #[cfg(not(feature = "ui-egui"))]
        {
            // Fallback: legacy loader (no metadata)
            let camera_data = self.scene.load_from_file(&path, &mut self.world)?;
            log::info!("Scene loaded successfully from {:?}", path.as_ref());
            if let Some((position, yaw, pitch)) = camera_data {
                self.simulation.set_position_yaw_pitch(position, yaw, pitch);
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
                                #[cfg(feature = "ui-egui")]
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
        self.game_state = crate::game_state::GameState::Playing;

        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            // Set UI to not visible directly
            adapter.set_visible(false);
            adapter.set_game_state(moho_ui::GameState::Playing);

            // Update the atomic flag
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);

            log::info!("Menu hidden - UI set to invisible");
        }

        // Update input router
        self.input_router
            .update_for_state(crate::game_state::GameState::Playing);

        // Grab cursor for game mode
        self.grab_cursor();
    }

    fn show_menu(&mut self) {
        self.game_state = crate::game_state::GameState::Menu;

        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            // Set UI to visible
            adapter.set_visible(true);
            adapter.set_game_state(moho_ui::GameState::Menu);

            // Update the atomic flag
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);

            log::info!("Menu shown - UI set to visible");
        }

        // Update input router
        self.input_router
            .update_for_state(crate::game_state::GameState::Menu);

        // Release cursor for menu mode
        self.release_cursor();
    }

    /// Enter console mode (opens debug console over game)
    fn enter_console(&mut self) {
        use crate::game_state::GameState;

        // Validate transition
        if !self.game_state.can_transition_to(GameState::ConsoleOpen) {
            log::warn!("Cannot open console from state: {:?}", self.game_state);
            return;
        }

        log::info!("Entering console mode");
        self.game_state = GameState::ConsoleOpen;

        // Update input router for console state
        self.input_router.update_for_state(GameState::ConsoleOpen);

        // Make UI visible for console overlay
        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            adapter.set_visible(true);
            adapter.set_game_state(moho_ui::GameState::ConsoleOpen);
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        // Release cursor so user can type
        self.release_cursor();
    }

    /// Exit console mode (return to playing)
    fn exit_console(&mut self) {
        use crate::game_state::GameState;

        // Validate transition
        if !self.game_state.can_transition_to(GameState::Playing) {
            log::warn!("Cannot exit console from state: {:?}", self.game_state);
            return;
        }

        log::info!("Exiting console mode");
        self.game_state = GameState::Playing;

        // Update input router for playing state
        self.input_router.update_for_state(GameState::Playing);

        // Hide UI when returning to game
        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            adapter.set_visible(false);
            adapter.set_game_state(moho_ui::GameState::Playing);
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Grab cursor for game mode
        self.grab_cursor();
    }

    /// Toggle pause state
    #[allow(dead_code)]
    fn toggle_pause(&mut self) {
        use crate::game_state::GameState;

        match self.game_state {
            GameState::Playing => {
                if self.game_state.can_transition_to(GameState::Paused) {
                    log::info!("Pausing game");
                    self.game_state = GameState::Paused;
                    self.input_router.update_for_state(GameState::Paused);

                    #[cfg(feature = "ui-egui")]
                    if let Some(ui_adapter) = &self.ui_adapter
                        && let Ok(mut adapter) = ui_adapter.lock()
                    {
                        adapter.set_visible(true);
                        adapter.set_game_state(moho_ui::GameState::Paused);
                        use moho_ui::UI_OVERLAY_VISIBLE;
                        UI_OVERLAY_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
                    }

                    self.release_cursor();
                }
            }
            GameState::Paused => {
                if self.game_state.can_transition_to(GameState::Playing) {
                    log::info!("Resuming game");
                    self.game_state = GameState::Playing;
                    self.input_router.update_for_state(GameState::Playing);

                    #[cfg(feature = "ui-egui")]
                    if let Some(ui_adapter) = &self.ui_adapter
                        && let Ok(mut adapter) = ui_adapter.lock()
                    {
                        adapter.set_visible(false);
                        adapter.set_game_state(moho_ui::GameState::Playing);
                        use moho_ui::UI_OVERLAY_VISIBLE;
                        UI_OVERLAY_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);
                    }

                    self.grab_cursor();
                }
            }
            _ => {
                log::debug!("Cannot toggle pause from state: {:?}", self.game_state);
            }
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

#[cfg(feature = "backend-wgpu")]
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window_renderer.is_none() {
            // Set a descriptive title for the main window instead of the default
            let mut window_attributes = WindowAttributes::default();
            window_attributes.title = "Project: Moho - Prototype".into();
            match event_loop.create_window(window_attributes) {
                Ok(window) => {
                    let window_arc = Arc::new(window);
                    if let Err(e) = self.setup_renderer_and_ui(window_arc.clone()) {
                        log::error!("Failed to setup renderer and UI: {}", e);
                        event_loop.exit();
                        return;
                    }

                    // Initial render
                    log::info!("Initial render");
                    if let Some(ref mut wr) = self.window_renderer {
                        self.scene.render(
                            &mut *wr.renderer,
                            &mut self.world,
                            wr.mesh_handle,
                            wr.cube_mesh_handle,
                            self.camera,
                        );
                        wr.window.request_redraw();
                    }
                }
                Err(e) => {
                    log::error!("Failed to create window: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        let now = Instant::now();
        if now >= self.last_frame + self.frame_duration {
            let dt = self.frame_duration.as_secs_f32();
            self.last_frame += self.frame_duration;

            // Publish frame start event
            static FRAME_COUNTER: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let frame_number = FRAME_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.event_bus
                .publish(moho_core::events::SystemEvent::FrameStart {
                    frame_number,
                    delta_time: dt,
                });

            // Update camera controls if in game mode
            if self.game_state == crate::game_state::GameState::Playing {
                // Update controller input from keyboard state
                self.update_controller_input();

                let (yaw_delta, pitch_delta) = self.input_system.sample_frame_input();
                {
                    let ci = self.simulation.controller_input_mut();
                    ci.yaw_delta = yaw_delta;
                    ci.pitch_delta = pitch_delta;
                }

                // Apply input via simulation wrapper and update camera from returned tuple.
                let (view, proj, eye) = self.simulation.apply_input(dt);
                self.camera = (view, proj, eye);

                // Update lighting based on celestial positions from game clock
                if let Some(ref mut wr) = self.window_renderer {
                    let (sun_dir, moon_dir) = self.simulation.celestial_directions();
                    let time = self.simulation.time_of_day();

                    // Calculate sun intensity (0 when below horizon)
                    let sun_intensity = if sun_dir.y > 0.0 { 1.0 } else { 0.0 };

                    // Calculate moon intensity based on position and time
                    // Moon is stronger at night, weaker during day transitions
                    let moon_base_intensity = if moon_dir.y > 0.0 {
                        // Moon is above horizon
                        if !(5.0..21.0).contains(&time) {
                            // Deep night: full moon brightness
                            0.4
                        } else if (5.0..7.0).contains(&time) {
                            // Dawn: moon fading
                            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
                            0.4 * (1.0 - t) // 0.4 -> 0.0
                        } else if (17.0..21.0).contains(&time) {
                            // Dusk: moon rising
                            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
                            0.4 * t // 0.0 -> 0.4
                        } else {
                            // Day: moon barely visible if at all
                            0.0
                        }
                    } else {
                        // Moon below horizon
                        0.0
                    };

                    // Calculate ambient lighting based on time of day
                    // Night: 0-5, 21-24 (very low, blue-tinted)
                    // Dawn: 5-7 (increasing, warm tint)
                    // Day: 7-17 (full brightness, neutral)
                    // Dusk: 17-21 (decreasing, warm tint)
                    let (ambient_color, ambient_intensity) = if (7.0..17.0).contains(&time) {
                        // Day: full brightness, cool ambient
                        ([0.4, 0.5, 0.6], 0.15)
                    } else if (5.0..7.0).contains(&time) {
                        // Dawn: increasing brightness, warm tint
                        let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
                        let intensity = 0.05 + t * 0.10; // 0.05 -> 0.15
                        ([0.5, 0.45, 0.4], intensity)
                    } else if (17.0..21.0).contains(&time) {
                        // Dusk: decreasing brightness, warm tint
                        let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
                        let intensity = 0.15 - t * 0.10; // 0.15 -> 0.05
                        ([0.5, 0.4, 0.35], intensity)
                    } else {
                        // Night: very low brightness, blue tint
                        ([0.3, 0.35, 0.5], 0.05)
                    };

                    let lighting = moho_renderer::LightingGpu {
                        sun_direction: [sun_dir.x, sun_dir.y, sun_dir.z, sun_intensity],
                        sun_color: [1.0, 0.95, 0.8, 0.0], // Warm sunlight
                        moon_direction: [moon_dir.x, moon_dir.y, moon_dir.z, moon_base_intensity],
                        moon_color: [0.7, 0.8, 0.9, 0.0], // Silver-blue moonlight
                        ambient: [
                            ambient_color[0],
                            ambient_color[1],
                            ambient_color[2],
                            ambient_intensity,
                        ],
                        time_of_day: [time, 0.0, 0.0, 0.0],
                    };
                    wr.renderer.update_lighting(lighting);
                }
            }

            if let Some(ref wr) = self.window_renderer {
                wr.window.request_redraw();
            }

            // Process UI events from event bus
            #[cfg(feature = "ui-egui")]
            {
                while let Ok(event) = self.ui_event_rx.try_recv() {
                    match event {
                        moho_core::events::UiEvent::LoadSceneRequested { path } => {
                            log::info!("UI requested load scene: {:?}", path);
                            if let Err(e) = self.load_scene(&path) {
                                log::error!("Failed to load scene from {:?}: {}", path, e);
                            }
                        }
                        moho_core::events::UiEvent::NewWorldRequested { name, seed, size } => {
                            log::info!(
                                "UI requested new world: {} (seed: {:?}, size: {})",
                                name,
                                seed,
                                size
                            );
                            let spec = moho_core::scene_builders::WorldSpec {
                                name,
                                seed,
                                size_xz: size,
                                day_length_seconds: 600.0,
                                night_length_seconds: 420.0,
                                initial_time_of_day: 6.0,
                            };
                            if let Err(e) = self.generate_new_world(spec) {
                                log::error!("Failed to generate new world: {}", e);
                            }
                        }
                        moho_core::events::UiEvent::ExitRequested => {
                            log::info!("UI requested exit");
                            // Auto-save before exit
                            if let Err(e) = self.auto_save_on_shutdown() {
                                log::warn!("Failed to auto-save on exit: {}", e);
                            }
                            event_loop.exit();
                        }
                        moho_core::events::UiEvent::MenuShown { name } => {
                            log::info!("UI requested show menu: {}", name);
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut adapter) = ui_adapter.lock()
                            {
                                adapter.show_menu(&name);
                            }
                        }
                        moho_core::events::UiEvent::MenuHidden { name } => {
                            log::info!("Menu hidden: {}", name);
                            // Handle console close event
                            if name == "console" {
                                self.exit_console();
                            }
                        }
                        moho_core::events::UiEvent::OverlayToggled { name, visible } => {
                            log::info!("Overlay {} toggled: {}", name, visible);
                            if visible && let Some(ref wr) = self.window_renderer {
                                wr.window.set_cursor_visible(true);
                            }
                        }
                        moho_core::events::UiEvent::SettingsSaved => {
                            log::info!("Settings saved");
                            // Settings are already saved by the UI adapter
                            // Here we could reload/apply them if needed
                        }
                    }
                }
            }

            // Process audio events from event bus
            {
                while let Ok(event) = self.audio_event_rx.try_recv() {
                    // Map core::events::AudioEvent to moho_audio::AudioEvent
                    let audio_event = match event {
                        moho_core::events::AudioEvent::ButtonClick => {
                            moho_audio::AudioEvent::ButtonClick
                        }
                        moho_core::events::AudioEvent::MenuNavigate => {
                            moho_audio::AudioEvent::MenuNavigate
                        }
                        moho_core::events::AudioEvent::Confirm => moho_audio::AudioEvent::Confirm,
                        moho_core::events::AudioEvent::Cancel => moho_audio::AudioEvent::Cancel,
                        moho_core::events::AudioEvent::Error => moho_audio::AudioEvent::Error,
                        moho_core::events::AudioEvent::PlaySound { path, volume } => {
                            moho_audio::AudioEvent::CustomSound { path, volume }
                        }
                        moho_core::events::AudioEvent::MusicStart {
                            path,
                            volume,
                            looped,
                        } => moho_audio::AudioEvent::BackgroundMusic {
                            path,
                            volume,
                            looped,
                        },
                        moho_core::events::AudioEvent::MusicStop => {
                            moho_audio::AudioEvent::Stop(moho_audio::AudioCategory::Music)
                        }
                        moho_core::events::AudioEvent::MusicVolumeChanged { volume: _ } => {
                            // Skip - not implemented in current audio system
                            continue;
                        }
                        moho_core::events::AudioEvent::StopAll => {
                            moho_audio::AudioEvent::Stop(moho_audio::AudioCategory::All)
                        }
                    };

                    self.handle_audio_event(audio_event);
                }
            }

            // Process graphics events from event bus
            {
                while let Ok(event) = self.graphics_event_rx.try_recv() {
                    match event {
                        moho_core::events::GraphicsEvent::TimeOfDayChanged { time, .. } => {
                            // Set the game clock time directly (time is in hours 0-24)
                            self.simulation.set_time_of_day(time);
                            log::info!(
                                "Time set to {:.2} ({})",
                                time,
                                self.simulation.game_clock().time_string()
                            );
                        }
                        _ => {
                            // Other graphics events not yet handled
                        }
                    }
                }
            }

            // Drain simplified input events forwarded by the dispatcher (e.g., mouse wheel)
            if let Some(rx) = &self.unconsumed_input_rx {
                while let Ok(iev) = rx.try_recv() {
                    match iev {
                        crate::input_event::InputEvent::MouseWheel { delta_y } => {
                            // Only act on wheel events in game mode
                            if self.game_state == crate::game_state::GameState::Playing {
                                // Simple zoom: move player forward/back along look direction
                                let dz = delta_y * 0.5; // tuning factor
                                let (yaw, pitch) = self.simulation.yaw_pitch();
                                let sy = yaw.sin();
                                let cy = yaw.cos();
                                let cp = pitch.cos();
                                let sp = pitch.sin();
                                let forward =
                                    glam::Vec3::new(sy * cp, sp, cy * cp).normalize_or_zero();
                                let new_pos = self.simulation.position() + forward * dz;
                                self.simulation.set_position_yaw_pitch(new_pos, yaw, pitch);
                            }
                        }
                    }
                }
            }

            // If the UI progress overlay Cancel button was pressed, propagate
            // cancellation to the generator thread by setting the atomic flag.
            #[cfg(feature = "ui-egui")]
            if let Some(ui_adapter) = &self.ui_adapter
                && let Ok(mut a) = ui_adapter.lock()
                && a.take_progress_canceled()
                && let Some(cancel_flag) = &self.generation_cancel
            {
                cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }

            // Poll async generation channel if running. Take the receiver so
            // we can mutate `self` while processing messages.
            if let Some(rx) = self.generation_receiver.take() {
                let mut still_running = true;
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        GenerationMsg::Progress(p) => {
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut a) = ui_adapter.lock()
                            {
                                a.set_progress(p);
                            }
                        }
                        GenerationMsg::Completed { scene_bytes, spec } => {
                            log::info!("Generation completed for spec={:?}", spec.name);
                            // Remember the last WorldSpec so future saves include
                            // the original metadata (autosave/continue consistency).
                            self.last_world_spec = Some(spec.clone());
                            // Complete progress UI
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut a) = ui_adapter.lock()
                            {
                                a.set_progress(1.0);
                                a.finish_progress();
                            }

                            // Load produced scene bytes into the main world
                            self.world.clear();
                            match self.scene.load_from_bytes(&scene_bytes, &mut self.world) {
                                Ok(camera_data) => {
                                    if let Some((position, yaw, pitch)) = camera_data {
                                        self.simulation
                                            .set_position_yaw_pitch(position, yaw, pitch);
                                        // Clear any pending input so the restored camera
                                        // orientation isn't immediately overridden by
                                        // accumulated mouse deltas or smoothing state.
                                        self.input_system.clear_pending_input();
                                    }
                                }
                                Err(e) => {
                                    log::error!("Failed to load generated scene bytes: {}", e);
                                }
                            }

                            // Clean up generation state
                            if let Some(h) = self.generation_handle.take() {
                                let _ = h.join();
                            }
                            self.generation_cancel = None;

                            // Switch to game mode and hide menu
                            self.game_state = crate::game_state::GameState::Playing;
                            self.hide_menu();

                            still_running = false;
                        }
                        GenerationMsg::Canceled => {
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut a) = ui_adapter.lock()
                            {
                                a.finish_progress();
                            }
                            if let Some(h) = self.generation_handle.take() {
                                let _ = h.join();
                            }
                            self.generation_cancel = None;
                            log::info!("Generation canceled by user");
                            still_running = false;
                        }
                        GenerationMsg::Failed(reason) => {
                            log::error!("Generation failed: {}", reason);
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut a) = ui_adapter.lock()
                            {
                                a.finish_progress();
                            }
                            if let Some(h) = self.generation_handle.take() {
                                let _ = h.join();
                            }
                            self.generation_cancel = None;
                            still_running = false;
                        }
                    }
                }

                if still_running {
                    // Put the receiver back for future polling
                    self.generation_receiver = Some(rx);
                } else {
                    // Drop the receiver and clear state
                    self.generation_receiver = None;
                }
            }

            // Publish frame end event and process deferred events
            self.event_bus
                .publish(moho_core::events::SystemEvent::FrameEnd { frame_number });
            self.event_bus.process_deferred();

            let next = self.last_frame + self.frame_duration;
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        } else {
            let next = self.last_frame + self.frame_duration;
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Dispatch the event to registered subscribers (UI first). If consumed,
        // skip further application-level handling.
        //
        // NOTE: InputRouter is maintained but not used for dispatch here.
        // The InputDispatcher provides UI-first routing which works well with egui.
        // See TODO.md Task 10 for architectural discussion.
        #[cfg(feature = "ui-egui")]
        {
            if self.dispatcher.dispatch(&event) {
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                // Auto-save before close
                if let Err(e) = self.auto_save_on_shutdown() {
                    log::warn!("Failed to auto-save on close: {}", e);
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut wr) = self.window_renderer {
                    wr.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard_input(&key_event);
            }
            WindowEvent::RedrawRequested => {
                log::debug!("RedrawRequested - rendering frame");
                if let Some(ref mut wr) = self.window_renderer {
                    self.scene.render(
                        &mut *wr.renderer,
                        &mut self.world,
                        wr.mesh_handle,
                        wr.cube_mesh_handle,
                        self.camera,
                    );
                }

                // Recall staging belt after render
                #[cfg(feature = "ui-egui")]
                if let Some(ui_adapter) = &self.ui_adapter
                    && let Ok(mut a) = ui_adapter.lock()
                {
                    a.recall_staging_belt();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        // Handle raw mouse motion for camera look
        if let DeviceEvent::MouseMotion { delta } = event {
            self.handle_mouse_motion(delta);
        }
    }
}

#[cfg(feature = "backend-wgpu")]
fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::new();

    // Run the modern event loop with ApplicationHandler
    let _ = event_loop.run_app(&mut app);
}

#[cfg(not(feature = "backend-wgpu"))]
fn main() {
    eprintln!("This binary requires the 'backend-wgpu' feature to be enabled.");
    eprintln!("Run with: cargo run --features backend-wgpu");
    std::process::exit(1);
}
