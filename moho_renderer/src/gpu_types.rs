use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CameraGpu {
    pub vp0: [f32; 4],
    pub vp1: [f32; 4],
    pub vp2: [f32; 4],
    pub vp3: [f32; 4],
    /// Camera position (xyz) and view-height cut (w): world y above it is discarded.
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
    /// Sky-ambient strength (w); xyz unused. The ambient colour comes from the sky palette.
    pub ambient: [f32; 4],
    /// Shader parameters:
    /// x = Time of day (0-24 hours)
    /// y = Debug Mode (0=None, 1=Normals, 2=Bias, 3=Shadows, 4=Light level, 5=Point lights, 6=Sky ambient, 7=Sky reflection, 8=Cascades)
    /// zw = Unused
    pub params: [f32; 4],
    /// Sky zenith colour (rgb) and unused (w). The renderer fills it from the time of
    /// day in `params[0]`; a game's value is replaced.
    pub sky_zenith: [f32; 4],
    /// Sky horizon colour (rgb) and unused (w). Filled by the renderer.
    pub sky_horizon: [f32; 4],
    /// Sky ground colour (rgb) and unused (w). Filled by the renderer.
    pub sky_ground: [f32; 4],
}

/// Uniform for the tone-map pass.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ToneMapGpu {
    pub exposure: f32,
    pub enabled: u32,
    /// Weight of the bloom chain added before exposure; 0 skips the bloom read.
    pub bloom_intensity: f32,
    _pad: u32,
}

impl ToneMapGpu {
    pub fn new(exposure: f32, enabled: bool, bloom_intensity: f32) -> Self {
        Self {
            exposure,
            enabled: u32::from(enabled),
            bloom_intensity,
            _pad: 0,
        }
    }
}

/// GPU-visible cascaded shadow block: sun layers 0-3, moon layers 4-5. Total size: 496 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct ShadowGpu {
    /// Light view-projection per layer
    pub matrices: [[[f32; 4]; 4]; 6],
    /// Far end of each cascade by view depth: sun in `[0]`, moon in `[1].xy`
    pub split_far: [[f32; 4]; 2],
    /// World size of one texel per cascade, packed like `split_far`
    pub texel_world: [[f32; 4]; 2],
    /// Camera forward (xyz) and the cascade blend fraction (w)
    pub camera_forward: [f32; 4],
    /// Light intensities (x=Sun, y=Moon)
    pub light_intensities: [f32; 4],
    /// Metadata: x=shadow distance, y=light_size (for PCSS), z=pcss_quality, w=view-height cut
    pub metadata: [f32; 4],
}

/// Uniform value for the view-height cut: `None` and non-finite heights map to
/// `f32::MAX`, which `y > cut` never exceeds. Not infinity, because WGSL lets
/// implementations assume no infinities.
pub fn view_height_uniform(height: Option<f32>) -> f32 {
    height.filter(|h| h.is_finite()).unwrap_or(f32::MAX)
}

/// Maximum number of dynamic point/spot lights supported
pub const MAX_DYNAMIC_LIGHTS: usize = 64;

/// Single point light data for GPU (32 bytes aligned to 16-byte boundaries)
/// Position and attenuation parameters for distance-based light falloff
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct PointLightGpu {
    /// Position (xyz) and range (w) - maximum distance light reaches
    pub position_range: [f32; 4],
    /// Color (rgb) and intensity (w) - brightness multiplier
    pub color_intensity: [f32; 4],
}

/// Dynamic lights buffer containing active point lights (1040 bytes max)
/// Sent to GPU as storage buffer for light accumulation loop
/// Structure: light_count (16 bytes) + lights array (64 × 32 = 2048 bytes)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DynamicLightsGpu {
    /// Number of active lights (x) and padding (yzw). Total 16 bytes for alignment.
    pub light_count: [u32; 4],
    /// Array of point lights (up to MAX_DYNAMIC_LIGHTS)
    pub lights: [PointLightGpu; MAX_DYNAMIC_LIGHTS],
}

impl Default for DynamicLightsGpu {
    fn default() -> Self {
        Self {
            light_count: [0, 0, 0, 0],
            lights: [PointLightGpu::default(); MAX_DYNAMIC_LIGHTS],
        }
    }
}

impl Default for PointLightGpu {
    fn default() -> Self {
        Self {
            position_range: [0.0, 0.0, 0.0, 10.0], // Default range of 10 units
            color_intensity: [1.0, 1.0, 1.0, 0.0], // White light, intensity 0 (off)
        }
    }
}

impl Default for LightingGpu {
    fn default() -> Self {
        let [sky_zenith, sky_horizon, sky_ground] = crate::sky::sky_palette(6.0).to_gpu();
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
            ambient: [0.0, 0.0, 0.0, 0.1],
            // Default to dawn (6:00)
            params: [6.0, 0.0, 0.0, 0.0],
            sky_zenith,
            sky_horizon,
            sky_ground,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_are_expected() {
        assert_eq!(std::mem::size_of::<CameraGpu>(), 80);
        assert_eq!(std::mem::size_of::<LightingGpu>(), 144); // 9 vec4s
        assert_eq!(std::mem::size_of::<ToneMapGpu>(), 16);
        // ShadowGpu: 6 matrices (384 bytes) + 2 split vec4s + 2 texel vec4s + 3 vec4s = 496 bytes
        assert_eq!(std::mem::size_of::<ShadowGpu>(), 496);
        assert_eq!(
            std::mem::size_of::<ShadowGpu>(),
            crate::shadow::CASCADE_LAYERS * 64 + 7 * 16,
            "one matrix per cascade layer, plus seven vec4s"
        );
    }

    #[test]
    fn lighting_default_carries_the_dawn_sky_palette() {
        let lighting = LightingGpu::default();

        let palette = crate::sky::sky_palette(6.0);

        assert_eq!(lighting.params[0], 6.0);
        let rgb = |v: [f32; 4]| [v[0], v[1], v[2]];
        assert_eq!(rgb(lighting.sky_zenith), palette.zenith);
        assert_eq!(rgb(lighting.sky_horizon), palette.horizon);
        assert_eq!(rgb(lighting.sky_ground), palette.ground);
    }

    #[test]
    fn sky_fields_follow_params_so_existing_offsets_hold() {
        assert_eq!(std::mem::offset_of!(LightingGpu, params), 80);
        assert_eq!(std::mem::offset_of!(LightingGpu, sky_zenith), 96);
        assert_eq!(std::mem::offset_of!(LightingGpu, sky_horizon), 112);
        assert_eq!(std::mem::offset_of!(LightingGpu, sky_ground), 128);
    }

    #[test]
    fn view_height_uniform_passes_a_set_height() {
        assert_eq!(view_height_uniform(Some(12.5)), 12.5);
        assert_eq!(view_height_uniform(Some(-3.0)), -3.0);
    }

    #[test]
    fn view_height_uniform_none_is_no_cut() {
        assert_eq!(view_height_uniform(None), f32::MAX);
    }

    #[test]
    fn view_height_uniform_non_finite_is_no_cut() {
        for height in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(view_height_uniform(Some(height)), f32::MAX, "{height}");
        }
    }
}
