use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::{Event, StartCause, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowAttributes;
use legion::World;

fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Starting minimal menu test");

    // Create basic world and scene (minimal setup)
    let mut world = World::default();
    let scene = engine_renderer::Scene::new();

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
    let (window, mut renderer, _mesh_handle, _cube_mesh_handle) = {
        let window = Arc::new(event_loop.create_window(window_attributes).expect("Failed to create window"));
        let mut renderer = engine_renderer::create_renderer(window.clone()).expect("Failed to create renderer");
        let mesh_handle = renderer.create_mesh_placeholder();
        let cube_mesh_handle = renderer.create_cube_mesh();
        (window, renderer, mesh_handle, cube_mesh_handle)
    };

    // UI setup - this is the critical part
    #[cfg(feature = "ui-egui")]
    let (ui_adapter, ui_receiver) = {
        let (adapter, receiver) = moho_ui::EguiUi::new(Some(window.clone()));
        (Arc::new(Mutex::new(adapter)), receiver)
    };

    // Frame callback setup for UI
    #[cfg(feature = "ui-egui")]
    {
        if let Ok(mut a) = ui_adapter.lock() {
            a.set_surface_format(wgpu::TextureFormat::Bgra8UnormSrgb.into());
        }
        renderer.set_frame_callback_arc(Some(ui_adapter.clone() 
            as Arc<Mutex<dyn engine_renderer::FrameCallback>>));
    }

    // Frame timing
    let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
    let mut last_frame = Instant::now();

    // Run the event loop - MINIMAL VERSION
    let _ = event_loop.run(move |event, active_event_loop| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                log::info!("Initial render");
                scene.render(&mut renderer, &world, _mesh_handle, _cube_mesh_handle, camera);
                window.request_redraw();
            }

            Event::NewEvents(_) => {
                let now = Instant::now();
                if now >= last_frame + frame_duration {
                    last_frame += frame_duration;
                    window.request_redraw();

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
                                        window.set_cursor_visible(true);
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
                        renderer.resize(size.width, size.height);
                    }
                    WindowEvent::RedrawRequested => {
                        log::debug!("RedrawRequested - rendering frame");
                        scene.render(&mut renderer, &world, _mesh_handle, _cube_mesh_handle, camera);
                        
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