//! Sun and moon shadow matrices as plain functions over glam types, so they run without a GPU.

use glam::camera::rh::{proj::directx::orthographic, view::look_at_mat4};
use glam::{Mat4, Vec3, Vec4};

use super::{
    ActiveShadowLight, CASCADE_RADIUS_MULTIPLIER, MIN_SHADOW_LIGHT_INTENSITY, PcssSettings,
    SHADOW_DISTANCE, SHADOW_MAP_SIZE,
};
use crate::gpu_types::MultiLightShadowGpu;

/// Above this `|light_dir · Y|` the look-at's `forward × up` is too close to zero to normalise.
const OVERHEAD_DOT: f32 = 0.999;

/// View-projection of a directional light covering `SHADOW_DISTANCE` around `center`.
/// `light_dir` points toward the light.
pub(super) fn light_view_projection(light_dir: Vec3, center: Vec3) -> Mat4 {
    let light_dir = light_dir.normalize();
    let light_distance = SHADOW_DISTANCE * 2.0;
    let light_pos = center + light_dir * light_distance;

    // A light parallel to Y makes look_at's `forward × up` zero, which normalises to NaN.
    let up = if light_dir.dot(Vec3::Y).abs() > OVERHEAD_DOT {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let light_view = look_at_mat4(light_pos, center, up);

    let radius = SHADOW_DISTANCE * CASCADE_RADIUS_MULTIPLIER;
    let world_units_per_texel = (2.0 * radius) / SHADOW_MAP_SIZE as f32;
    let light_proj = orthographic(
        -radius,
        radius,
        -radius,
        radius,
        1.0,
        light_distance + SHADOW_DISTANCE,
    );

    // Maps -1..1 Z to 0..1 Z.
    let correction_matrix = Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 1.0),
    );

    // Snap the world origin to the texel grid so the map doesn't shimmer as the camera moves.
    let origin_light_space = light_view.transform_point3(Vec3::ZERO);
    let snapped_x = (origin_light_space.x / world_units_per_texel).floor() * world_units_per_texel;
    let snapped_y = (origin_light_space.y / world_units_per_texel).floor() * world_units_per_texel;
    let snap = Mat4::from_translation(Vec3::new(
        snapped_x - origin_light_space.x,
        snapped_y - origin_light_space.y,
        0.0,
    ));

    correction_matrix * light_proj * snap * light_view
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

/// The shader's shadow block: sun and moon matrices, identity for the two unused slots.
pub(super) fn multi_light_gpu(
    sun: Mat4,
    moon: Mat4,
    (sun_intensity, moon_intensity): (f32, f32),
    pcss: &PcssSettings,
) -> MultiLightShadowGpu {
    let sun_cols = sun.to_cols_array_2d();
    let moon_cols = moon.to_cols_array_2d();
    let unused = Mat4::IDENTITY.to_cols_array_2d();

    MultiLightShadowGpu {
        light0_m0: sun_cols[0],
        light0_m1: sun_cols[1],
        light0_m2: sun_cols[2],
        light0_m3: sun_cols[3],

        light1_m0: moon_cols[0],
        light1_m1: moon_cols[1],
        light1_m2: moon_cols[2],
        light1_m3: moon_cols[3],

        light2_m0: unused[0],
        light2_m1: unused[1],
        light2_m2: unused[2],
        light2_m3: unused[3],

        light3_m0: unused[0],
        light3_m1: unused[1],
        light3_m2: unused[2],
        light3_m3: unused[3],

        light_intensities: [sun_intensity, moon_intensity, 0.0, 0.0],
        // [shadow_distance, light_size, pcss_quality, view-height cut (set per frame by shadow_ops)]
        metadata: [
            SHADOW_DISTANCE,
            pcss.light_size,
            pcss.quality as u32 as f32,
            0.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::shadow::{
        CASCADE_RADIUS_MULTIPLIER, MIN_SHADOW_LIGHT_INTENSITY, PcssQuality, SHADOW_DISTANCE,
        SHADOW_MAP_SIZE,
    };

    fn above_horizon() -> impl Strategy<Value = Vec3> {
        (-1.0f32..1.0, 0.05f32..1.0, -1.0f32..1.0)
            .prop_map(|(x, y, z)| Vec3::new(x, y, z).normalize())
    }

    fn any_position() -> impl Strategy<Value = Vec3> {
        (-500.0f32..500.0, -50.0f32..200.0, -500.0f32..500.0)
            .prop_map(|(x, y, z)| Vec3::new(x, y, z))
    }

    fn direction_on_sphere() -> impl Strategy<Value = Vec3> {
        (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
            .prop_filter("non-degenerate", |(x, y, z)| x * x + y * y + z * z > 1e-3)
            .prop_map(|(x, y, z)| Vec3::new(x, y, z).normalize())
    }

    proptest! {
        #[test]
        fn points_within_shadow_distance_map_inside_the_clip_volume(
            light in above_horizon(),
            camera in any_position(),
            offset in direction_on_sphere(),
        ) {
            let matrix = light_view_projection(light, camera);
            let point = camera + offset * SHADOW_DISTANCE;

            let clip = matrix.project_point3(point);

            let slack = 1e-3;
            prop_assert!(clip.x.abs() <= 1.0 + slack, "x {} outside", clip.x);
            prop_assert!(clip.y.abs() <= 1.0 + slack, "y {} outside", clip.y);
            prop_assert!((-slack..=1.0 + slack).contains(&clip.z), "depth {} outside 0..1", clip.z);
        }

        #[test]
        fn world_origin_lands_on_a_whole_texel(light in above_horizon(), camera in any_position()) {
            let clip = light_view_projection(light, camera).project_point3(Vec3::ZERO);

            let half_map = SHADOW_MAP_SIZE as f32 / 2.0;
            for axis in [clip.x * half_map, clip.y * half_map] {
                prop_assert!((axis - axis.round()).abs() < 1e-2, "texel coordinate {axis}");
            }
        }

        #[test]
        fn snapping_shifts_the_camera_by_under_one_texel(light in above_horizon(), camera in any_position()) {
            // The reference view uses Y as up, which the overhead branch deliberately replaces.
            prop_assume!(light.dot(Vec3::Y).abs() <= OVERHEAD_DOT);
            let snapped = light_view_projection(light, camera);
            let unsnapped_origin_clip = unsnapped_clip(light, camera, Vec3::ZERO);
            let snapped_origin_clip = snapped.project_point3(Vec3::ZERO);

            let half_map = SHADOW_MAP_SIZE as f32 / 2.0;
            let shift_x = (snapped_origin_clip.x - unsnapped_origin_clip.x) * half_map;
            let shift_y = (snapped_origin_clip.y - unsnapped_origin_clip.y) * half_map;
            prop_assert!((-1.01..=0.01).contains(&shift_x), "x shift {shift_x} texels");
            prop_assert!((-1.01..=0.01).contains(&shift_y), "y shift {shift_y} texels");
        }
    }

    /// Clip-space position of `point` under the same view and projection without the snap.
    fn unsnapped_clip(light: Vec3, camera: Vec3, point: Vec3) -> Vec3 {
        let light = light.normalize();
        let view = glam::camera::rh::view::look_at_mat4(
            camera + light * SHADOW_DISTANCE * 2.0,
            camera,
            Vec3::Y,
        );
        let radius = SHADOW_DISTANCE * CASCADE_RADIUS_MULTIPLIER;
        let projection = glam::camera::rh::proj::directx::orthographic(
            -radius,
            radius,
            -radius,
            radius,
            1.0,
            SHADOW_DISTANCE * 3.0,
        );
        (projection * view).project_point3(point)
    }

    proptest! {
        #[test]
        fn camera_maps_to_the_middle_of_the_map_at_a_fixed_depth(
            light in above_horizon(),
            camera in any_position(),
        ) {
            let clip = light_view_projection(light, camera).project_point3(camera);

            // Depth is 0.5 + 0.5 * (2000 - 1) / (3000 - 1): the camera is 2 * SHADOW_DISTANCE into
            // a 3 * SHADOW_DISTANCE volume, and the 0..1 correction stacks on an already-0..1 ortho.
            let texel = 2.0 / SHADOW_MAP_SIZE as f32;
            prop_assert!(clip.x.abs() <= texel * 1.01, "x {}", clip.x);
            prop_assert!(clip.y.abs() <= texel * 1.01, "y {}", clip.y);
            prop_assert!((clip.z - 0.8333).abs() < 1e-3, "depth {}", clip.z);
        }
    }

    #[test]
    fn light_straight_overhead_gives_a_finite_matrix() {
        for light in [Vec3::Y, Vec3::NEG_Y] {
            let matrix = light_view_projection(light, Vec3::new(12.0, 3.0, -7.0));

            assert!(
                matrix.to_cols_array().iter().all(|v| v.is_finite()),
                "non-finite element for {light}: {matrix:?}"
            );
        }
    }

    #[test]
    fn overhead_light_still_maps_nearby_points_into_the_shadow_map() {
        let camera = Vec3::new(12.0, 3.0, -7.0);
        let matrix = light_view_projection(Vec3::Y, camera);

        let clip = matrix.project_point3(camera);

        assert!(clip.x.abs() < 1.0 && clip.y.abs() < 1.0, "{clip:?}");
        assert!((0.0..=1.0).contains(&clip.z), "{clip:?}");
    }

    fn marker(scale: f32) -> Mat4 {
        Mat4::from_scale(Vec3::splat(scale))
    }

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

    #[test]
    fn gpu_block_packs_sun_moon_and_identity_slots() {
        let sun = marker(1.0) * Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let moon = marker(2.0) * Mat4::from_translation(Vec3::new(4.0, 5.0, 6.0));
        let pcss = PcssSettings {
            quality: PcssQuality::High,
            light_size: 0.07,
        };

        let gpu = multi_light_gpu(sun, moon, (0.9, 0.3), &pcss);

        let sun_cols = sun.to_cols_array_2d();
        let moon_cols = moon.to_cols_array_2d();
        assert_eq!(
            [gpu.light0_m0, gpu.light0_m1, gpu.light0_m2, gpu.light0_m3],
            sun_cols
        );
        assert_eq!(
            [gpu.light1_m0, gpu.light1_m1, gpu.light1_m2, gpu.light1_m3],
            moon_cols
        );
        let identity = Mat4::IDENTITY.to_cols_array_2d();
        assert_eq!(
            [gpu.light2_m0, gpu.light2_m1, gpu.light2_m2, gpu.light2_m3],
            identity
        );
        assert_eq!(
            [gpu.light3_m0, gpu.light3_m1, gpu.light3_m2, gpu.light3_m3],
            identity
        );
        assert_eq!(gpu.light_intensities, [0.9, 0.3, 0.0, 0.0]);
        assert_eq!(
            gpu.metadata,
            [SHADOW_DISTANCE, 0.07, PcssQuality::High as u32 as f32, 0.0]
        );
    }
}
