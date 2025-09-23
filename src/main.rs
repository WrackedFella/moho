use engine_core::actors::InstanceGpu;
use engine_core::actors::Sphere;
use engine_core::materials::MaterialType;
use engine_core::vector_length;
use glam::Vec3;
use rand::{Rng, rng};
mod gpu;
use legion::World;
use legion::query::IntoQuery;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// a component is any type that is 'static, sized, send and sync
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

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

    // Create the winit event loop and window here and drive the renderer
    // from `main`. This keeps platform-specific windowing on the main thread
    // and allows cross-platform input handling.
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("moho - vulkan renderer")
        .build(&event_loop)
        .expect("Failed to create window");

    // Create the GPU renderer (feature-gated). The non-vulkan placeholder
    // `Renderer` has a `new()` that does not require the window; the Vulkano
    // `Renderer::new` expects the `EventLoop` and `Window` so pass them along.
    #[cfg(feature = "vulkan")]
    let mut renderer = {
        // The Vulkano renderer consumes the `Window` so we moved it here.
        let r = gpu::Renderer::new(&event_loop, window);
        r
    };

    #[cfg(not(feature = "vulkan"))]
    let mut renderer = gpu::Renderer::new();

    // For now perform one frame of rendering to validate the renderer
    // integration and keep builds/tests fast. We'll replace this with a
    // proper winit event loop in a follow-up change.
    renderer.render(&mut world, camera);
    println!("Rendered one frame (exiting).");
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

fn collect_instances(world: &mut World) -> Vec<InstanceGpu> {
    let mut out: Vec<InstanceGpu> = Vec::new();
    // Query all entities that have a Sphere component
    let mut q = <&Sphere>::query();
    for s in q.iter(world) {
        out.push(s.to_instance());
    }
    out
}
