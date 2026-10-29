//! Sun and moon shadow layers and uniform block as plain functions over glam types, so they run
//! without a GPU.

use glam::Vec3;

use super::cascades::CascadeFit;
use super::tier::ShadowTier;
use super::{
    ActiveShadowLight, CASCADE_BLEND, CASCADE_LAYERS, MIN_SHADOW_LIGHT_INTENSITY, MOON_CASCADES,
    SUN_CASCADES,
};
use crate::gpu_types::ShadowGpu;

/// Above this `|light_dir · Y|` the look-at's `forward × up` is too close to zero to normalise.
const OVERHEAD_DOT: f32 = 0.999;

/// The look-at up vector for a light toward `light_dir`. A light parallel to Y makes
/// `forward × up` zero, which normalises to NaN, so an overhead light looks along Z instead.
pub(super) fn light_up(light_dir: Vec3) -> Vec3 {
    if light_dir.dot(Vec3::Y).abs() > OVERHEAD_DOT {
        Vec3::Z
    } else {
        Vec3::Y
    }
}

/// The lights that cast a shadow this frame: sun in slot 0, moon in slot 1, each only when its
/// intensity is above `MIN_SHADOW_LIGHT_INTENSITY`.
pub(super) fn active_shadow_lights(sun: f32, moon: f32) -> Vec<ActiveShadowLight> {
    [(0, sun), (1, moon)]
        .into_iter()
        .filter(|(_, intensity)| *intensity > MIN_SHADOW_LIGHT_INTENSITY)
        .map(|(light_index, intensity)| ActiveShadowLight {
            light_index,
            intensity,
        })
        .collect()
}

/// The shadow-map layers to render for `active`: a sun's four cascades, a moon's two.
pub(super) fn shadow_passes(active: &[ActiveShadowLight], tier: ShadowTier) -> Vec<u32> {
    let lights = if tier.casts_shadows() { active } else { &[] };
    lights
        .iter()
        .flat_map(|light| match light.light_index {
            0 => 0..SUN_CASCADES as u32,
            _ => SUN_CASCADES as u32..CASCADE_LAYERS as u32,
        })
        .collect()
}

/// The shader's cascaded shadow block. `splits` are the sun's and the moon's far ends by view
/// depth; the effective shadow distance is the sun's last.
pub(super) fn shadow_gpu(
    sun: &[CascadeFit; SUN_CASCADES],
    moon: &[CascadeFit; MOON_CASCADES],
    splits: ([f32; SUN_CASCADES], [f32; MOON_CASCADES]),
    camera_forward: Vec3,
    (sun_intensity, moon_intensity): (f32, f32),
    tier: ShadowTier,
) -> ShadowGpu {
    let mut matrices = [[[0.0; 4]; 4]; CASCADE_LAYERS];
    for (matrix, fit) in matrices.iter_mut().zip(sun.iter().chain(moon)) {
        *matrix = fit.view_proj.to_cols_array_2d();
    }
    let mut split_far = [[0.0; 4]; 2];
    split_far[0] = splits.0;
    split_far[1][..MOON_CASCADES].copy_from_slice(&splits.1);
    let mut texel_world = [[0.0; 4]; 2];
    for (slot, fit) in texel_world[0].iter_mut().zip(sun) {
        *slot = fit.texel_world;
    }
    for (slot, fit) in texel_world[1].iter_mut().zip(moon) {
        *slot = fit.texel_world;
    }

    ShadowGpu {
        matrices,
        split_far,
        texel_world,
        camera_forward: [
            camera_forward.x,
            camera_forward.y,
            camera_forward.z,
            CASCADE_BLEND,
        ],
        light_intensities: [sun_intensity, moon_intensity, 0.0, 0.0],
        metadata: [
            if tier.casts_shadows() {
                splits.0[SUN_CASCADES - 1]
            } else {
                0.0
            },
            0.0,
            tier.pcf_radius() as f32,
            0.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    use glam::Mat4;

    use super::*;
    use crate::shadow::{CASCADE_BLEND, MIN_SHADOW_LIGHT_INTENSITY};

    #[test]
    fn only_lights_above_the_floor_are_active() {
        let floor = MIN_SHADOW_LIGHT_INTENSITY;
        let cases: [(f32, f32, &[u32]); 5] = [
            (1.0, 0.5, &[0, 1]),
            (1.0, 0.0, &[0]),
            (0.0, 0.5, &[1]),
            (0.0, 0.0, &[]),
            (floor, floor, &[]),
        ];

        for (sun, moon, expected) in cases {
            let lights = active_shadow_lights(sun, moon);

            let slots: Vec<u32> = lights.iter().map(|l| l.light_index).collect();
            assert_eq!(slots, expected, "sun {sun}, moon {moon}");
        }
    }

    #[test]
    fn active_lights_carry_slot_and_intensity() {
        let lights = active_shadow_lights(0.8, 0.4);

        assert_eq!(lights[0].light_index, 0);
        assert_eq!(lights[0].intensity, 0.8);
        assert_eq!(lights[1].light_index, 1);
        assert_eq!(lights[1].intensity, 0.4);
    }

    fn light(light_index: u32) -> ActiveShadowLight {
        ActiveShadowLight {
            light_index,
            intensity: 1.0,
        }
    }

    #[test]
    fn passes_are_the_active_lights_cascade_layers() {
        let cases: [(&[ActiveShadowLight], &[u32]); 4] = [
            (&[light(0)], &[0, 1, 2, 3]),
            (&[light(1)], &[4, 5]),
            (&[light(0), light(1)], &[0, 1, 2, 3, 4, 5]),
            (&[], &[]),
        ];

        for (active, expected) in cases {
            let layers = shadow_passes(active, ShadowTier::High);

            assert_eq!(layers, expected, "active {active:?}");
        }
    }

    fn fits() -> ([CascadeFit; SUN_CASCADES], [CascadeFit; MOON_CASCADES]) {
        let fit = |i: u32| CascadeFit {
            view_proj: Mat4::from_translation(Vec3::new(i as f32, 2.0 * i as f32, 3.0)),
            texel_world: 0.01 * (i + 1) as f32,
        };
        (
            std::array::from_fn(|i| fit(i as u32)),
            std::array::from_fn(|i| fit(4 + i as u32)),
        )
    }

    #[test]
    fn off_lists_no_passes_and_zero_distance() {
        let (sun, moon) = fits();
        let splits = ([30.0, 90.0, 300.0, 700.0], [50.0, 700.0]);

        let layers = shadow_passes(&[light(0), light(1)], ShadowTier::Off);
        let gpu = shadow_gpu(
            &sun,
            &moon,
            splits,
            Vec3::new(0.0, 0.6, -0.8),
            (0.9, 0.3),
            ShadowTier::Off,
        );

        assert!(layers.is_empty(), "layers {layers:?}");
        assert_eq!(gpu.metadata[0], 0.0);
    }

    #[test]
    fn gpu_block_carries_the_filter_radius_in_metadata_z() {
        let (sun, moon) = fits();
        let splits = ([30.0, 90.0, 300.0, 700.0], [50.0, 700.0]);

        for (tier, radius) in [
            (ShadowTier::Low, 1.0),
            (ShadowTier::Medium, 1.0),
            (ShadowTier::High, 2.0),
            (ShadowTier::Ultra, 3.0),
        ] {
            let gpu = shadow_gpu(&sun, &moon, splits, Vec3::NEG_Z, (0.9, 0.3), tier);

            assert_eq!(gpu.metadata[2], radius, "{tier:?}");
            assert_eq!(gpu.metadata[1], 0.0, "{tier:?}: y is unused");
            assert_eq!(gpu.metadata[0], 700.0, "{tier:?}: distance stays on");
        }
    }

    #[test]
    fn gpu_block_packs_six_layers_splits_and_texel_sizes() {
        let fit = |i: u32| CascadeFit {
            view_proj: Mat4::from_translation(Vec3::new(i as f32, 2.0 * i as f32, 3.0)),
            texel_world: 0.01 * (i + 1) as f32,
        };
        let sun: [CascadeFit; SUN_CASCADES] = std::array::from_fn(|i| fit(i as u32));
        let moon: [CascadeFit; MOON_CASCADES] = std::array::from_fn(|i| fit(4 + i as u32));
        let splits = ([30.0, 90.0, 300.0, 700.0], [50.0, 700.0]);

        let gpu = shadow_gpu(
            &sun,
            &moon,
            splits,
            Vec3::new(0.0, 0.6, -0.8),
            (0.9, 0.3),
            ShadowTier::High,
        );

        for (layer, fit) in sun.iter().chain(&moon).enumerate() {
            assert_eq!(
                gpu.matrices[layer],
                fit.view_proj.to_cols_array_2d(),
                "layer {layer}"
            );
        }
        assert_eq!(
            gpu.split_far,
            [[30.0, 90.0, 300.0, 700.0], [50.0, 700.0, 0.0, 0.0]]
        );
        assert_eq!(
            gpu.texel_world,
            [
                [
                    sun[0].texel_world,
                    sun[1].texel_world,
                    sun[2].texel_world,
                    sun[3].texel_world
                ],
                [moon[0].texel_world, moon[1].texel_world, 0.0, 0.0]
            ]
        );
        assert_eq!(gpu.camera_forward, [0.0, 0.6, -0.8, CASCADE_BLEND]);
        assert_eq!(gpu.light_intensities, [0.9, 0.3, 0.0, 0.0]);
        assert_eq!(
            gpu.metadata,
            [700.0, 0.0, 2.0, 0.0],
            "the effective shadow distance is the sun's last split"
        );
    }
}
