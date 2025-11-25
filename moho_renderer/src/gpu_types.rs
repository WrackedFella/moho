use bytemuck::{Pod, Zeroable};

use crate::shadow::CASCADE_SPLIT_DISTANCES;

/// GPU-visible material layout: two vec4-sized fields to satisfy WGSL
/// storage-buffer alignment and make the CPU representation match WGSL.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MaterialGpu {
    pub albedo: [f32; 4],
    pub params: [f32; 4],
}

impl MaterialGpu {
    /// Return true if this material was marked as potentially
    /// transparent by the application (params[2] > 0.0).
    #[allow(dead_code)]
    pub fn is_transparent(&self) -> bool {
        self.params[2] > 0.0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraGpu {
    pub vp0: [f32; 4],
    pub vp1: [f32; 4],
    pub vp2: [f32; 4],
    pub vp3: [f32; 4],
    pub cam_pos: [f32; 4],
}

/// GPU-visible lighting data for directional sun/moon light and ambient lighting
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LightingGpu {
    /// Directional light direction (normalized, xyz) and intensity (w)
    pub sun_direction: [f32; 4],
    /// Sun color (rgb) and unused (w)
    pub sun_color: [f32; 4],
    /// Moon direction (normalized, xyz) and intensity (w)
    pub moon_direction: [f32; 4],
    /// Moon color (rgb) and unused (w)
    pub moon_color: [f32; 4],
    /// Ambient light color (rgb) and intensity (w)
    pub ambient: [f32; 4],
    /// Time of day (x = 0-24 hours, yzw = unused)
    pub time_of_day: [f32; 4],
}

/// GPU-visible shadow matrix for light-space transformation (single shadow map - legacy)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ShadowMatrixGpu {
    /// Light view-projection matrix (4x4 stored as 4 vec4s)
    pub sm0: [f32; 4],
    pub sm1: [f32; 4],
    pub sm2: [f32; 4],
    pub sm3: [f32; 4],
}

/// Maximum number of shadow-casting lights supported
pub const MAX_SHADOW_LIGHTS: usize = 4;

/// GPU-visible multi-light shadow matrices
/// Contains up to 4 shadow matrices (Sun=0, Moon=1, Dynamic1=2, Dynamic2=3)
/// Total size: 288 bytes (4 lights × 64 bytes + 32 bytes metadata)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MultiLightShadowGpu {
    /// Light 0 matrix (Sun) - 4x4 stored as 4 vec4s
    pub light0_m0: [f32; 4],
    pub light0_m1: [f32; 4],
    pub light0_m2: [f32; 4],
    pub light0_m3: [f32; 4],

    /// Light 1 matrix (Moon) - 4x4 stored as 4 vec4s
    pub light1_m0: [f32; 4],
    pub light1_m1: [f32; 4],
    pub light1_m2: [f32; 4],
    pub light1_m3: [f32; 4],

    /// Light 2 matrix (Dynamic 1) - 4x4 stored as 4 vec4s
    pub light2_m0: [f32; 4],
    pub light2_m1: [f32; 4],
    pub light2_m2: [f32; 4],
    pub light2_m3: [f32; 4],

    /// Light 3 matrix (Dynamic 2) - 4x4 stored as 4 vec4s
    pub light3_m0: [f32; 4],
    pub light3_m1: [f32; 4],
    pub light3_m2: [f32; 4],
    pub light3_m3: [f32; 4],

    /// Light intensities for blending (x=Sun, y=Moon, z=Dynamic1, w=Dynamic2)
    pub light_intensities: [f32; 4],

    /// Active light count and metadata (x=count, yzw=unused)
    pub metadata: [f32; 4],
}

impl Default for MultiLightShadowGpu {
    fn default() -> Self {
        Self {
            // Identity matrices for all lights
            light0_m0: [1.0, 0.0, 0.0, 0.0],
            light0_m1: [0.0, 1.0, 0.0, 0.0],
            light0_m2: [0.0, 0.0, 1.0, 0.0],
            light0_m3: [0.0, 0.0, 0.0, 1.0],

            light1_m0: [1.0, 0.0, 0.0, 0.0],
            light1_m1: [0.0, 1.0, 0.0, 0.0],
            light1_m2: [0.0, 0.0, 1.0, 0.0],
            light1_m3: [0.0, 0.0, 0.0, 1.0],

            light2_m0: [1.0, 0.0, 0.0, 0.0],
            light2_m1: [0.0, 1.0, 0.0, 0.0],
            light2_m2: [0.0, 0.0, 1.0, 0.0],
            light2_m3: [0.0, 0.0, 0.0, 1.0],

            light3_m0: [1.0, 0.0, 0.0, 0.0],
            light3_m1: [0.0, 1.0, 0.0, 0.0],
            light3_m2: [0.0, 0.0, 1.0, 0.0],
            light3_m3: [0.0, 0.0, 0.0, 1.0],

            light_intensities: [0.0, 0.0, 0.0, 0.0],
            metadata: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl Default for ShadowMatrixGpu {
    fn default() -> Self {
        // Identity matrix by default
        Self {
            sm0: [1.0, 0.0, 0.0, 0.0],
            sm1: [0.0, 1.0, 0.0, 0.0],
            sm2: [0.0, 0.0, 1.0, 0.0],
            sm3: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// GPU-visible cascaded shadow matrices for CSM (Cascaded Shadow Maps)
/// Contains 2 shadow matrices (one per cascade) and their split distances
/// Two-cascade system for 50% performance improvement over 4-cascade system
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CascadedShadowMatrixGpu {
    /// Cascade 0 matrix (4x4 stored as 4 vec4s) - near cascade
    pub cascade0_m0: [f32; 4],
    pub cascade0_m1: [f32; 4],
    pub cascade0_m2: [f32; 4],
    pub cascade0_m3: [f32; 4],

    /// Cascade 1 matrix (4x4 stored as 4 vec4s) - far cascade
    pub cascade1_m0: [f32; 4],
    pub cascade1_m1: [f32; 4],
    pub cascade1_m2: [f32; 4],
    pub cascade1_m3: [f32; 4],

    /// Split distances for cascade boundaries (x = cascade 0 far plane, y = cascade 1 far plane, zw unused)
    pub split_distances: [f32; 4],
}

impl Default for CascadedShadowMatrixGpu {
    fn default() -> Self {
        // Identity matrices for both cascades by default
        Self {
            cascade0_m0: [1.0, 0.0, 0.0, 0.0],
            cascade0_m1: [0.0, 1.0, 0.0, 0.0],
            cascade0_m2: [0.0, 0.0, 1.0, 0.0],
            cascade0_m3: [0.0, 0.0, 0.0, 1.0],

            cascade1_m0: [1.0, 0.0, 0.0, 0.0],
            cascade1_m1: [0.0, 1.0, 0.0, 0.0],
            cascade1_m2: [0.0, 0.0, 1.0, 0.0],
            cascade1_m3: [0.0, 0.0, 0.0, 1.0],

            split_distances: [
                CASCADE_SPLIT_DISTANCES[0],
                CASCADE_SPLIT_DISTANCES[1],
                0.0, // unused
                0.0, // unused
            ],
        }
    }
}

impl Default for LightingGpu {
    fn default() -> Self {
        Self {
            // Default sun direction: lower in the sky (morning/evening light)
            // Normalized from approximately (0.7, 0.3, 0.6) for ~20° elevation
            sun_direction: [0.707, 0.303, 0.641, 1.0], // Lower angle, intensity=1.0
            // Warm sunlight color
            sun_color: [1.0, 0.95, 0.8, 0.0],
            // Moon direction: opposite the sun (below horizon initially)
            moon_direction: [0.0, -1.0, 0.0, 0.3], // Pointing down, intensity=0.3
            // Silver-blue moonlight
            moon_color: [0.7, 0.8, 0.9, 0.0],
            // Reduced ambient light to make shadows more visible
            ambient: [0.4, 0.5, 0.6, 0.1], // Cool ambient, intensity=0.1 (reduced from 0.3)
            // Default to dawn (6:00)
            time_of_day: [6.0, 0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_are_expected() {
        assert_eq!(std::mem::size_of::<MaterialGpu>(), 32);
        assert_eq!(std::mem::size_of::<CameraGpu>(), 80);
        assert_eq!(std::mem::size_of::<LightingGpu>(), 96); // 6 vec4s: sun_dir, sun_col, moon_dir, moon_col, ambient, time_of_day
        assert_eq!(std::mem::size_of::<ShadowMatrixGpu>(), 64);
        // CascadedShadowMatrixGpu: 2 matrices (2x64 bytes = 128 bytes) + 1 vec4 (16 bytes) = 144 bytes
        assert_eq!(std::mem::size_of::<CascadedShadowMatrixGpu>(), 144);
        // MultiLightShadowGpu: 4 matrices (4x64 bytes = 256 bytes) + 2 vec4s (32 bytes) = 288 bytes
        assert_eq!(std::mem::size_of::<MultiLightShadowGpu>(), 288);
    }
}
