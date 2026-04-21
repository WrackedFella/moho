//! Core systems and primitives for the Moho game engine.
//!
//! Provides the foundational building blocks shared across the engine:
//!
//! - **[`events`]** — Pub/sub event bus for inter-system communication
//! - **[`voxel`]** — Voxel grid, chunk management, and mesh generation
//! - **[`controller`]** — First-person / isometric player controller
//! - **[`camera`]** — Ray-tracing camera (legacy)
//! - **[`game_clock`]** — Day/night cycle with celestial body tracking
//! - **[`actors`]** — Primitive scene objects (sphere, cube, custom mesh)
//! - **[`materials`]** — Ray-tracing shading models (Lambertian, Metal, Dielectric)
//! - **[`raycast`]** — Voxel grid raycasting utility
//! - **[`input`]** — Low-level input collection

use crate::materials::MaterialType;
use glam::Vec3;
use rand::{Rng, rng};

pub mod actors;
pub mod camera;
pub mod controller;
pub mod events;
pub mod game_clock;
pub mod input;
pub mod materials;
pub mod raycast;
pub mod scene_builders;
pub mod voxel;

// Re-export commonly used event types
pub use events::{Event, EventBus};

/// Schlick approximation for Fresnel reflectance.
pub fn schlick(cosine: f32, ref_idx: f32) -> f32 {
    let mut r0 = (1f32 - ref_idx) / (1f32 + ref_idx);
    r0 = r0 * r0;
    r0 + (1f32 - r0) * (1f32 - cosine).powf(5f32)
}

/// Compute refracted ray via Snell's law, returns `None` for total internal reflection.
pub fn refract(v: Vec3, n: Vec3, ni_over_nt: f32) -> Option<Vec3> {
    let uv = v.normalize();
    let dt = uv.dot(n);
    let discriminant = 1.0f32 - ni_over_nt * ni_over_nt * (1f32 - dt * dt);
    if discriminant > 0f32 {
        Some(ni_over_nt * (uv - n * dt) - n * (discriminant.sqrt()))
    } else {
        None
    }
}

/// Reflect a vector about a normal.
pub fn reflect(v: Vec3, n: Vec3) -> Vec3 {
    v - 2f32 * v.dot(n) * n
}

/// Generate a random point inside the unit sphere (rejection sampling).
pub fn random_in_unit_sphere() -> Vec3 {
    let mut rng = rng();
    loop {
        let p =
            2f32 * Vec3::new(
                rng.random::<f32>(),
                rng.random::<f32>(),
                rng.random::<f32>(),
            ) - Vec3::ONE;
        if p.length_squared() < 1.0 {
            return p;
        }
    }
}

/// Generate a random point inside the unit disk (rejection sampling).
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

#[derive(Copy, Clone, Debug)]
pub struct ScatterRecord {
    pub attenuation: Vec3,
    pub scattered: Ray,
}

#[derive(Copy, Clone, Debug)]
pub struct Ray {
    a: Vec3,
    b: Vec3,
}

#[derive(Copy, Clone, Debug)]
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
