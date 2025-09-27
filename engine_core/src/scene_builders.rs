use glam::Vec3;
use legion::World;

use crate::actors::{Cube, Sphere};
use crate::materials::MaterialType;
use crate::vector_length;
use rand::{rng, Rng};

/// Simple random scene generator used for testing and demos.
/// Moved out of `main.rs` to keep application code minimal.
pub fn random_scene(world: &mut World) {
    let mut rng_local = rng();
    let sphere = Sphere::new(
        Vec3::new(0f32, -1000f32, 0f32),
        1000f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
            ),
        },
    );
    world.push((sphere,));
    for a in -11..11 {
        for b in -11..11 {
            let choose_mat = rng_local.random::<f32>();
            let center = Vec3::new(
                a as f32 + 0.9f32 * rng_local.random::<f32>(),
                0.2f32,
                b as f32 + 0.9f32 * rng_local.random::<f32>(),
            );
            if vector_length(center - Vec3::new(4f32, 0.2f32, 0f32)) > 0.9f32 {
                if choose_mat < 0.8f32 {
                    // diffuse
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Lambertian {
                            albedo: Vec3::new(
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
                                rng_local.random::<f32>() * rng_local.random::<f32>(),
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
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                                0.5f32 * (1f32 + rng_local.random::<f32>()),
                            ),
                            fuzz: 0.5f32 * rng_local.random::<f32>(),
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
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
                rng_local.random::<f32>() * rng_local.random::<f32>(),
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

    println!("World Generated");
}
