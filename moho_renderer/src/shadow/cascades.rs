//! Cascaded shadow-map fits as plain functions over glam types, so they run without a GPU.

use glam::camera::rh::proj::directx::orthographic;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

use super::light_matrix::light_up;
use super::{
    CASCADE_BLEND, CASTER_REACH, FILTER_MARGIN_TEXELS, SHADOW_DISTANCE, SPLIT_LAMBDA, SPLIT_NEAR,
};

/// Perspective camera shape the cascades are cut from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CameraFrustum {
    pub(super) near: f32,
    /// Infinite for an infinite perspective.
    pub(super) far: f32,
    /// Largest `|xy| / |z|` of a corner ray.
    pub(super) corner_slope: f32,
}

impl CameraFrustum {
    /// The frustum of a perspective `proj`, or `None` when `proj` is not a perspective projection.
    pub(super) fn from_projection(proj: Mat4) -> Option<Self> {
        if proj.w_axis.w != 0.0 {
            return None;
        }
        let inverse = proj.inverse();
        let corners = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)]
            .map(|(x, y)| inverse.project_point3(Vec3::new(x, y, 0.0)));
        let far = -inverse.project_point3(Vec3::new(0.0, 0.0, 1.0)).z;

        Some(Self {
            near: -corners[0].z,
            // An infinite perspective unprojects its far plane to infinity or NaN.
            far: if far.is_finite() && far > 0.0 {
                far
            } else {
                f32::INFINITY
            },
            corner_slope: corners
                .iter()
                .map(|corner| corner.truncate().length() / -corner.z)
                .fold(0.0, f32::max),
        })
    }
}

/// Far end of each of `N` slices over `near..min(far, SHADOW_DISTANCE)`.
pub(super) fn practical_splits<const N: usize>(near: f32, far: f32) -> [f32; N] {
    let start = near.max(SPLIT_NEAR);
    let end = far.min(SHADOW_DISTANCE);

    std::array::from_fn(|i| {
        let fraction = (i + 1) as f32 / N as f32;
        let logarithmic = start * (end / start).powf(fraction);
        let uniform = start + (end - start) * fraction;
        SPLIT_LAMBDA * logarithmic + (1.0 - SPLIT_LAMBDA) * uniform
    })
}

/// `(centre_distance, radius)` of the smallest sphere centred on the view axis holding the slice.
pub(super) fn slice_sphere(near: f32, far: f32, corner_slope: f32) -> (f32, f32) {
    let centre = ((1.0 + corner_slope * corner_slope) * (near + far) / 2.0).min(far);
    let radius = (far - centre)
        .hypot(corner_slope * far)
        .max((centre - near).hypot(corner_slope * near));
    (centre, radius)
}

/// One cascade: the light's view-projection and the world size of one shadow-map texel.
#[derive(Debug, Clone, Copy)]
pub(super) struct CascadeFit {
    pub(super) view_proj: Mat4,
    pub(super) texel_world: f32,
}

/// Fit an orthographic light view around the sphere (`centre`, `radius`); `light_dir` points
/// toward the light.
pub(super) fn fit_cascade(light_dir: Vec3, centre: Vec3, radius: f32, map_size: u32) -> CascadeFit {
    let light_dir = light_dir.normalize();
    let light_pos = centre + light_dir * (radius + CASTER_REACH);
    let light_view = look_at_mat4(light_pos, centre, light_up(light_dir));
    let light_proj = orthographic(
        -radius,
        radius,
        -radius,
        radius,
        0.0,
        2.0 * radius + CASTER_REACH,
    );
    let texel_world = 2.0 * radius / map_size as f32;

    // Snap the world origin to the texel grid so the map doesn't shimmer as the camera moves.
    let origin = light_view.transform_point3(Vec3::ZERO);
    let snap = Mat4::from_translation(Vec3::new(
        (origin.x / texel_world).floor() * texel_world - origin.x,
        (origin.y / texel_world).floor() * texel_world - origin.y,
        0.0,
    ));

    CascadeFit {
        view_proj: light_proj * snap * light_view,
        texel_world,
    }
}

/// `N` cascade fits for the light, near to far. `camera_world` is the inverse view matrix.
pub(super) fn light_cascades<const N: usize>(
    light_dir: Vec3,
    frustum: CameraFrustum,
    camera_world: Mat4,
    map_size: u32,
) -> [CascadeFit; N] {
    let splits = practical_splits::<N>(frustum.near, frustum.far);
    let position = camera_world.w_axis.truncate();
    let forward = -camera_world.z_axis.truncate().normalize();
    let size = map_size as f32;
    let padding = size / (size - 2.0 * FILTER_MARGIN_TEXELS);

    std::array::from_fn(|i| {
        let slice_near = if i == 0 {
            frustum.near
        } else {
            let before = if i == 1 { frustum.near } else { splits[i - 2] };
            splits[i - 1] - CASCADE_BLEND * (splits[i - 1] - before)
        };
        let (distance, radius) = slice_sphere(slice_near, splits[i], frustum.corner_slope);
        fit_cascade(
            light_dir,
            position + forward * distance,
            radius * padding,
            map_size,
        )
    })
}

#[cfg(test)]
mod tests {
    use glam::camera::rh::proj::directx::{orthographic, perspective, perspective_infinite};
    use proptest::prelude::*;

    use super::*;
    use crate::shadow::{
        CASCADE_BLEND, CASCADE_MAP_SIZE, FILTER_MARGIN_TEXELS, SHADOW_DISTANCE, SUN_CASCADES,
    };

    const SEARCHLIGHT: (f32, f32, f32) = (75.0, 0.05, 500.0);
    const FLAGSTONE: (f32, f32, f32) = (45.0, 0.1, 1500.0);
    const SIXTEEN_NINE: f32 = 16.0 / 9.0;
    const TWENTY_ONE_NINE: f32 = 21.0 / 9.0;

    fn above_horizon() -> impl Strategy<Value = Vec3> {
        (-1.0f32..1.0, 0.05f32..1.0, -1.0f32..1.0)
            .prop_map(|(x, y, z)| Vec3::new(x, y, z).normalize())
    }

    fn any_position() -> impl Strategy<Value = Vec3> {
        (-200.0f32..200.0, -50.0f32..100.0, -200.0f32..200.0)
            .prop_map(|(x, y, z)| Vec3::new(x, y, z))
    }

    /// Inverse view matrix of a camera at `position` looking along `(yaw, pitch)`.
    fn camera_world(position: Vec3, yaw: f32, pitch: f32) -> Mat4 {
        Mat4::from_translation(position) * Mat4::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0)
    }

    fn frustum(fov_degrees: f32, aspect: f32, near: f32, far: f32) -> CameraFrustum {
        let proj = perspective(fov_degrees.to_radians(), aspect, near, far);
        CameraFrustum::from_projection(proj).expect("a perspective projection has a frustum")
    }

    fn close(a: f32, b: f32, relative: f32) -> bool {
        (a - b).abs() <= relative * a.abs().max(b.abs()).max(1e-6)
    }

    fn splits_far() -> impl Strategy<Value = f32> {
        prop_oneof![50.0f32..5000.0, Just(f32::INFINITY)]
    }

    proptest! {
        #[test]
        fn splits_rise_and_end_at_the_shadow_distance(near in 0.01f32..1.0, far in splits_far()) {
            let splits = practical_splits::<4>(near, far);

            let mut previous = near;
            for split in splits {
                prop_assert!(split.is_finite(), "split {split} in {splits:?}");
                prop_assert!(split > previous, "{splits:?} does not rise past {previous}");
                previous = split;
            }
            let end = far.min(SHADOW_DISTANCE);
            prop_assert!(close(splits[3], end, 1e-4), "ends at {} not {end}", splits[3]);
        }

        #[test]
        fn slice_corners_and_blend_band_map_inside_their_cascade(
            light in above_horizon(),
            position in any_position(),
            yaw in 0.0f32..std::f32::consts::TAU,
            pitch in -1.4f32..1.4,
            fov in 30.0f32..100.0,
            aspect in 1.0f32..2.4,
            near in 0.05f32..1.0,
            far in 50.0f32..2000.0,
        ) {
            let frustum = frustum(fov, aspect, near, far);
            let world = camera_world(position, yaw, pitch);
            let fits = light_cascades::<4>(light, frustum, world, CASCADE_MAP_SIZE);
            let splits = practical_splits::<4>(frustum.near, frustum.far);
            let tan_y = (fov.to_radians() / 2.0).tan();
            let tan_x = tan_y * aspect;
            // One of the filter margin's texels is spent on the origin snap.
            let margin = 2.0 * (FILTER_MARGIN_TEXELS - 1.0) / CASCADE_MAP_SIZE as f32;
            let slack = 1e-4;

            for (i, fit) in fits.iter().enumerate() {
                let slice_far = splits[i];
                let slice_near = match i {
                    0 => frustum.near,
                    _ => {
                        let before = if i == 1 { frustum.near } else { splits[i - 2] };
                        splits[i - 1] - CASCADE_BLEND * (splits[i - 1] - before)
                    }
                };
                for depth in [slice_near, slice_far] {
                    for (sx, sy) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                        let view_point = Vec3::new(sx * tan_x * depth, sy * tan_y * depth, -depth);
                        let clip = fit.view_proj.project_point3(world.transform_point3(view_point));

                        prop_assert!(clip.x.abs() <= 1.0 - margin + slack, "cascade {i} x {}", clip.x);
                        prop_assert!(clip.y.abs() <= 1.0 - margin + slack, "cascade {i} y {}", clip.y);
                        prop_assert!(
                            (-slack..=1.0 + slack).contains(&clip.z),
                            "cascade {i} depth {}",
                            clip.z
                        );
                    }
                }
            }
        }

        #[test]
        fn cascade_radius_is_independent_of_camera_orientation(
            light in above_horizon(),
            position in any_position(),
            (yaw_a, pitch_a) in (0.0f32..std::f32::consts::TAU, -1.4f32..1.4),
            (yaw_b, pitch_b) in (0.0f32..std::f32::consts::TAU, -1.4f32..1.4),
        ) {
            let frustum = frustum(60.0, SIXTEEN_NINE, 0.1, 800.0);

            let a = light_cascades::<4>(light, frustum, camera_world(position, yaw_a, pitch_a), CASCADE_MAP_SIZE);
            let b = light_cascades::<4>(light, frustum, camera_world(position, yaw_b, pitch_b), CASCADE_MAP_SIZE);

            for (i, (a, b)) in a.iter().zip(&b).enumerate() {
                prop_assert!(a.texel_world > 0.0, "cascade {i} texel {}", a.texel_world);
                prop_assert!(
                    close(a.texel_world, b.texel_world, 1e-5),
                    "cascade {i}: {} vs {}",
                    a.texel_world,
                    b.texel_world
                );
            }
        }

        #[test]
        fn world_origin_lands_on_a_whole_texel_in_every_cascade(
            light in above_horizon(),
            position in any_position(),
            yaw in 0.0f32..std::f32::consts::TAU,
            pitch in -1.4f32..1.4,
        ) {
            let frustum = frustum(60.0, SIXTEEN_NINE, 0.1, 800.0);

            let fits = light_cascades::<4>(light, frustum, camera_world(position, yaw, pitch), CASCADE_MAP_SIZE);

            let half_map = CASCADE_MAP_SIZE as f32 / 2.0;
            for (i, fit) in fits.iter().enumerate() {
                let clip = fit.view_proj.project_point3(Vec3::ZERO);
                for axis in [clip.x * half_map, clip.y * half_map] {
                    prop_assert!((axis - axis.round()).abs() < 0.05, "cascade {i} texel coordinate {axis}");
                }
            }
        }
    }

    #[test]
    fn first_sun_cascade_is_under_3_cm_per_texel_past_20_m() {
        let light = Vec3::new(0.3, 0.8, 0.2).normalize();

        for (name, (fov, near, far)) in [("searchlight", SEARCHLIGHT), ("flagstone", FLAGSTONE)] {
            for aspect in [SIXTEEN_NINE, TWENTY_ONE_NINE] {
                let frustum = frustum(fov, aspect, near, far);

                let fits = light_cascades::<{ SUN_CASCADES }>(
                    light,
                    frustum,
                    Mat4::IDENTITY,
                    CASCADE_MAP_SIZE,
                );

                let splits = practical_splits::<{ SUN_CASCADES }>(frustum.near, frustum.far);
                assert!(
                    splits[0] > 20.0,
                    "{name} {aspect}: first split {}",
                    splits[0]
                );
                assert!(
                    fits[0].texel_world <= 0.03,
                    "{name} {aspect}: {} m per texel",
                    fits[0].texel_world
                );
            }
        }
    }

    #[test]
    fn frustum_from_projection_recovers_near_far_and_corner_slope() {
        for (fov, aspect, near, far) in [
            (75.0f32, SIXTEEN_NINE, 0.05f32, 500.0f32),
            (45.0, TWENTY_ONE_NINE, 0.1, 1500.0),
            (90.0, 1.0, 1.0, 100.0),
        ] {
            let slope = (fov.to_radians() / 2.0).tan() * (1.0 + aspect * aspect).sqrt();

            let finite = frustum(fov, aspect, near, far);
            let infinite = CameraFrustum::from_projection(perspective_infinite(
                fov.to_radians(),
                aspect,
                near,
            ))
            .expect("an infinite perspective has a frustum");

            assert!(
                close(finite.near, near, 1e-3),
                "near {} vs {near}",
                finite.near
            );
            assert!(close(finite.far, far, 1e-3), "far {} vs {far}", finite.far);
            assert!(
                close(finite.corner_slope, slope, 1e-3),
                "slope {} vs {slope}",
                finite.corner_slope
            );
            assert!(
                close(infinite.near, near, 1e-3),
                "near {} vs {near}",
                infinite.near
            );
            assert!(
                infinite.far.is_infinite() && infinite.far > 0.0,
                "far {}",
                infinite.far
            );
            assert!(
                close(infinite.corner_slope, slope, 1e-3),
                "slope {} vs {slope}",
                infinite.corner_slope
            );
        }
    }

    #[test]
    fn frustum_from_projection_refuses_an_orthographic_projection() {
        let proj = orthographic(-10.0, 10.0, -10.0, 10.0, 0.1, 100.0);

        assert_eq!(CameraFrustum::from_projection(proj), None);
    }

    #[test]
    fn shadow_distance_is_capped_by_the_camera_far_plane() {
        let near = 0.1;

        let short = practical_splits::<4>(near, 300.0);
        let long = practical_splits::<4>(near, 5000.0);

        assert!(close(short[3], 300.0, 1e-4), "{short:?}");
        assert!(close(long[3], SHADOW_DISTANCE, 1e-4), "{long:?}");
        let light = Vec3::new(0.2, 0.9, 0.1).normalize();
        let near_fit = light_cascades::<4>(
            light,
            frustum(60.0, SIXTEEN_NINE, near, 60.0),
            Mat4::IDENTITY,
            CASCADE_MAP_SIZE,
        );
        let far_fit = light_cascades::<4>(
            light,
            frustum(60.0, SIXTEEN_NINE, near, 5000.0),
            Mat4::IDENTITY,
            CASCADE_MAP_SIZE,
        );
        assert!(
            near_fit[3].texel_world < far_fit[3].texel_world / 2.0,
            "a 60 m far plane fits a smaller last cascade ({}) than 5000 m ({})",
            near_fit[3].texel_world,
            far_fit[3].texel_world
        );
    }

    #[test]
    fn overhead_light_gives_finite_cascades() {
        let position = Vec3::new(12.0, 3.0, -7.0);
        let frustum = frustum(60.0, SIXTEEN_NINE, 0.1, 800.0);

        for light in [Vec3::Y, Vec3::NEG_Y] {
            let fits = light_cascades::<4>(
                light,
                frustum,
                Mat4::from_translation(position),
                CASCADE_MAP_SIZE,
            );

            for (i, fit) in fits.iter().enumerate() {
                assert!(
                    fit.view_proj.to_cols_array().iter().all(|v| v.is_finite()),
                    "light {light}, cascade {i}: {:?}",
                    fit.view_proj
                );
                assert!(
                    fit.texel_world.is_finite() && fit.texel_world > 0.0,
                    "{}",
                    fit.texel_world
                );
            }
            let ahead = position + Vec3::NEG_Z * 1.0;
            let clip = fits[0].view_proj.project_point3(ahead);
            assert!(
                clip.x.abs() < 1.0 && clip.y.abs() < 1.0,
                "light {light}: {clip:?}"
            );
            assert!((0.0..=1.0).contains(&clip.z), "light {light}: {clip:?}");
        }
    }
}
