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
        todo!("origin, facing -Z")
    }
}

impl Listener {
    /// Read the listener from a camera view matrix (world to eye space).
    pub fn from_view(view: Mat4) -> Self {
        let _ = view;
        todo!("invert the view")
    }
}

/// Volume multiplier for a sound `distance` metres from the listener, or `None` past
/// [`MAX_AUDIBLE_DISTANCE`].
pub fn distance_gain(distance: f32) -> Option<f32> {
    let _ = distance;
    todo!("inverse distance, clamped")
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

    #[test]
    fn listener_from_view_takes_eye_and_facing() {
        let eye = Vec3::new(3.0, 50.0, -7.0);
        let target = Vec3::new(10.0, 20.0, 4.0);
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);

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
