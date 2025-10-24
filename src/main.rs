#[cfg(feature = "backend-wgpu")]
use legion::World;
#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use moho_ui::prefs::Prefs;
#[cfg(feature = "backend-wgpu")]
use std::sync::Arc;
#[cfg(all(feature = "backend-wgpu", feature = "ui-egui"))]
use std::sync::Mutex;
#[cfg(feature = "backend-wgpu")]
use std::time::{Duration, Instant};
#[cfg(feature = "backend-wgpu")]
use winit::application::ApplicationHandler;
#[cfg(feature = "backend-wgpu")]
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, StartCause, WindowEvent};
#[cfg(feature = "backend-wgpu")]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
#[cfg(feature = "backend-wgpu")]
use winit::keyboard::{KeyCode, PhysicalKey};
#[cfg(feature = "backend-wgpu")]
use winit::window::{CursorGrabMode, Window, WindowAttributes, WindowId};

// Combined state to handle lifetimes properly
#[cfg(feature = "backend-wgpu")]
struct WindowRenderer {
    window: Arc<Window>,
    renderer: Box<dyn engine_renderer::RendererBackend>,
    mesh_handle: u32,
    cube_mesh_handle: u32,
}

// App state to manage different modes
#[cfg(feature = "backend-wgpu")]
#[derive(Clone, Copy, Debug, PartialEq)]
enum AppMode {
    Menu,
    Game,
}

// Application state structure that implements ApplicationHandler
#[cfg(feature = "backend-wgpu")]
struct App {
    world: World,
    scene: engine_renderer::Scene,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    mode: AppMode,

    // Runtime state (initialized after window creation)
    window_renderer: Option<WindowRenderer>,

    // Audio system
    audio_system: Option<engine_audio::AudioSystem>,

    // UI components
    #[cfg(feature = "ui-egui")]
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,
    #[cfg(feature = "ui-egui")]
    ui_receiver: Option<moho_ui::UiReceiver>,

    // Camera control
    player_controller: engine_core::controller::PlayerController,
    controller_input: engine_core::controller::ControllerInput,
    mouse_sensitivity: f32,
    input_system: engine_core::input::InputSystem,

    // Keyboard state tracking
    key_w: bool,
    key_a: bool,
    key_s: bool,
    key_d: bool,
    key_space: bool,
    key_shift: bool,

    // Frame timing
    frame_duration: Duration,
    last_frame: Instant,
}

#[cfg(feature = "backend-wgpu")]
impl App {
    fn new() -> Self {
        // Initialize logging
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        log::info!("Starting minimal menu test");

        // Create basic world and scene (minimal setup)
        let world = World::default();
        let scene = engine_renderer::Scene::new();

        // Basic camera - positioned to get a good view of voxel terrain
        let camera = {
            let eye = glam::Vec3::new(40.0, 25.0, 40.0);
            let center = glam::Vec3::new(0.0, 8.0, 0.0);
            let up = glam::Vec3::new(0.0, 1.0, 0.0);
            let view = glam::Mat4::look_at_rh(eye, center, up);
            let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 200.0f32);
            (view, proj, eye)
        };

        // Initialize player controller at the camera position
        let player_controller = engine_core::controller::PlayerController::new(camera.2);

        // Load preferences and apply mouse sensitivity
        #[cfg(feature = "ui-egui")]
        let prefs = Prefs::load();
        #[cfg(feature = "ui-egui")]
        let mouse_sensitivity = prefs.mouse_sensitivity * 0.002;
        #[cfg(not(feature = "ui-egui"))]
        let mouse_sensitivity = 0.002;

        // Always use the default filter preset
        let engine_filter_preset = engine_core::input::FilterPreset::Default;

        // Initialize audio system
        let audio_system = match engine_audio::AudioSystem::new() {
            Ok(audio) => {
                log::info!("Audio system initialized successfully");
                Some(audio)
            }
            Err(e) => {
                log::warn!("Failed to initialize audio system: {}", e);
                None
            }
        };

        Self {
            world,
            scene,
            camera,
            mode: AppMode::Menu, // Start in menu mode
            window_renderer: None,
            audio_system,

            #[cfg(feature = "ui-egui")]
            ui_adapter: None,
            #[cfg(feature = "ui-egui")]
            ui_receiver: None,

            // Camera control
            player_controller,
            controller_input: engine_core::controller::ControllerInput::default(),
            mouse_sensitivity,
            input_system: {
                let mut input_sys = engine_core::input::InputSystem::new_with_preset(
                    mouse_sensitivity,
                    engine_filter_preset,
                );
                #[cfg(feature = "ui-egui")]
                input_sys.set_filter_enabled(prefs.input_filtering_enabled);
                input_sys
            },

            // Keyboard state
            key_w: false,
            key_a: false,
            key_s: false,
            key_d: false,
            key_space: false,
            key_shift: false,

            frame_duration: Duration::from_secs_f64(1.0 / 60.0),
            last_frame: Instant::now(),
        }
    }

    fn setup_renderer_and_ui(
        &mut self,
        window: Arc<Window>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create renderer - leak the Arc to get a 'static reference
        // This is acceptable for a main application window that lives for the program duration
        let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
        let mut renderer = engine_renderer::create_renderer(Some(window_ref))?;

        // Create sphere mesh data using the proper sphere geometry
        let (vertices, normals, indices) = engine_core::actors::Sphere::unit_sphere_indexed(16, 16);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);

        let (cube_vertices, cube_normals, cube_indices) =
            engine_core::actors::Cube::unit_cube_indexed();
        let cube_mesh_handle =
            renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

        // UI setup
        #[cfg(feature = "ui-egui")]
        {
            let (adapter, receiver) = moho_ui::build_adapter(Some(window.clone()));
            let ui_adapter = Arc::new(Mutex::new(adapter));

            if let Ok(mut a) = ui_adapter.lock() {
                a.set_surface_format(wgpu::TextureFormat::Bgra8UnormSrgb);
            }

            renderer.set_frame_callback_arc(Some(
                ui_adapter.clone() as Arc<Mutex<dyn engine_renderer::FrameCallback>>
            ));

            self.ui_adapter = Some(ui_adapter);
            self.ui_receiver = Some(receiver);
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

    fn generate_new_world(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Generating new world...");

        // Clear the existing world
        self.world.clear();

        // Generate voxel terrain scene
        engine_core::scene_builders::voxel_terrain_scene(&mut self.world);
        
        // Old random scene (for testing/fallback)
        // engine_core::scene_builders::random_scene(&mut self.world);

        // Ensure saves directory exists
        let saves_dir = std::path::Path::new("saves");
        if !saves_dir.exists() {
            std::fs::create_dir_all(saves_dir)?;
        }

        // Save the new scene to the standard location
        let save_path = saves_dir.join("scene.bin");
        let camera_data = Some((
            self.player_controller.position,
            self.player_controller.yaw,
            self.player_controller.pitch,
        ));
        self.scene
            .save_to_file(&save_path, &self.world, camera_data)?;
        log::info!("Saved new scene to {:?}", save_path);

        // Request a redraw to show the new scene
        if let Some(ref wr) = self.window_renderer {
            wr.window.request_redraw();
        }

        // Switch to game mode and hide menu
        self.mode = AppMode::Game;
        self.hide_menu();

        log::info!("New world generation complete - switched to game mode");
        Ok(())
    }

    fn auto_save_on_shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        log::info!("Auto-saving on shutdown...");

        // Ensure saves directory exists
        let saves_dir = std::path::Path::new("saves");
        if !saves_dir.exists() {
            std::fs::create_dir_all(saves_dir)?;
        }

        // Save the current scene with camera data
        let save_path = saves_dir.join("scene.bin");
        let camera_data = Some((
            self.player_controller.position,
            self.player_controller.yaw,
            self.player_controller.pitch,
        ));

        self.scene
            .save_to_file(&save_path, &self.world, camera_data)?;
        log::info!("Auto-saved scene to {:?}", save_path);

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

        // Load the scene from file
        let camera_data = self.scene.load_from_file(&path, &mut self.world)?;
        log::info!("Scene loaded successfully from {:?}", path.as_ref());

        // Restore camera position if available
        if let Some((position, yaw, pitch)) = camera_data {
            self.player_controller.position = position;
            self.player_controller.yaw = yaw;
            self.player_controller.pitch = pitch;
            log::info!(
                "Restored camera position: {:?}, yaw: {:.2}, pitch: {:.2}",
                position,
                yaw,
                pitch
            );
        } else {
            log::info!("No camera data found in scene file, keeping current position");
        }

        // Request a redraw to show the loaded scene
        if let Some(ref wr) = self.window_renderer {
            wr.window.request_redraw();
        }

        // Switch to game mode and hide menu
        self.mode = AppMode::Game;
        self.hide_menu();

        log::info!("Scene loading complete - switched to game mode");
        Ok(())
    }

    /// Update controller input from keyboard state
    fn update_controller_input(&mut self) {
        // Calculate forward/backward
        let forward = if self.key_w { 1.0 } else { 0.0 } - if self.key_s { 1.0 } else { 0.0 };

        // Calculate left/right (A is left, so negative)
        let right = if self.key_d { 1.0 } else { 0.0 } - if self.key_a { 1.0 } else { 0.0 };

        // Calculate up/down (Space is up, Shift is down) - only in first person mode
        let up = if self.player_controller.camera_mode
            == engine_core::controller::CameraMode::FirstPerson
        {
            (if self.key_space { 1.0 } else { 0.0 }) - (if self.key_shift { 1.0 } else { 0.0 })
        } else {
            0.0 // No up/down in isometric mode
        };

        self.controller_input.forward = forward;
        self.controller_input.right = right;
        self.controller_input.up = up;
    }

    /// Handle keyboard input for camera controls
    fn handle_keyboard_input(&mut self, event: &KeyEvent) {
        // Only process input in game mode
        if self.mode != AppMode::Game {
            return;
        }

        let pressed = event.state == ElementState::Pressed;

        if let PhysicalKey::Code(keycode) = event.physical_key {
            match keycode {
                KeyCode::KeyW => self.key_w = pressed,
                KeyCode::KeyA => self.key_a = pressed,
                KeyCode::KeyS => self.key_s = pressed,
                KeyCode::KeyD => self.key_d = pressed,
                KeyCode::Space => self.key_space = pressed,
                KeyCode::ShiftLeft | KeyCode::ShiftRight => self.key_shift = pressed,
                KeyCode::Escape => {
                    // ESC to show menu
                    if pressed {
                        self.show_menu();
                    }
                }
                KeyCode::Tab => {
                    if pressed {
                        // Toggle camera mode
                        self.player_controller.camera_mode =
                            match self.player_controller.camera_mode {
                                engine_core::controller::CameraMode::FirstPerson => {
                                    engine_core::controller::CameraMode::Isometric
                                }
                                engine_core::controller::CameraMode::Isometric => {
                                    engine_core::controller::CameraMode::FirstPerson
                                }
                            };
                        log::info!(
                            "Switched to camera mode: {:?}",
                            self.player_controller.camera_mode
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// Handle mouse motion for camera look
    fn handle_mouse_motion(&mut self, delta: (f64, f64)) {
        // Only process input in game mode and first person camera mode
        if self.mode != AppMode::Game
            || self.player_controller.camera_mode
                != engine_core::controller::CameraMode::FirstPerson
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
        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            // Set UI to not visible directly
            adapter.ui_visible = false;

            // Update the atomic flag
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(false, std::sync::atomic::Ordering::SeqCst);

            log::info!("Menu hidden - UI set to invisible");
        }

        // Grab cursor for game mode
        self.grab_cursor();
    }

    fn show_menu(&mut self) {
        self.mode = AppMode::Menu;

        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut adapter) = ui_adapter.lock()
        {
            // Set UI to visible
            adapter.ui_visible = true;

            // Update the atomic flag
            use moho_ui::UI_OVERLAY_VISIBLE;
            UI_OVERLAY_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);

            log::info!("Menu shown - UI set to visible");
        }

        // Release cursor for menu mode
        self.release_cursor();
    }

    /// Handle audio events from the UI or game
    fn handle_audio_event(&mut self, event: engine_audio::AudioEvent) {
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
            let window_attributes = WindowAttributes::default();
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

            // Update camera controls if in game mode
            if self.mode == AppMode::Game {
                // Update controller input from keyboard state
                self.update_controller_input();

                let (yaw_delta, pitch_delta) = self.input_system.sample_frame_input();
                self.controller_input.yaw_delta = yaw_delta;
                self.controller_input.pitch_delta = pitch_delta;

                // Apply input to player controller
                self.player_controller
                    .apply_input(&self.controller_input, dt);

                // Update camera from controller
                self.camera =
                    engine_core::controller::controller_to_camera(&self.player_controller);
            }

            if let Some(ref wr) = self.window_renderer {
                wr.window.request_redraw();
            }

            // Process UI events - collect events first to avoid borrowing conflicts
            #[cfg(feature = "ui-egui")]
            {
                use moho_ui::UiEvent;
                let mut events = Vec::new();
                if let Some(ui_receiver) = &self.ui_receiver {
                    while let Ok(ev) = ui_receiver.try_recv() {
                        events.push(ev);
                    }
                }

                // Process collected events
                for ev in events {
                    match ev {
                        UiEvent::LoadScene(path) => {
                            log::info!("UI requested load scene: {:?}", path);
                            if let Err(e) = self.load_scene(&path) {
                                log::error!("Failed to load scene from {:?}: {}", path, e);
                            }
                        }
                        UiEvent::NewWorld => {
                            log::info!("UI requested NewWorld");
                            if let Err(e) = self.generate_new_world() {
                                log::error!("Failed to generate new world: {}", e);
                            }
                        }
                        UiEvent::Exit => {
                            log::info!("UI requested exit");
                            // Auto-save before exit
                            if let Err(e) = self.auto_save_on_shutdown() {
                                log::warn!("Failed to auto-save on exit: {}", e);
                            }
                            event_loop.exit();
                        }
                        UiEvent::ShowMenu(name) => {
                            log::info!("UI requested ShowMenu: {}", name);
                            #[cfg(feature = "ui-egui")]
                            if let Some(ui_adapter) = &self.ui_adapter
                                && let Ok(mut adapter) = ui_adapter.lock()
                            {
                                adapter.show_menu(&name);
                            }
                        }
                        UiEvent::OverlayToggled(visible) => {
                            log::info!("Overlay visibility -> {}", visible);
                            if visible && let Some(ref wr) = self.window_renderer {
                                wr.window.set_cursor_visible(true);
                            }
                        }
                        UiEvent::SettingsSaved(prefs) => {
                            log::info!(
                                "Settings saved - applying mouse sensitivity: {} | filtering enabled: {}",
                                prefs.mouse_sensitivity,
                                prefs.input_filtering_enabled
                            );

                            // Scale the UI range (0.01-10.0) to radians per pixel
                            self.mouse_sensitivity = prefs.mouse_sensitivity * 0.002;
                            self.input_system.set_sensitivity(self.mouse_sensitivity);

                            // Always use default filter preset, only apply filtering enabled state
                            self.input_system
                                .set_filter_enabled(prefs.input_filtering_enabled);
                        }
                        UiEvent::AudioEvent(audio_event) => {
                            // Convert UI audio event to engine audio event
                            let engine_event = match audio_event {
                                moho_ui::UiAudioEvent::ButtonClick => {
                                    engine_audio::AudioEvent::ButtonClick
                                }
                                moho_ui::UiAudioEvent::MenuNavigate => {
                                    engine_audio::AudioEvent::MenuNavigate
                                }
                                moho_ui::UiAudioEvent::Confirm => engine_audio::AudioEvent::Confirm,
                                moho_ui::UiAudioEvent::Cancel => engine_audio::AudioEvent::Cancel,
                                moho_ui::UiAudioEvent::Error => engine_audio::AudioEvent::Error,
                            };
                            self.handle_audio_event(engine_event);
                        }
                    }
                }
            }

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
        // Forward input events to UI adapter when in Menu mode
        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter {
            use winit::event::WindowEvent as WEvent;
            match &event {
                // Always forward mouse events
                WEvent::CursorMoved { .. }
                | WEvent::MouseInput { .. }
                | WEvent::ModifiersChanged(_) => {
                    if let Ok(mut a) = ui_adapter.lock() {
                        a.handle_winit_event(&event);
                    }
                }
                // Forward keyboard events ONLY in Menu mode
                WEvent::KeyboardInput { .. } if self.mode == AppMode::Menu => {
                    if let Ok(mut a) = ui_adapter.lock() {
                        a.handle_winit_event(&event);
                    }
                }
                _ => {}
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
