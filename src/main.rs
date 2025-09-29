#[cfg(feature = "backend-wgpu")]
use engine_core::actors::Cube;
use engine_core::actors::Sphere;
use engine_core::scene_builders::random_scene;
// glam types are used via fully-qualified names where needed
// Renderer has been moved into the `engine_renderer` crate to provide a
// reusable rendering API.
#[cfg(feature = "backend-wgpu")]
use engine_renderer::MaterialTable;
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
use winit::window::WindowBuilder;

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
    world.push((
        engine_core::controller::PlayerController::new(start_pos),
        engine_core::controller::ControllerInput::default(),
    ));
    // initial camera value (will be computed from controller each frame)
    let camera = make_camera();

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
    {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("moho - wgpu renderer")
            .build(&event_loop)
            .expect("Failed to create window");

        // Create a boxed renderer backend via the factory.
        let mut renderer = create_renderer(&event_loop, &window);
        let mut cursor_grabbed = false;
        use winit::window::CursorGrabMode;

        // Use the `scene` created above; it was intentionally created once
        // before entering the backend-specific code paths so persistence and
        // material state are shared.

        // Use `MaterialTable` methods for deduplication; the local helper was
        // removed because it was unused after consolidating material logic.

        // Register the indexed unit-sphere mesh once so we don't re-upload
        // vertex/index data each frame. Using an indexed mesh reduces vertex
        // duplication compared to the non-indexed generator. We also include
        // per-vertex normals for lighting.
        let (vertices, normals, indices) = collect_indexed_vertices(&mut world);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);

        // Also register the cube mesh so Cube instances can be rendered.
        let (cube_vertices, cube_normals, cube_indices) = Cube::unit_cube_indexed();
        let cube_mesh_handle =
            renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

        // Walk the world and build instances while populating the material table
        // indices used by instances.

        // Frame timing: aim for ~60 FPS.
        let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
        let mut last_frame = Instant::now();
        // Simulation timestep (fixed) and accumulator for decoupled updates.
        let sim_dt = frame_duration; // simulation uses the same fixed timestep by default
        let mut sim_acc = Duration::from_secs(0);
        // Track wall-clock time to accumulate simulation time.
        let mut last_time = Instant::now();

        // Run the winit event loop and render on-demand. Render once on init
        // and after each `WindowEvent::RedrawRequested` (if any external code
        // calls `window.request_redraw()`). We schedule the next frame using
        // `ControlFlow::WaitUntil` to sleep the event loop until it's time for
        // the next frame.
        event_loop.run(move |event, _event_loop_window_target, control_flow| {
            match event {
                Event::NewEvents(start_cause) => {
                    if matches!(start_cause, StartCause::Init) {
                        // Build material table, instances, and render the world via Scene
                        scene.render(
                            &mut *renderer,
                            &world,
                            mesh_handle,
                            cube_mesh_handle,
                            camera,
                        );
                        *control_flow = ControlFlow::Wait;
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    renderer.resize(size.width, size.height);
                }
                Event::WindowEvent {
                    event: WindowEvent::KeyboardInput { input, .. },
                    ..
                } => {
                    use winit::event::{ElementState, VirtualKeyCode};
                    if let Some(vk) = input.virtual_keycode {
                        let mut q = <&mut engine_core::controller::ControllerInput>::query();
                        if let Some(ci) = q.iter_mut(&mut world).next() {
                            match (vk, input.state) {
                                (VirtualKeyCode::Escape, ElementState::Pressed) => {
                                    // Toggle cursor grab/visibility
                                    cursor_grabbed = !cursor_grabbed;
                                    if cursor_grabbed {
                                        let _ = window
                                            .set_cursor_grab(CursorGrabMode::Locked)
                                            .or_else(|_| {
                                                window.set_cursor_grab(CursorGrabMode::Confined)
                                            });
                                        window.set_cursor_visible(false);
                                    } else {
                                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                                        window.set_cursor_visible(true);
                                    }
                                }
                                (VirtualKeyCode::W, ElementState::Pressed) => ci.forward = 1.0,
                                (VirtualKeyCode::W, ElementState::Released) => ci.forward = 0.0,
                                (VirtualKeyCode::S, ElementState::Pressed) => ci.forward = -1.0,
                                (VirtualKeyCode::S, ElementState::Released) => ci.forward = 0.0,
                                (VirtualKeyCode::D, ElementState::Pressed) => ci.right = 1.0,
                                (VirtualKeyCode::D, ElementState::Released) => ci.right = 0.0,
                                (VirtualKeyCode::A, ElementState::Pressed) => ci.right = -1.0,
                                (VirtualKeyCode::A, ElementState::Released) => ci.right = 0.0,
                                (VirtualKeyCode::Space, ElementState::Pressed) => ci.up = 1.0,
                                (VirtualKeyCode::Space, ElementState::Released) => ci.up = 0.0,
                                (VirtualKeyCode::LShift, ElementState::Pressed) => ci.up = -1.0,
                                (VirtualKeyCode::LShift, ElementState::Released) => ci.up = 0.0,
                                _ => {}
                            }
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::Focused(true),
                    ..
                } => {
                    // When the window gains focus, capture and hide the cursor
                    // if we are not already grabbed. This allows refocus to
                    // re-enable FPS mouse look.
                    if !cursor_grabbed {
                        cursor_grabbed = true;
                        let _ = window
                            .set_cursor_grab(CursorGrabMode::Locked)
                            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
                        window.set_cursor_visible(false);
                    }
                }
                Event::RedrawRequested(_window_id) => {
                    // Update camera from the first PlayerController component
                    let mut qc = <&mut engine_core::controller::PlayerController>::query();
                    if let Some(pc) = qc.iter_mut(&mut world).next() {
                        camera = engine_core::controller::controller_to_camera(pc);
                    }
                    scene.render(
                        &mut *renderer,
                        &world,
                        mesh_handle,
                        cube_mesh_handle,
                        camera,
                    );
                    *control_flow = ControlFlow::Wait;
                }
                Event::DeviceEvent {
                    event: winit::event::DeviceEvent::MouseMotion { delta },
                    ..
                } => {
                    // Only apply mouse motion when the cursor is grabbed for FPS look.
                    if cursor_grabbed {
                        // Apply mouse motion as small yaw/pitch deltas
                        let mut q = <&mut engine_core::controller::ControllerInput>::query();
                        if let Some(ci) = q.iter_mut(&mut world).next() {
                            let sensitivity = 0.0025f32;
                            ci.yaw_delta += -(delta.0 as f32) * sensitivity;
                            ci.pitch_delta += -(delta.1 as f32) * sensitivity;
                        }
                    }
                }

                Event::MainEventsCleared => {
                    // Decouple simulation (fixed-step) from rendering.
                    // 1) Accumulate wall-clock time into `sim_acc`.
                    // 2) Step simulation in fixed `sim_dt` increments (capped).
                    // 3) Render at the render cadence (`frame_duration`) by
                    //    requesting a redraw when the next render boundary is hit.
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
                        window.request_redraw();
                        // Schedule next wake at the next fixed step boundary.
                        let next = last_frame + frame_duration;
                        *control_flow = ControlFlow::WaitUntil(next);
                    } else {
                        let next = last_frame + frame_duration;
                        *control_flow = ControlFlow::WaitUntil(next);
                    }
                }
                _ => {}
            }
        });
    }

    #[cfg(not(feature = "backend-wgpu"))]
    {
        let mut renderer = create_renderer();
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
    for (pc, ci) in q.iter_mut(_world) {
        pc.apply_input(ci, seconds);
        // yaw/pitch deltas are one-shot; clear after applying so they act per-frame
        ci.yaw_delta = 0.0;
        ci.pitch_delta = 0.0;
    }
}
