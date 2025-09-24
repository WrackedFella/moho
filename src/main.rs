use engine_core::actors::InstanceGpu;
use engine_core::actors::Sphere;
use engine_core::materials::MaterialType;
use engine_core::vector_length;
use glam::Vec3;
use rand::{Rng, rng};
mod gpu;
use legion::World;
use legion::query::IntoQuery;

// `winit` is an optional dependency used by the GPU backends. Guard its
// usage behind the `backend-wgpu` feature. When the backend is disabled we
// fall back to a single-frame placeholder renderer to keep builds fast.
#[cfg(feature = "backend-wgpu")]
use winit::event::{Event, WindowEvent, StartCause};
#[cfg(feature = "backend-wgpu")]
use winit::event_loop::{ControlFlow, EventLoop};
#[cfg(feature = "backend-wgpu")]
use winit::window::WindowBuilder;
#[cfg(feature = "backend-wgpu")]
use std::time::{Instant, Duration};

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

    // create a simple perspective camera
    let camera = {
        let eye = glam::Vec3::new(13.0, 2.0, 3.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
        (view, proj)
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

    // The GPU renderer may own the `Window`; we keep a reference by moving
    // the `Window` into the renderer so it can call `request_redraw`.
    let mut renderer = gpu::Renderer::new(&event_loop, window);

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
        let _ = event_loop.run(move |event, _event_loop_window_target, control_flow| {
            match event {
                Event::NewEvents(start_cause) => {
                    if matches!(start_cause, StartCause::Init) {
                        renderer.render(&mut world, camera);
                        *control_flow = ControlFlow::Wait;
                    }
                }
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                    renderer.resize(size.width, size.height);
                }
                Event::RedrawRequested(_window_id) => {
                    renderer.render(&mut world, camera);
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
        let mut renderer = gpu::Renderer::new();
        renderer.render(&mut world, camera);
        println!("Rendered one frame (exiting).");
    }
}

fn random_scene(world: &mut World) {
    let sphere = Sphere::new(
        Vec3::new(0f32, -1000f32, 0f32),
        1000f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(0.5f32, 0.5f32, 0.5f32),
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
    world.push((Sphere::new(
        Vec3::new(0f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    ),));
    world.push((Sphere::new(
        Vec3::new(-4f32, 1f32, 0f32),
        1f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(0.4f32, 0.2f32, 0.1f32),
        },
    ),));
    world.push((Sphere::new(
        Vec3::new(4f32, 1f32, 0f32),
        1f32,
        MaterialType::Metal {
            albedo: Vec3::new(0.7f32, 0.6f32, 0.5f32),
            fuzz: 0.0f32,
        },
    ),));
    println!("World Generated");
}

#[allow(dead_code)]
fn collect_instances(world: &mut World) -> Vec<InstanceGpu> {
    let mut out: Vec<InstanceGpu> = Vec::new();
    // Query all entities that have a Sphere component
    let mut q = <&Sphere>::query();
    for s in q.iter(world) {
        out.push(s.to_instance());
    }
    out
}

// Simple fixed-step simulation function. Advance the ECS world by `dt`.
// Currently this is a placeholder — it can be extended to run physics,
// animate entities, or mutate components. `dt` is provided as a Duration
// to match our fixed-step scheduling.
fn simulate(_world: &mut World, dt: std::time::Duration) {
    // Example placeholder: You might iterate components and update positions
    // by velocities here. Keep minimal so builds remain fast.
    #[allow(unused_variables)]
    let _seconds = dt.as_secs_f32();
}
