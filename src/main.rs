//! Moho — a voxel game engine and application binary.
//!
//! This is the main executable that wires together the engine crates
//! ([`moho_core`], [`moho_renderer`], [`moho_audio`], [`moho_ui`],
//! [`moho_sim`]) into a runnable application via [`winit`]'s event loop.

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

// Core game state module
mod game_state;

// Application initialization modules
mod app;

mod save;

enum GenerationMsg {
    Progress(f32),
    Completed {
        scene_bytes: Vec<u8>,
        spec: moho_core::scene_builders::WorldSpec,
        // Pass the generated grid back to the main thread
        grid: moho_core::voxel::VoxelGrid,
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

    // Voxel grid and light propagation system
    voxel_grid: Option<moho_core::voxel::VoxelGrid>,
    light_system: Option<moho_core::voxel::LightSystem>,

    // Game state management (replaces old AppMode)
    game_state: crate::game_state::GameState,

    // Runtime state (initialized after window creation)
    window_renderer: Option<WindowRenderer>,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Event collection channels (for events that need to mutate App state)
    ui_event_rx: crossbeam_channel::Receiver<moho_core::events::UiEvent>,
    audio_event_rx: crossbeam_channel::Receiver<moho_core::events::AudioEvent>,
    graphics_event_rx: crossbeam_channel::Receiver<moho_core::events::GraphicsEvent>,
    world_event_rx: crossbeam_channel::Receiver<moho_core::events::WorldEvent>,
    debug_event_rx: crossbeam_channel::Receiver<moho_core::events::DebugEvent>,

    // Audio system (not thread-safe, stays on main thread)
    audio_system: Option<moho_audio::AudioSystem>,

    // UI components
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,
    last_world_spec: Option<moho_core::scene_builders::WorldSpec>,
    // Async generation plumbing (only used when UI is present)
    generation_receiver: Option<Receiver<GenerationMsg>>,
    generation_handle: Option<std::thread::JoinHandle<()>>,
    generation_cancel: Option<Arc<AtomicBool>>,

    // Debug state
    debug_mode: u32,

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

    // Tracks gizmo spheres spawned alongside debug point lights (light_id -> entity)
    light_gizmos: std::collections::HashMap<u32, legion::Entity>,

    // Physics
    physics_world: Option<moho_physics::PhysicsWorld>,
    chunk_colliders: std::collections::HashMap<glam::IVec3, moho_physics::ColliderHandle>,
    test_physics_bodies: Vec<(moho_physics::RigidBodyHandle, legion::Entity)>,
    jump_pressed: bool,
}

impl App {
    fn new() -> Self {
        // Load application configuration
        let config = crate::app::config::AppConfig::from_prefs();

        // Initialize all systems using the builder
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
        log::info!("Created LightSystem for frame loop integration");

        Self {
            world: initialized.world,
            scene: initialized.scene,
            camera: initialized.camera,

            // Voxel grid and light system (minimal for testing)
            // Note: Grid is moved into LightSystem, so we don't store it separately
            voxel_grid: None,
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
            generation_receiver: None,
            generation_handle: None,
            generation_cancel: None,

            debug_mode: 0,

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

            light_gizmos: std::collections::HashMap::new(),

            physics_world: Some(moho_physics::PhysicsWorld::new()),
            chunk_colliders: std::collections::HashMap::new(),
            test_physics_bodies: Vec::new(),
            jump_pressed: false,
        }
    }

    fn setup_renderer_and_ui(
        &mut self,
        window: Arc<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: The renderer requires a &'static Window because Box<dyn RendererBackend>
        // is implicitly 'static. We clone the Arc<Window> before leaking it, so the Arc
        // refcount keeps the Window alive independently of `window`. This is a deliberate
        // one-time leak for the main window, which lives for the entire program lifetime.
        // If winit ever allows window recreation (e.g. fullscreen toggle) this should be
        // replaced by giving RendererBackend a lifetime parameter (TD-09 / TD-06).
        let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
        let mut renderer = moho_renderer::create_renderer(Some(window_ref))?;

        // Apply initial quality settings
        renderer.set_shadow_quality(self.prefs.shadow_quality() as u8);
        renderer.set_ssao_quality(self.prefs.ssao_quality() as u8);

        // Create sphere mesh data using the proper sphere geometry
        let (vertices, normals, indices) = moho_core::actors::Sphere::unit_sphere_indexed(16, 16);
        let ao_data = vec![1.0; vertices.len()]; // Full brightness for non-voxel geometry
        let geo_type = vec![1; vertices.len()]; // Type 1 (blocky/non-voxel)
        let light_level = vec![1.0; vertices.len()]; // Full light for non-voxel geometry
        let mesh_handle = renderer.register_indexed_mesh(
            &vertices,
            &normals,
            &ao_data,
            &geo_type,
            &light_level,
            &indices,
        );

        let (cube_vertices, cube_normals, cube_indices) =
            moho_core::actors::Cube::unit_cube_indexed();
        let cube_ao = vec![1.0; cube_vertices.len()];
        let cube_geo_type = vec![1; cube_vertices.len()];
        let cube_light_level = vec![1.0; cube_vertices.len()];
        let cube_mesh_handle = renderer.register_indexed_mesh(
            &cube_vertices,
            &cube_normals,
            &cube_ao,
            &cube_geo_type,
            &cube_light_level,
            &cube_indices,
        );

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

            // Register UI adapter as a normal UI subscriber.
            // Only forward events to egui when menus/console are visible;
            // during gameplay the overlays are passive and don't need input.
            let ui_adapter_clone = ui_adapter.clone();
            self.dispatcher.register(100, move |event: &WindowEvent| {
                if let Ok(mut a) = ui_adapter_clone.lock() {
                    if a.is_visible() {
                        a.handle_winit_event(event)
                    } else {
                        false
                    }
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
                    let grid = moho_core::scene_builders::voxel_terrain_scene_with_config(
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
                        &[], // fresh world — no spawned lights
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
                    let block_records: Vec<save::BlockRecord> =
                        grid.iter_blocks().map(save::BlockRecord::from_block).collect();
                    if let Err(e) = save::write_scene_with_metadata(
                        &save_path,
                        &scene_bytes,
                        &spec_for_thread,
                        &block_records,
                    ) {
                        let _ = sender.send(GenerationMsg::Failed(format!("write failed: {}", e)));
                        return;
                    }

                    let _ = sender.send(GenerationMsg::Progress(1.0));
                    let _ = sender.send(GenerationMsg::Completed {
                        scene_bytes,
                        spec: spec_for_thread,
                        grid,
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
        let scene_bytes = self.scene.encode_to_bytes(
            &self.world,
            camera_data,
            &self
                .window_renderer
                .as_ref()
                .map_or_else(Vec::new, |wr| wr.renderer.all_lights_as_descs()),
        )?;
        let mut spec =
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
        // Persist the current time of day so Continue resumes at the right time.
        spec.initial_time_of_day = self.simulation.time_of_day();
        let block_records: Vec<save::BlockRecord> = self
            .light_system
            .as_ref()
            .map(|ls| ls.grid().iter_blocks().map(save::BlockRecord::from_block).collect())
            .unwrap_or_default();
        save::write_scene_with_metadata(&save_path, &scene_bytes, &spec, &block_records)?;
        log::info!(
            "Auto-saved scene (envelope) to {:?} (spec={:?}, {} blocks)",
            save_path,
            spec.name,
            block_records.len()
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
            let (spec, scene_bytes, block_records) = save::read_scene_and_metadata(&path)?;
            // Remember the WorldSpec from the loaded file so autosaves and
            // subsequent writes preserve the original metadata.
            self.last_world_spec = Some(spec.clone());
            log::info!("Loaded WorldSpec from save: {:?}", spec);
            // Restore time of day from the persisted WorldSpec.
            self.simulation.set_time_of_day(spec.initial_time_of_day);
            let (camera_data, lights) =
                self.scene.load_from_bytes(&scene_bytes, &mut self.world)?;
            log::info!(
                "Scene loaded successfully from {:?}, {} lights",
                path.as_ref(),
                lights.len()
            );

            // Re-add persisted lights to the renderer
            if let Some(ref mut wr) = self.window_renderer {
                for desc in &lights {
                    if desc.enabled {
                        wr.renderer.add_point_light(
                            glam::Vec3::from_array(desc.position),
                            glam::Vec3::from_array(desc.color),
                            desc.intensity,
                            desc.range,
                        );
                    }
                }
            }

            // Reconstruct VoxelGrid from persisted block records, then initialize LightSystem.
            let mut grid = moho_core::voxel::VoxelGrid::new(16);
            if block_records.is_empty() {
                // KNOWN LIMITATION: v1 saves do not contain block data. Light propagation
                // will be inactive until the world is regenerated and saved in v2 format.
                log::warn!(
                    "Save file contains no block data (v1 format). \
                     Light propagation disabled for this session. \
                     Regenerate the world to fix permanently."
                );
            } else {
                for record in &block_records {
                    let pos = moho_core::voxel::BlockPos::new(record.x, record.y, record.z);
                    let mut block = moho_core::voxel::VoxelBlock::new(pos, record.material_id);
                    block.resource_id = record.resource_id;
                    grid.set_block(pos, block);
                }
                log::info!(
                    "Reconstructed VoxelGrid with {} blocks for LightSystem",
                    block_records.len()
                );
            }
            self.light_system = Some(moho_core::voxel::LightSystem::with_default_budget(
                grid,
                self.event_bus.clone(),
            ));

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

        // Initialize physics for loaded world.
        self.setup_physics_for_loaded_world();

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

    /// Initialize physics world after a scene is loaded.
    fn setup_physics_for_loaded_world(&mut self) {
        self.physics_world = Some(moho_physics::PhysicsWorld::new());
        self.chunk_colliders.clear();
        self.test_physics_bodies.clear();

        crate::app::event_loop::EventProcessor::sync_chunk_colliders(self);

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
        if let Some(ref mut pw) = self.physics_world {
            pw.add_character(spawn_pos);
        }

        // Align the simulation Y to match physics spawn (avoids terrain clipping).
        let (yaw, pitch) = self.simulation.yaw_pitch();
        self.simulation
            .set_position_yaw_pitch(spawn_pos, yaw, pitch);

        log::info!(
            "Physics initialized for loaded world: {} chunk colliders",
            self.chunk_colliders.len()
        );
    }

    /// Update controller input from keyboard state
    fn update_controller_input(&mut self) {
        // Helper to check if a binding is currently active
        let is_active = |binding: &moho_ui::prefs::Binding| -> bool {
            self.active_keys.contains(&(binding.code, binding.mods))
        };

        // Calculate forward/backward
        let forward = if is_active(&self.prefs.key_w()) {
            1.0
        } else {
            0.0
        } - if is_active(&self.prefs.key_s()) {
            1.0
        } else {
            0.0
        };

        // Calculate left/right (A is left, so negative)
        let right = if is_active(&self.prefs.key_d()) {
            1.0
        } else {
            0.0
        } - if is_active(&self.prefs.key_a()) {
            1.0
        } else {
            0.0
        };

        // Calculate up/down - only in first person mode
        let up = if self.simulation.camera_mode() == moho_core::controller::CameraMode::FirstPerson
        {
            (if is_active(&self.prefs.key_up()) {
                1.0
            } else {
                0.0
            }) - (if is_active(&self.prefs.key_down()) {
                1.0
            } else {
                0.0
            })
        } else {
            0.0 // No up/down in isometric mode
        };

        // Check if sprint is active (Shift)
        let sprint = is_active(&self.prefs.key_sprint());

        self.simulation.controller_input.forward = forward;
        self.simulation.controller_input.right = right;
        self.simulation.controller_input.up = up;
        self.simulation.controller_input.sprint = sprint;
        // Zoom is handled separately via mouse wheel input
        self.simulation.controller_input.zoom_delta = 0.0;

        // Track jump key for physics KCC
        self.jump_pressed = is_active(&self.prefs.key_jump());
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
                KeyCode::F3 => {
                    // F3 toggles the debug HUD overlay
                    if pressed
                        && let Some(ui_adapter) = &self.ui_adapter
                        && let Ok(mut adapter) = ui_adapter.lock()
                    {
                        adapter.toggle_debug_hud();
                    }
                    return;
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
                    moho_core::controller::CameraMode::FirstPerson => {
                        // Switch to RTS first so look_at runs in Isometric mode,
                        // setting rts_look_target without touching FPS yaw/pitch.
                        self.simulation
                            .set_camera_mode(moho_core::controller::CameraMode::Isometric);
                        self.simulation.look_at(self.simulation.position());
                    }
                    moho_core::controller::CameraMode::Isometric => {
                        self.simulation
                            .set_camera_mode(moho_core::controller::CameraMode::FirstPerson);
                    }
                }
            log::info!(
                "Switched to camera mode: {:?}",
                self.simulation.camera_mode()
            );
            return; // Don't dispatch Tab further
        }

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

impl Drop for App {
    fn drop(&mut self) {
        log::info!("Shutting down application...");

        // Signal cancellation to any running background generation
        if let Some(cancel) = &self.generation_cancel {
            cancel.store(true, Ordering::SeqCst);
        }

        // Wait for generation thread to finish
        if let Some(handle) = self.generation_handle.take() {
            log::info!("Waiting for background generation to finish...");
            if handle.join().is_err() {
                log::error!("Failed to join generation thread");
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::new();

    // Run the modern event loop with ApplicationHandler
    let _ = event_loop.run_app(&mut app);
}
