use bytemuck::{Pod, Zeroable};

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

/// GPU-visible lighting data for directional sun light and ambient lighting
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LightingGpu {
    /// Directional light direction (normalized, xyz) and intensity (w)
    pub sun_direction: [f32; 4],
    /// Sun color (rgb) and unused (w)
    pub sun_color: [f32; 4],
    /// Ambient light color (rgb) and intensity (w)
    pub ambient: [f32; 4],
}

impl Default for LightingGpu {
    fn default() -> Self {
        Self {
            // Default sun direction: from upper-right-front
            sun_direction: [0.577, 0.577, 0.577, 1.0], // normalized(1,1,1), intensity=1.0
            // Warm sunlight color
            sun_color: [1.0, 0.95, 0.8, 0.0],
            // Soft ambient light
            ambient: [0.4, 0.5, 0.6, 0.3], // Cool ambient, intensity=0.3
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
        assert_eq!(std::mem::size_of::<LightingGpu>(), 48);
    }
}
