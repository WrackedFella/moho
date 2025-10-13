use legion::World;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

// Combined state to handle lifetimes properly
struct WindowRenderer {
    window: Arc<Window>,
    renderer: Box<dyn engine_renderer::RendererBackend>,
    mesh_handle: u32,
    cube_mesh_handle: u32,
}

// App state to manage different modes
#[derive(Clone, Copy, Debug, PartialEq)]
enum AppMode {
    Menu,
    Game,
}

// Application state structure that implements ApplicationHandler
struct App {
    world: World,
    scene: engine_renderer::Scene,
    camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    mode: AppMode,

    // Runtime state (initialized after window creation)
    window_renderer: Option<WindowRenderer>,

    // UI components
    #[cfg(feature = "ui-egui")]
    ui_adapter: Option<Arc<Mutex<moho_ui::EguiAdapter>>>,
    #[cfg(feature = "ui-egui")]
    ui_receiver: Option<moho_ui::UiReceiver>,

    // Frame timing
    frame_duration: Duration,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        // Initialize logging
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
        log::info!("Starting minimal menu test");

        // Create basic world and scene (minimal setup)
        let world = World::default();
        let scene = engine_renderer::Scene::new();

        // Basic camera - positioned to get a good view of the random scene
        let camera = {
            let eye = glam::Vec3::new(13.0, 2.0, 3.0);
            let center = glam::Vec3::new(0.0, 0.0, 0.0);
            let up = glam::Vec3::new(0.0, 1.0, 0.0);
            let view = glam::Mat4::look_at_rh(eye, center, up);
            let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
            (view, proj, eye)
        };

        Self {
            world,
            scene,
            camera,
            mode: AppMode::Menu, // Start in menu mode
            window_renderer: None,

            #[cfg(feature = "ui-egui")]
            ui_adapter: None,
            #[cfg(feature = "ui-egui")]
            ui_receiver: None,

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

        // Generate new random scene
        engine_core::scene_builders::random_scene(&mut self.world);

        // Ensure saves directory exists
        let saves_dir = std::path::Path::new("saves");
        if !saves_dir.exists() {
            std::fs::create_dir_all(saves_dir)?;
        }

        // Save the new scene to the standard location
        let save_path = saves_dir.join("scene.bin");
        self.scene.save_to_file(&save_path, &self.world)?;
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
        self.scene.load_from_file(&path, &mut self.world)?;
        log::info!("Scene loaded successfully from {:?}", path.as_ref());

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

        // Hide cursor when in game mode
        if let Some(ref wr) = self.window_renderer {
            wr.window.set_cursor_visible(false);
        }
    }

    #[allow(dead_code)]
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

        // Show cursor when in menu mode
        if let Some(ref wr) = self.window_renderer {
            wr.window.set_cursor_visible(true);
        }
    }
}

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
                            &self.world,
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
            self.last_frame += self.frame_duration;

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
                            event_loop.exit();
                        }
                        UiEvent::ShowMenu(name) => {
                            log::info!("UI requested ShowMenu: {}", name);
                        }
                        UiEvent::OverlayToggled(visible) => {
                            log::info!("Overlay visibility -> {}", visible);
                            if visible && let Some(ref wr) = self.window_renderer {
                                wr.window.set_cursor_visible(true);
                            }
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
        // Forward ONLY mouse events to UI - NO KEYBOARD
        #[cfg(feature = "ui-egui")]
        if let Some(ui_adapter) = &self.ui_adapter {
            use winit::event::WindowEvent as WEvent;
            match &event {
                WEvent::CursorMoved { .. }
                | WEvent::MouseInput { .. }
                | WEvent::ModifiersChanged(_) => {
                    if let Ok(mut a) = ui_adapter.lock() {
                        a.handle_winit_event(&event);
                    }
                }
                _ => {}
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut wr) = self.window_renderer {
                    wr.renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                log::debug!("RedrawRequested - rendering frame");
                if let Some(ref mut wr) = self.window_renderer {
                    self.scene.render(
                        &mut *wr.renderer,
                        &self.world,
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
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::new();

    // Run the modern event loop with ApplicationHandler
    let _ = event_loop.run_app(&mut app);
}
