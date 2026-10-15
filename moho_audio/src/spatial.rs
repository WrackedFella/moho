//! Distance attenuation and the listener, as pure functions over glam types.

use glam::{Mat4, Quat, Vec3};

/// Distance in metres up to which a sound plays at full volume.
pub const REFERENCE_DISTANCE: f32 = 2.0;

/// Distance in metres past which a sound is not played.
pub const MAX_AUDIBLE_DISTANCE: f32 = 100.0;

/// Where the listener is and which way it faces (default: origin, facing -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub orientation: Quat,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
        }
    }
}

impl Listener {
    /// Read the listener from a camera view matrix (world to eye space).
    pub fn from_view(view: Mat4) -> Self {
        let (_, orientation, position) = view.inverse().to_scale_rotation_translation();
        Self {
            position,
            orientation,
        }
    }

    /// The listener's right-hand direction in world space.
    pub fn right(&self) -> Vec3 {
        self.orientation * Vec3::X
    }
}

/// Per-ear volume multipliers for a panned sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoGain {
    pub left: f32,
    pub right: f32,
}

/// Constant-power pan of a sound at `source` as heard by `listener`.
pub fn stereo_pan(listener: &Listener, source: Vec3) -> StereoGain {
    let side = (source - listener.position)
        .normalize_or_zero()
        .dot(listener.right());
    let angle = (side + 1.0) * std::f32::consts::FRAC_PI_4;
    StereoGain {
        left: angle.cos(),
        right: angle.sin(),
    }
}

/// Volume multiplier for a sound `distance` metres from the listener, or `None` past
/// [`MAX_AUDIBLE_DISTANCE`] or for a non-finite distance.
pub fn distance_gain(distance: f32) -> Option<f32> {
    if !distance.is_finite() || distance > MAX_AUDIBLE_DISTANCE {
        None
    } else if distance <= REFERENCE_DISTANCE {
        Some(1.0)
    } else {
        Some(REFERENCE_DISTANCE / distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "{actual} != expected {expected}"
        );
    }

    #[test]
    fn distance_gain_follows_inverse_distance_to_the_cutoff() {
        let rows = [
            (0.0, Some(1.0)),
            (REFERENCE_DISTANCE, Some(1.0)),
            (4.0, Some(0.5)),
            (MAX_AUDIBLE_DISTANCE, Some(0.02)),
            (MAX_AUDIBLE_DISTANCE + 0.01, None),
        ];

        for (distance, expected) in rows {
            let gain = distance_gain(distance);

            match (gain, expected) {
                (Some(actual), Some(expected)) => assert_close(actual, expected),
                (actual, expected) => assert_eq!(actual, expected, "at {distance} m"),
            }
        }
    }

    #[test]
    fn distance_gain_of_a_non_finite_distance_is_none() {
        for distance in [f32::NAN, f32::INFINITY] {
            let gain = distance_gain(distance);

            assert_eq!(gain, None, "at {distance} m");
        }
    }

    proptest! {
        #[test]
        fn distance_gain_never_rises_with_distance(a in 0.0f32..200.0, b in 0.0f32..200.0) {
            let (near, far) = if a <= b { (a, b) } else { (b, a) };

            let near_gain = distance_gain(near);
            let far_gain = distance_gain(far);

            for gain in [near_gain, far_gain].into_iter().flatten() {
                prop_assert!(gain > 0.0 && gain <= 1.0, "gain {gain}");
            }
            match (near_gain, far_gain) {
                (Some(n), Some(f)) => prop_assert!(f <= n, "{far} m gain {f} > {near} m gain {n}"),
                (None, Some(f)) => prop_assert!(false, "{near} m silent but {far} m plays at {f}"),
                _ => {}
            }
        }
    }

    #[test]
    fn default_listener_is_at_the_origin_facing_negative_z() {
        let listener = Listener::default();

        assert_eq!(listener.position, Vec3::ZERO);
        let facing = listener.orientation * Vec3::NEG_Z;
        assert!((facing - Vec3::NEG_Z).length() < 1e-6, "{facing}");
    }

    const NORTH: Vec3 = Vec3::NEG_Z;
    const EAST: Vec3 = Vec3::X;

    fn facing_south() -> Listener {
        Listener {
            orientation: Quat::from_rotation_y(std::f32::consts::PI),
            ..Listener::default()
        }
    }

    #[track_caller]
    fn assert_gain(gain: StereoGain, left: f32, right: f32) {
        assert_close(gain.left, left);
        assert_close(gain.right, right);
    }

    fn power(gain: StereoGain) -> f32 {
        gain.left * gain.left + gain.right * gain.right
    }

    #[test]
    fn listener_right_is_the_orientation_x_axis() {
        use std::f32::consts::{FRAC_PI_2, PI};
        let rows = [
            ("facing south", Quat::from_rotation_y(PI), Vec3::NEG_X),
            ("facing east", Quat::from_rotation_y(-FRAC_PI_2), Vec3::Z),
            (
                "pitched straight down",
                Quat::from_rotation_x(-FRAC_PI_2),
                Vec3::X,
            ),
        ];

        for (name, orientation, expected) in rows {
            let listener = Listener {
                orientation,
                ..Listener::default()
            };

            let right = listener.right();

            assert!((right - expected).length() < 1e-5, "{name}: {right}");
        }
    }

    #[test]
    fn diagonal_sound_follows_the_cos_sin_law() {
        use std::f32::consts::FRAC_PI_2;
        let rows = [
            (
                "north-east",
                Vec3::new(10.0, 0.0, -10.0),
                Listener::default(),
            ),
            (
                "right and up",
                Vec3::new(10.0, 10.0, 0.0),
                Listener::default(),
            ),
            (
                "east, listener pitched straight down",
                EAST * 10.0,
                Listener {
                    orientation: Quat::from_rotation_x(-FRAC_PI_2),
                    ..Listener::default()
                },
            ),
        ];
        let expected = [(0.22801, 0.97366), (0.22801, 0.97366), (0.0, 1.0)];

        for ((name, source, listener), (left, right)) in rows.into_iter().zip(expected) {
            let gain = stereo_pan(&listener, source);

            assert!(
                (gain.left - left).abs() < 1e-4 && (gain.right - right).abs() < 1e-4,
                "{name}: {gain:?}, want ({left}, {right})"
            );
        }
    }

    #[test]
    fn sound_to_a_side_is_louder_in_that_ear() {
        let rows = [("east", EAST, 0.0, 1.0), ("west", -EAST, 1.0, 0.0)];

        for (side, direction, left, right) in rows {
            let gain = stereo_pan(&Listener::default(), direction * 10.0);

            assert_gain(gain, left, right);
            assert!(
                (gain.right > gain.left) == (right > left),
                "{side}: {gain:?}"
            );
        }
    }

    #[test]
    fn panning_follows_facing_not_world_axes() {
        let gain = stereo_pan(&facing_south(), EAST * 10.0);

        assert_gain(gain, 1.0, 0.0);
    }

    #[test]
    fn pan_is_measured_from_the_listener_position() {
        let listener = Listener {
            position: Vec3::new(5.0, 0.0, 5.0),
            ..Listener::default()
        };

        let gain = stereo_pan(&listener, Vec3::new(15.0, 0.0, 5.0));

        assert_gain(gain, 0.0, 1.0);
    }

    #[test]
    fn ahead_behind_and_on_listener_are_centred() {
        let rows = [
            ("10 m north", NORTH * 10.0),
            ("10 m south", -NORTH * 10.0),
            ("10 m above", Vec3::Y * 10.0),
            ("at the listener", Vec3::ZERO),
        ];

        for (where_, source) in rows {
            let gain = stereo_pan(&Listener::default(), source);

            assert!(gain.left.is_finite() && gain.right.is_finite(), "{where_}");
            assert_gain(
                gain,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            );
        }
    }

    #[test]
    fn pan_keeps_total_power() {
        let listener = Listener::default();

        let east = stereo_pan(&listener, EAST * 10.0);
        let north = stereo_pan(&listener, NORTH * 10.0);

        assert_close(power(east), 1.0);
        assert_close(power(north), 1.0);
        assert_close(power(east), power(north));
    }

    proptest! {
        #[test]
        fn pan_power_is_one_in_every_direction(
            x in -50.0f32..50.0,
            y in -50.0f32..50.0,
            z in -50.0f32..50.0,
            yaw in -3.2f32..3.2,
            pitch in -1.5f32..1.5,
        ) {
            let listener = Listener {
                position: Vec3::new(1.0, 2.0, 3.0),
                orientation: Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0),
            };

            let gain = stereo_pan(&listener, Vec3::new(x, y, z));

            prop_assert!((power(gain) - 1.0).abs() < 1e-4, "{gain:?}");
            prop_assert!(gain.left >= -1e-6 && gain.left <= 1.0 + 1e-6, "{gain:?}");
            prop_assert!(gain.right >= -1e-6 && gain.right <= 1.0 + 1e-6, "{gain:?}");
        }
    }

    #[test]
    fn listener_from_view_takes_eye_and_facing() {
        let eye = Vec3::new(3.0, 50.0, -7.0);
        let target = Vec3::new(10.0, 20.0, 4.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);

        let listener = Listener::from_view(view);

        assert!((listener.position - eye).length() < 1e-4, "{listener:?}");
        let facing = (target - eye).normalize();
        let rotated = listener.orientation * Vec3::NEG_Z;
        assert!(
            (rotated - facing).length() < 1e-4,
            "faces {rotated}, want {facing}"
        );
        let right = listener.orientation * Vec3::X;
        let level_right = facing.cross(Vec3::Y).normalize();
        assert!(
            (right - level_right).length() < 1e-4,
            "right is {right}, want {level_right} (no roll)"
        );
    }
}
