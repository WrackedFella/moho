#[cfg(feature = "backend-wgpu")]
use engine_core::actors::Cube;
use engine_core::actors::Sphere;
use engine_core::scene_builders::random_scene;
// glam types are used via fully-qualified names where needed
// Renderer has been moved int                                            app_state.window.set_cursor_visible(true); the `engine_renderer` crate to provide a
// reusable rendering API.
#[cfg(feature = "backend-wgpu")]
use engine_renderer::create_renderer;
use legion::World;
use legion::query::IntoQuery;

// `winit` is an optional dependency used by the GPU backends. Guard its
// usage behind the `backend-wgpu` feature. When the backend is disabled we
// fall back to a single-frame placeholder renderer to keep builds fast.
#[cfg(feature = "backend-wgpu")]
use std::time::{Duration, Instant};
#[cfg(feature = "backend-wgpu")]
use winit::event::{Event, StartCause, WindowEvent};
#[cfg(feature = "backend-wgpu")]
use winit::event_loop::{ControlFlow, EventLoop};
#[cfg(feature = "backend-wgpu")]
use winit::window::WindowAttributes;

// a component is any type that is 'static, sized, send and sync
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

fn main() {
    let mut world = World::default();
    // Parse CLI args for --scene <path>. Default to ./scene.bin
    let args: Vec<String> = std::env::args().collect();
    let scene_path_buf = if let Some(i) = args.iter().position(|a| a == "--scene" || a == "-s") {
        args.get(i + 1)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("scene.bin"))
    } else {
        std::path::PathBuf::from("scene.bin")
    };

    // Create the Scene manager once and try to load the persisted scene.
    let mut scene = engine_renderer::Scene::new();
    load_or_generate_scene(&mut scene, &mut world, &scene_path_buf);

    // create a simple PlayerController entity and derive the camera from it
    let start_pos = glam::Vec3::new(13.0, 2.0, 3.0);
    let mut controller = engine_core::controller::PlayerController::new(start_pos);
    // Set initial yaw and pitch to look towards the origin
    // direction = (-13, -2, -3), normalized ≈ (-0.94, -0.14, -0.22)
    // yaw = atan2(-0.94, -0.22) ≈ -1.76, pitch = asin(-0.14) ≈ -0.14
    controller.yaw = -1.76;
    controller.pitch = -0.14;
    world.push((
        controller,
        engine_core::controller::ControllerInput::default(),
    ));
    // initial camera value (will be computed from controller each frame)
    #[allow(unused_mut)]
    let mut camera = make_camera();

    // Helper to make the camera clearer - kept local for now.
    fn make_camera() -> (glam::Mat4, glam::Mat4, glam::Vec3) {
        let eye = glam::Vec3::new(13.0, 2.0, 3.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
        (view, proj, eye)
    }

    // If `backend-wgpu` (and therefore `winit`) is enabled, create the event loop
    // and run a proper per-frame loop. Otherwise use the placeholder
    // renderer and run a single frame to keep builds fast.
    #[cfg(feature = "backend-wgpu")]
    #[allow(deprecated)]
    {
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        let window_attributes = WindowAttributes::default();

        // Create the window on the main thread then move ownership into the renderer.
        // The renderer now owns the Window and exposes control methods
        // (request_redraw, set_cursor_visible, set_cursor_grab).
        #[allow(deprecated)]
        let window = event_loop
            .create_window(window_attributes.clone())
            .expect("Failed to create window");
        window.set_title("moho - wgpu renderer");

        // Wrap the window in an Arc so we can share ownership between the
        // renderer and the event loop without unsafe or leaking memory.
        use std::sync::Arc;
        let arc_window = Arc::new(window);

        let mut renderer = create_renderer(Some(&*arc_window));

        // Register meshes up-front.
        let (vertices, normals, indices) = collect_indexed_vertices(&mut world);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);
        let (cube_vertices, cube_normals, cube_indices) = Cube::unit_cube_indexed();
        let cube_mesh_handle =
            renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

        struct AppState<'a> {
            renderer: Box<dyn engine_renderer::RendererBackend + 'a>,
            mesh_handle: u32,
            cube_mesh_handle: u32,
            cursor_grabbed: bool,
            // keep the Window so event handling can manipulate cursor
            window: std::sync::Arc<winit::window::Window>,
        }

        let mut app_state = AppState {
            renderer,
            mesh_handle,
            cube_mesh_handle,
            cursor_grabbed: false,
            window: Arc::clone(&arc_window),
        };
        // use winit::window::CursorGrabMode;

        // Frame timing: aim for ~60 FPS.
        let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
        let mut last_frame = Instant::now();
        // Simulation timestep (fixed) and accumulator for decoupled updates.
        let sim_dt = frame_duration; // simulation uses the same fixed timestep by default
        let mut sim_acc = Duration::from_secs(0);
        // Track wall-clock time to accumulate simulation time.
        let mut last_time = Instant::now();
        // simple frame counter for debug correlation with renderer logs
        let mut frame_count: u64 = 0;

        // Run the event loop. The leaked window will remain valid for the
        // duration of the event loop. It will be dropped when the process
        // terminates (acceptable for this demo app) or can be handled more
        // explicitly if desired.
        let _ = event_loop.run(move |event, active_event_loop| {
            match event {
                Event::NewEvents(start_cause) => {
                    if matches!(start_cause, StartCause::Init) {
                        // On Init, perform an initial render pass so the window
                        // displays content immediately.
                        scene.render(
                            &mut *app_state.renderer,
                            &world,
                            app_state.mesh_handle,
                            app_state.cube_mesh_handle,
                            camera,
                        );

                        active_event_loop.set_control_flow(ControlFlow::Wait);
                    } else if matches!(
                        start_cause,
                        StartCause::ResumeTimeReached { .. } | StartCause::Poll
                    ) {
                        // This is called when the event loop wakes at our scheduled time.
                        // Mirror the previous MainEventsCleared behavior: step simulation and
                        // request a redraw when appropriate.
                        let now = Instant::now();
                        let elapsed = now - last_time;
                        last_time = now;

                        // Accumulate time for simulation updates.
                        sim_acc += elapsed;
                        let mut sim_steps = 0;
                        while sim_acc >= sim_dt && sim_steps < 20 {
                            // Advance simulation by exactly sim_dt.
                            simulate(&mut world, sim_dt);
                            sim_acc -= sim_dt;
                            sim_steps += 1;
                        }

                        // Now handle rendering on the fixed render cadence.
                        if now >= last_frame + frame_duration {
                            // Advance the render timestamp by one fixed frame.
                            last_frame += frame_duration;
                            // Request a redraw via the Window (renderer.request_redraw is a no-op)
                            app_state.window.request_redraw();
                            // Schedule next wake at the next fixed step boundary.
                            let next = last_frame + frame_duration;
                            active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                        } else {
                            let next = last_frame + frame_duration;
                            active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                        }
                    }
                }
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => {
                            active_event_loop.exit();
                        }
                        WindowEvent::Resized(size) => {
                            app_state.renderer.resize(size.width, size.height);
                        }
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == winit::event::ElementState::Pressed {
                                let is_escape = event
                                    .text
                                    .as_deref()
                                    .map(|s| s == "\u{1b}")
                                    .unwrap_or(false)
                                    || format!("{:?}", event.logical_key).contains("Escape");

                                if is_escape {
                                    eprintln!(
                                        "Escape pressed; toggling cursor grab (currently={})",
                                        app_state.cursor_grabbed
                                    );
                                    if app_state.cursor_grabbed {
                                        let r = app_state
                                            .window
                                            .set_cursor_grab(winit::window::CursorGrabMode::None);
                                        eprintln!("window.set_cursor_grab(None) -> {:?}", r);
                                        app_state.cursor_grabbed = false;
                                        app_state.window.set_cursor_visible(true);
                                    } else {
                                        // Try Locked then Confined
                                        let r = app_state
                                            .window
                                            .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                            .or_else(|_| {
                                                app_state.window.set_cursor_grab(
                                                    winit::window::CursorGrabMode::Confined,
                                                )
                                            });
                                        eprintln!(
                                            "window.set_cursor_grab(Locked|Confined) -> {:?}",
                                            r
                                        );
                                        if r.is_ok() {
                                            app_state.cursor_grabbed = true;
                                            app_state.window.set_cursor_visible(false);
                                        }
                                    }
                                }
                            }
                        }
                        WindowEvent::Focused(gained) => {
                            if gained && !app_state.cursor_grabbed {
                                let r = app_state
                                    .window
                                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                    .or_else(|_| {
                                        app_state.window.set_cursor_grab(
                                            winit::window::CursorGrabMode::Confined,
                                        )
                                    });
                                eprintln!("Focused -> window.set_cursor_grab -> {:?}", r);
                                if r.is_ok() {
                                    app_state.cursor_grabbed = true;
                                    app_state.window.set_cursor_visible(false);
                                }
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            // NOTE: in winit 0.30 RedrawRequested may also arrive as a
                            // top-level Event::RedrawRequested(window_id). We still
                            // handle the WindowEvent form when present for
                            // completeness.
                            let mut qc = <&mut engine_core::controller::PlayerController>::query();
                            if let Some(pc) = qc.iter_mut(&mut world).next() {
                                camera = engine_core::controller::controller_to_camera(pc);
                            }
                            // Debug: print camera eye and frame id to correlate with renderer logs
                            eprintln!(
                                "[main] frame={} WindowEvent::RedrawRequested -> camera_eye={:?}",
                                frame_count, camera.2
                            );
                            scene.render(
                                &mut *app_state.renderer,
                                &world,
                                app_state.mesh_handle,
                                app_state.cube_mesh_handle,
                                camera,
                            );
                            frame_count = frame_count.wrapping_add(1);
                            active_event_loop.set_control_flow(ControlFlow::Wait);
                        }
                        _ => {}
                    }
                }
                Event::DeviceEvent {
                    event: winit::event::DeviceEvent::MouseMotion { delta },
                    ..
                } => {
                    // Only apply mouse motion when the cursor is grabbed for FPS look.
                    let cursor_is_grabbed = app_state.cursor_grabbed;
                    if !cursor_is_grabbed {
                        // ignore mouse motion when cursor is not grabbed
                    } else {
                        eprintln!("MouseMotion delta={:?}", delta);
                        // Apply mouse motion as small yaw/pitch deltas
                        let mut q = <&mut engine_core::controller::ControllerInput>::query();
                        if let Some(ci) = q.iter_mut(&mut world).next() {
                            let sensitivity = 0.0025f32;
                            ci.yaw_delta += -(delta.0 as f32) * sensitivity;
                            ci.pitch_delta += -(delta.1 as f32) * sensitivity;
                        }

                        // For debugging: run one simulation step immediately after
                        // applying controller input so we can observe the effect
                        // in the simulate() debug prints. This is temporary and
                        // helps verify that mouse deltas are being consumed.
                        simulate(&mut world, sim_dt);
                        // Ensure the renderer draws the updated camera state.
                        // Request a redraw via the owned window so the renderer
                        // will obtain an up-to-date SurfaceTexture during its
                        // next render call.
                        app_state.window.request_redraw();
                    }
                }
                // MainEventsCleared is no longer a top-level Event variant in winit 0.30.
                // Its previous responsibilities are handled via StartCause::ResumeTimeReached
                // in the NewEvents branch above.
                _ => {}
            }
        });

        // event_loop.run returned due to exit(); nothing special to do here
        // as the application retains ownership of the Window via Arc.
    }

    #[cfg(not(feature = "backend-wgpu"))]
    {
        let mut renderer = engine_renderer::create_renderer(None);
        // Register indexed mesh once with placeholder renderer (no-op) and use render_mesh
        let (vertices, normals, indices) = collect_indexed_vertices(&mut world);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);
        let instances = collect_instances(&mut world);
        renderer.render_mesh(mesh_handle, &instances, camera, true);
        println!("Rendered one frame (exiting).");
    }
}

fn load_or_generate_scene(
    scene: &mut engine_renderer::Scene,
    world: &mut World,
    scene_path: &std::path::Path,
) {
    if scene_path.exists() {
        match scene.load_from_file(scene_path, world) {
            Ok(_) => println!("Loaded scene from scene.bin"),
            Err(e) => {
                eprintln!("Failed to load scene.bin: {}. Generating new scene.", e);
                random_scene(world);
                if let Err(e) = scene.save_to_file(scene_path, world) {
                    eprintln!("Failed to save generated scene: {}", e);
                }
            }
        }
    } else {
        random_scene(world);
        if let Err(e) = scene.save_to_file(scene_path, world) {
            eprintln!("Failed to save generated scene: {}", e);
        }
    }
}

#[allow(dead_code)]
fn collect_instances(world: &mut World) -> Vec<engine_core::actors::InstanceGpu> {
    let mut out: Vec<engine_core::actors::InstanceGpu> = Vec::new();
    // Query all entities that have a Sphere component
    let mut q = <&Sphere>::query();
    for s in q.iter(world) {
        // Fallback helper used by the placeholder renderer path: assign
        // material index 0 for all instances.
        out.push(s.to_instance_with_material(0));
    }
    out
}

// `collect_vertices` was removed; the renderer uses indexed meshes via
// `collect_indexed_vertices` above.

fn collect_indexed_vertices(_world: &mut World) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    // Return an indexed unit-sphere mesh with normals.
    Sphere::unit_sphere_indexed(16, 16)
}

// Simple fixed-step simulation function. Advance the ECS world by `dt`.
// Currently this is a placeholder — it can be extended to run physics,
// animate entities, or mutate components. `dt` is provided as a Duration
// to match our fixed-step scheduling.
#[allow(dead_code)]
fn simulate(_world: &mut World, dt: std::time::Duration) {
    // Apply controller inputs to any PlayerController components.
    let seconds = dt.as_secs_f32();
    let mut q = <(
        &mut engine_core::controller::PlayerController,
        &mut engine_core::controller::ControllerInput,
    )>::query();
    use std::sync::atomic::{AtomicU64, Ordering};
    static SIM_COUNT: AtomicU64 = AtomicU64::new(0);
    for (pc, ci) in q.iter_mut(_world) {
        let n = SIM_COUNT.fetch_add(1, Ordering::Relaxed);
        if n.is_multiple_of(60) {
            eprintln!(
                "simulate[#{}]: before pc={:?} ci={:?} dt={}",
                n, *pc, *ci, seconds
            );
        }
        pc.apply_input(ci, seconds);
        if n.is_multiple_of(60) {
            eprintln!("simulate[#{}]: after  pc={:?} ci={:?}", n, *pc, *ci);
        }
        // yaw/pitch deltas are one-shot; clear after applying so they act per-frame
        ci.yaw_delta = 0.0;
        ci.pitch_delta = 0.0;
    }
}
