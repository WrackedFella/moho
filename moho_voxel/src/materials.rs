use glam::Vec3;
use moho_render_api::{MaterialGpu, MaterialKey, RenderMaterial};

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

impl RenderMaterial for MaterialType {
    fn to_gpu(&self) -> MaterialGpu {
        match self {
            MaterialType::Lambertian { albedo } => MaterialGpu {
                // params: [fuzz, ref_idx, is_transparent, emissive_intensity]
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [0.0, 0.0, 0.0, 0.0],
            },
            MaterialType::Metal { albedo, fuzz } => MaterialGpu {
                albedo: [albedo.x, albedo.y, albedo.z, 0.0],
                params: [*fuzz, 0.0, 0.0, 0.0],
            },
            MaterialType::Dielectric { ref_indx } => MaterialGpu {
                albedo: [1.0, 1.0, 1.0, 0.0],
                params: [0.0, *ref_indx, 1.0, 0.0],
            },
            MaterialType::Emissive { color, intensity } => MaterialGpu {
                // params.w = emissive_intensity; fragment shader short-circuits lighting
                // when params.w > 0, outputting albedo * intensity directly.
                albedo: [color.x, color.y, color.z, 0.0],
                params: [0.0, 0.0, 0.0, *intensity],
            },
            MaterialType::VoxelTerrain {
                top_albedo,
                side_albedo,
            } => MaterialGpu {
                // For terrain (object_type == 0) the fragment shader selects
                // albedo.xyz (top) or params.xyz (side) based on face normal.
                albedo: [top_albedo.x, top_albedo.y, top_albedo.z, 0.0],
                params: [side_albedo.x, side_albedo.y, side_albedo.z, 0.0],
            },
        }
    }

    fn dedup_key(&self) -> MaterialKey {
        match self {
            MaterialType::Lambertian { albedo } => MaterialKey {
                variant: 0,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: 0,
                ref_idx_bits: 0,
                extra_bits: [0; 3],
            },
            MaterialType::Metal { albedo, fuzz } => MaterialKey {
                variant: 1,
                albedo_bits: [albedo.x.to_bits(), albedo.y.to_bits(), albedo.z.to_bits()],
                fuzz_bits: fuzz.to_bits(),
                ref_idx_bits: 0,
                extra_bits: [0; 3],
            },
            MaterialType::Dielectric { ref_indx } => MaterialKey {
                variant: 2,
                albedo_bits: [1u32, 1u32, 1u32],
                fuzz_bits: 0,
                ref_idx_bits: ref_indx.to_bits(),
                extra_bits: [0; 3],
            },
            MaterialType::Emissive { color, intensity } => MaterialKey {
                variant: 3,
                albedo_bits: [color.x.to_bits(), color.y.to_bits(), color.z.to_bits()],
                fuzz_bits: intensity.to_bits(),
                ref_idx_bits: 0,
                extra_bits: [0; 3],
            },
            MaterialType::VoxelTerrain {
                top_albedo,
                side_albedo,
            } => MaterialKey {
                variant: 4,
                albedo_bits: [
                    top_albedo.x.to_bits(),
                    top_albedo.y.to_bits(),
                    top_albedo.z.to_bits(),
                ],
                fuzz_bits: 0,
                ref_idx_bits: 0,
                extra_bits: [
                    side_albedo.x.to_bits(),
                    side_albedo.y.to_bits(),
                    side_albedo.z.to_bits(),
                ],
            },
        }
    }
}
