use engine_core::actors::{Cube, Sphere};
use engine_core::materials::MaterialType;
use engine_core::vector_length;
use glam::Vec3;
use rand::{Rng, rng};
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
    // Populate the ECS world with spheres
    random_scene(&mut world);

    // create a simple perspective camera (we keep the camera world position
    // so the shader can compute view-dependent lighting)
    let camera = {
        let eye = glam::Vec3::new(13.0, 2.0, 3.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
        (view, proj, eye)
    };

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
        let mut renderer = create_renderer(&event_loop, window);

        // Build a compact material table using an incremental helper that
        // deduplicates materials using a hash map and only flags the table as
        // dirty when a new material is added. This lets us avoid re-uploading
        // the material storage buffer when nothing changed.
        let mut material_table = MaterialTable::new();

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
                        // Build material table, instances, and render the world.
                        engine_renderer::render_world(
                            &mut *renderer,
                            &world,
                            &mut material_table,
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
                Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                    renderer.resize(size.width, size.height);
                }
                Event::RedrawRequested(_window_id) => {
                    engine_renderer::render_world(
                        &mut *renderer,
                        &world,
                        &mut material_table,
                        mesh_handle,
                        cube_mesh_handle,
                        camera,
                    );
                    *control_flow = ControlFlow::Wait;
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
                        renderer.request_redraw();
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

fn random_scene(world: &mut World) {
    let sphere = Sphere::new(
        Vec3::new(0f32, -1000f32, 0f32),
        1000f32,
        MaterialType::Lambertian {
            // albedo: Vec3::new(0.5f32, 0.5f32, 0.5f32),
            albedo: Vec3::new(
                rng().random::<f32>() * rng().random::<f32>(),
                rng().random::<f32>() * rng().random::<f32>(),
                rng().random::<f32>() * rng().random::<f32>(),
            ),
        },
    );
    world.push((sphere,));
    for a in -11..11 {
        for b in -11..11 {
            let choose_mat = rng().random::<f32>();
            let center = Vec3::new(
                a as f32 + 0.9f32 * rng().random::<f32>(),
                0.2f32,
                b as f32 + 0.9f32 * rng().random::<f32>(),
            );
            if vector_length(center - Vec3::new(4f32, 0.2f32, 0f32)) > 0.9f32 {
                if choose_mat < 0.8f32 {
                    // diffuse
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Lambertian {
                            albedo: Vec3::new(
                                rng().random::<f32>() * rng().random::<f32>(),
                                rng().random::<f32>() * rng().random::<f32>(),
                                rng().random::<f32>() * rng().random::<f32>(),
                            ),
                        },
                    );
                    world.push((sphere,));
                } else if choose_mat < 0.95f32 {
                    // metal
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Metal {
                            albedo: Vec3::new(
                                0.5f32 * (1f32 + rng().random::<f32>()),
                                0.5f32 * (1f32 + rng().random::<f32>()),
                                0.5f32 * (1f32 + rng().random::<f32>()),
                            ),
                            fuzz: 0.5f32 * rng().random::<f32>(),
                        },
                    );
                    world.push((sphere,));
                } else {
                    // glass
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Dielectric { ref_indx: 1.5f32 },
                    );
                    world.push((sphere,));
                }
            }
        }
    }
    world.push((Cube::new(
        Vec3::new(0f32, 1f32, 0f32),
        1f32,
        1f32,
        1f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(
                rng().random::<f32>() * rng().random::<f32>(),
                rng().random::<f32>() * rng().random::<f32>(),
                rng().random::<f32>() * rng().random::<f32>(),
            ),
        },
    ),));
    world.push((Sphere::new(
        Vec3::new(4f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    ),));
    world.push((Sphere::new(
        Vec3::new(8f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    ),));
    // world.push((Sphere::new(
    //     Vec3::new(-4f32, 1f32, 0f32),
    //     1f32,
    //     MaterialType::Lambertian {
    //         albedo: Vec3::new(0.4f32, 0.2f32, 0.1f32),
    //     },
    // ),));
    
    println!("World Generated");
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
    // Example placeholder: You might iterate components and update positions
    // by velocities here. Keep minimal so builds remain fast.
    #[allow(unused_variables)]
    let _seconds = dt.as_secs_f32();
}
