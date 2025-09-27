extern crate glam;
extern crate rand;
use crate::materials::MaterialType;
use glam::Vec3;
use rand::{rng, Rng};

pub mod actors;
pub mod camera;
pub mod controller;
pub mod materials;
pub mod scene_builders;

// Reflection function
pub fn schlick(cosine: f32, ref_idx: f32) -> f32 {
    let mut r0 = (1f32 - ref_idx) / (1f32 + ref_idx);
    r0 = r0 * r0;
    r0 + (1f32 - r0) * (1f32 - cosine).powf(5f32)
}

pub fn refract(v: Vec3, n: Vec3, ni_over_nt: f32) -> Option<Vec3> {
    let uv = unit_vector(v);
    let dt = uv.dot(n);
    let discriminant = 1.0f32 - ni_over_nt * ni_over_nt * (1f32 - dt * dt);
    if discriminant > 0f32 {
        Some(ni_over_nt * (uv - n * dt) - n * (discriminant.sqrt()))
    } else {
        None
    }
}

pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - 2f32 * v.dot(n) * n
}

pub fn multiply_vectors(v1: Vec3, v2: Vec3) -> Vec3 {
    Vec3::new(v1.x * v2.x, v1.y * v2.y, v1.z * v2.z)
}

pub fn unit_vector(v: Vec3) -> Vec3 {
    v / vector_length(v)
}

pub fn vector_length_squared(v: Vec3) -> f32 {
    v.x * v.x + v.y * v.y + v.z * v.z
}

pub fn vector_length(v: Vec3) -> f32 {
    vector_length_squared(v).sqrt()
}

pub fn random_in_unit_sphere() -> Vec3 {
    let mut rng = rng();
    let mut p = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    while vector_length_squared(p) >= 1.0 {
        p =
            2f32 * Vec3::new(
                rng.random::<f32>(),
                rng.random::<f32>(),
                rng.random::<f32>(),
            ) - Vec3::new(1f32, 1f32, 1f32);
    }
    p
}

pub fn random_in_unit_disk() -> Vec3 {
    let mut rng = rng();
    loop {
        let p = 2f32 * Vec3::new(rng.random::<f32>(), rng.random::<f32>(), 0f32)
            - Vec3::new(1f32, 1f32, 0f32);
        if p.dot(p) < 1f32 {
            return p;
        }
    }
}

pub trait Hittable {
    fn hit(&self, r: Ray, t_min: f32, t_max: f32) -> Option<HittableRecord>;
}

pub trait Scatterable {
    fn scatter(&self, r_in: Ray, rec: HittableRecord) -> Option<ScatterRecord>;
}

#[derive(Copy, Clone)]
pub struct RefractRecord {
    pub attenuation: Vec3,
    pub scattered: Ray,
}

#[derive(Copy, Clone)]
pub struct ScatterRecord {
    pub attenuation: Vec3,
    pub scattered: Ray,
}

#[derive(Copy, Clone)]
pub struct Ray {
    a: Vec3,
    b: Vec3,
}

#[derive(Copy, Clone)]
pub struct HittableRecord {
    pub t: f32,
    pub p: Vec3,
    pub normal: Vec3,
    pub mat_ptr: MaterialType,
}

impl Ray {
    pub fn new(a_in: Vec3, b_in: Vec3) -> Ray {
        Ray { a: a_in, b: b_in }
    }

    pub fn origin(&self) -> Vec3 {
        self.a
    }
    pub fn direction(&self) -> Vec3 {
        self.b
    }

    pub fn point_at_parameter(&self, t: f32) -> Vec3 {
        self.a + t * self.b
    }
}
