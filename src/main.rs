use rand::{rng, Rng};
use glam::Vec3;
use moho_core::actors::Sphere;
use moho_core::materials::MaterialType;
use moho_core::vector_length;
use legion::World;

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

    println!("Hello, world!");
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
