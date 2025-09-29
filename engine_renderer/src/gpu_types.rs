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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_are_expected() {
        assert_eq!(std::mem::size_of::<MaterialGpu>(), 32);
        assert_eq!(std::mem::size_of::<CameraGpu>(), 80);
    }
}
