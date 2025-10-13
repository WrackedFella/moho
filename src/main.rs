use legion::World;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::{Event, StartCause, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowAttributes;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting minimal menu test");

    // Create basic world and scene (minimal setup)
    let world = World::default();
    let mut scene = engine_renderer::Scene::new();

    // Basic camera
    let camera = {
        let eye = glam::Vec3::new(0.0, 0.0, 5.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
        (view, proj, eye)
    };

    // Event loop and window
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let window_attributes = WindowAttributes::default();

    // Window and renderer creation
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(window_attributes)
            .expect("Failed to create window"),
    );
    let mut renderer =
        engine_renderer::create_renderer(Some(&*window)).expect("Failed to create renderer");

    // Create minimal mesh data in correct format
    let vertices = vec![[0.0f32, 0.0f32, 0.0f32]]; // minimal vertex data
    let normals = vec![[0.0f32, 1.0f32, 0.0f32]]; // minimal normal data  
    let indices = vec![0u32]; // minimal index data
    let _mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);

    let (cube_vertices, cube_normals, cube_indices) =
        engine_core::actors::Cube::unit_cube_indexed();
    let _cube_mesh_handle =
        renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

    // Wrap renderer in AppState-like structure for consistency
    struct AppState<'a> {
        renderer: Box<dyn engine_renderer::RendererBackend + 'a>,
        mesh_handle: u32,
        cube_mesh_handle: u32,
        window: Arc<winit::window::Window>,
    }

    let mut app_state = AppState {
        renderer,
        mesh_handle: _mesh_handle,
        cube_mesh_handle: _cube_mesh_handle,
        window: window.clone(),
    };

    // UI setup - this is the critical part
    #[cfg(feature = "ui-egui")]
    let (ui_adapter, ui_receiver) = {
        let (adapter, receiver) = moho_ui::build_adapter(Some(window.clone()));
        (Arc::new(Mutex::new(adapter)), receiver)
    };

    // Frame callback setup for UI
    #[cfg(feature = "ui-egui")]
    {
        if let Ok(mut a) = ui_adapter.lock() {
            a.set_surface_format(wgpu::TextureFormat::Bgra8UnormSrgb);
        }
        app_state.renderer.set_frame_callback_arc(Some(
            ui_adapter.clone() as Arc<Mutex<dyn engine_renderer::FrameCallback>>
        ));
    }

    // Frame timing
    let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
    let mut last_frame = Instant::now();

    // Run the event loop - MINIMAL VERSION
    #[allow(deprecated)]
    let _ = event_loop.run(move |event, active_event_loop| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                log::info!("Initial render");
                scene.render(
                    &mut *app_state.renderer,
                    &world,
                    app_state.mesh_handle,
                    app_state.cube_mesh_handle,
                    camera,
                );
                app_state.window.request_redraw();
            }

            Event::NewEvents(_) => {
                let now = Instant::now();
                if now >= last_frame + frame_duration {
                    last_frame += frame_duration;
                    app_state.window.request_redraw();

                    // Process UI events
                    #[cfg(feature = "ui-egui")]
                    {
                        use moho_ui::UiEvent;
                        while let Ok(ev) = ui_receiver.try_recv() {
                            match ev {
                                UiEvent::LoadScene(path) => {
                                    log::info!("UI requested load scene: {:?}", path);
                                }
                                UiEvent::NewWorld => {
                                    log::info!("UI requested NewWorld");
                                }
                                UiEvent::Exit => {
                                    log::info!("UI requested exit");
                                    active_event_loop.exit();
                                }
                                UiEvent::ShowMenu(name) => {
                                    log::info!("UI requested ShowMenu: {}", name);
                                }
                                UiEvent::OverlayToggled(visible) => {
                                    log::info!("Overlay visibility -> {}", visible);
                                    if visible {
                                        app_state.window.set_cursor_visible(true);
                                    }
                                }
                            }
                        }
                    }

                    let next = last_frame + frame_duration;
                    active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                } else {
                    let next = last_frame + frame_duration;
                    active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                }
            }

            Event::WindowEvent { event, .. } => {
                // Forward ONLY mouse events to UI - NO KEYBOARD
                #[cfg(feature = "ui-egui")]
                {
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
                        active_event_loop.exit();
                    }
                    WindowEvent::Resized(size) => {
                        app_state.renderer.resize(size.width, size.height);
                    }
                    WindowEvent::RedrawRequested => {
                        log::debug!("RedrawRequested - rendering frame");
                        scene.render(
                            &mut *app_state.renderer,
                            &world,
                            app_state.mesh_handle,
                            app_state.cube_mesh_handle,
                            camera,
                        );

                        // Recall staging belt after render
                        #[cfg(feature = "ui-egui")]
                        {
                            if let Ok(mut a) = ui_adapter.lock() {
                                a.recall_staging_belt();
                            }
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    });
}
