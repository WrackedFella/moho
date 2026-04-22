use glam::Vec3;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MaterialType {
    Lambertian {
        albedo: Vec3,
    },
    Metal {
        albedo: Vec3,
        fuzz: f32,
    },
    Dielectric {
        ref_indx: f32,
    },
    /// Bypasses all lighting — outputs `color * intensity` regardless of scene lights.
    /// Used for editor gizmos, UI geometry, and self-illuminating objects.
    Emissive {
        color: Vec3,
        intensity: f32,
    },
    /// Terrain material where `top_albedo` is used for near-vertical normals (grass)
    /// and `side_albedo` for horizontal normals (dirt/rock sides).
    VoxelTerrain {
        top_albedo: Vec3,
        side_albedo: Vec3,
    },
}
