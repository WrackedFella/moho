extern crate glam;
extern crate rand;
extern crate raytracing;

use glam::Vec3;
use rand::{rng, Rng};
use std::fs::File;
use std::io::prelude::*;

use raytracing::actors::Sphere;
use raytracing::camera::Camera;
use raytracing::materials::MaterialType;
use raytracing::*;

fn color(mut r: Ray, world: &[Box<dyn Hittable>], depth: i16) -> Vec3 {
    let mut rec = HittableRecord {
        t: 0f32,
        p: Vec3::new(0f32, 0f32, 0f32),
        normal: Vec3::new(0f32, 0f32, 0f32),
        mat_ptr: MaterialType::Lambertian {
            albedo: Vec3::new(0f32, 0f32, 0f32),
        },
    };
    let mut hit_anything: bool = false;
    let mut closest_so_far: f32 = std::f32::MAX;

    world.iter().for_each(|h| {
        let hit_test = h.hit(r, 0.001f32, closest_so_far);
        match hit_test {
            Some(x) => {
                hit_anything = true;
                closest_so_far = x.t;
                rec = x;
            }
            None => {}
        };
    });
    if hit_anything {
        let scatter_test = rec.mat_ptr.scatter(r, rec);
        match scatter_test {
            Some(x) => {
                if depth < 500 {
                    return multiply_vectors(x.attenuation, color(x.scattered, world, depth + 1));
                }
            }
            None => {}
        }
        return Vec3::new(0f32, 0f32, 0f32);
    } else {
        let unit_direction: Vec3 = unit_vector(r.direction());
        let t: f32 = 0.5f32 * (unit_direction.y + 1f32);
        return (1.0f32 - t) * Vec3::new(1.0f32, 1.0f32, 1.0f32)
            + t * Vec3::new(0.5f32, 0.7f32, 1.0f32);
    }
}

fn random_scene() -> Vec<Box<dyn Hittable>> {
    let mut rng = rng();
    let mut world: Vec<Box<dyn Hittable>> = Vec::new();
    let sphere = Sphere::new(
        Vec3::new(0f32, -1000f32, 0f32).to_owned(),
        1000f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(0.5f32, 0.5f32, 0.5f32),
        },
    );
    world.push(Box::new(sphere));
    for a in -11..11 {
        for b in -11..11 {
            let choose_mat = rng.random::<f32>();
            let center = Vec3::new(
                a as f32 + 0.9f32 * rng.random::<f32>(),
                0.2f32,
                b as f32 + 0.9f32 * rng.random::<f32>(),
            );
            if vector_length(center - Vec3::new(4f32, 0.2f32, 0f32)) > 0.9f32 {
                if choose_mat < 0.8f32 {
                    // diffuse
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Lambertian {
                            albedo: Vec3::new(
                                rng.random::<f32>() * rng.random::<f32>(),
                                rng.random::<f32>() * rng.random::<f32>(),
                                rng.random::<f32>() * rng.random::<f32>(),
                            ),
                        },
                    );
                    world.push(Box::new(sphere));
                } else if choose_mat < 0.95f32 {
                    // metal
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Metal {
                            albedo: Vec3::new(
                                0.5f32 * (1f32 + rng.random::<f32>()),
                                0.5f32 * (1f32 + rng.random::<f32>()),
                                0.5f32 * (1f32 + rng.random::<f32>()),
                            ),
                            fuzz: 0.5f32 * rng.random::<f32>(),
                        },
                    );
                    world.push(Box::new(sphere));
                } else {
                    // glass
                    let sphere = Sphere::new(
                        center,
                        0.2f32,
                        MaterialType::Dielectric { ref_indx: 1.5f32 },
                    );
                    world.push(Box::new(sphere));
                }
            }
        }
    }
    world.push(Box::new(Sphere::new(
        Vec3::new(0f32, 1f32, 0f32),
        1f32,
        MaterialType::Dielectric { ref_indx: 1.5f32 },
    )));
    world.push(Box::new(Sphere::new(
        Vec3::new(-4f32, 1f32, 0f32),
        1f32,
        MaterialType::Lambertian {
            albedo: Vec3::new(0.4f32, 0.2f32, 0.1f32),
        },
    )));
    world.push(Box::new(Sphere::new(
        Vec3::new(4f32, 1f32, 0f32),
        1f32,
        MaterialType::Metal {
            albedo: Vec3::new(0.7f32, 0.6f32, 0.5f32),
            fuzz: 0.0f32,
        },
    )));
    println!("World Generated");
    return world;
}

fn main() -> std::io::Result<()> {
    let nx: u32 = 200;
    let ny: u32 = 100;
    let ns: u32 = 5;

    let mut file_contents = format!("P3\n{} {}\n255\n", nx, ny);
    let mut rng = rng();

    let world: Vec<Box<dyn Hittable>> = random_scene();

    let look_from = Vec3::new(13f32, 2f32, 3f32);
    let look_at = Vec3::new(0f32, 0f32, 0f32);
    let dist_to_focus = 10f32; // vector_length(look_from-look_at);
    let aperture = 0.1f32;
    let cam = Camera::new(
        look_from,
        look_at,
        Vec3::new(0f32, 1f32, 0f32),
        20f32,
        nx as f32 / ny as f32,
        aperture,
        dist_to_focus,
    );
    println!("Starting loops");
    for j in (1..(ny - 1)).rev() {
        for i in 0..nx {
            let mut col: Vec3 = Vec3::new(0f32, 0f32, 0f32);
            for _s in 0..ns {
                let u: f32 = (rng.random::<f32>() + i as f32) / nx as f32;
                let v: f32 = (rng.random::<f32>() + j as f32) / ny as f32;
                let mut r = cam.get_ray(u, v);
                let _p = r.point_at_parameter(2.0);
                col = col + color(r, &world, 0);
            }
            col = col / ns as f32;
            col = Vec3::new(col.x.sqrt(), col.y.sqrt(), col.z.sqrt());
            let ir = 255.99 * col.x;
            let ig = 255.99 * col.y;
            let ib = 255.99 * col.z;

            file_contents = file_contents + &format!("{} {} {}\n", ir, ig, ib);
        }
    }

    println!("Saving File");
    let mut file = File::create("output.ppm")?;
    file.write_all(file_contents.as_bytes())?;
    Ok(())
}
