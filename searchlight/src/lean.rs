//! View maths for a lean: the sideways eye offset and the rolled view matrix.

use glam::{Mat4, Vec3};

/// The horizontal eye offset for a lean of `shift` metres (positive right) at `yaw`.
/// Right is `-X` at yaw 0.
pub fn lean_offset(_yaw: f32, _shift: f32) -> Vec3 {
    Vec3::ZERO
}

/// A right-handed view matrix looking from `eye` along `forward`, with up tipped about
/// `forward` by `roll` radians; positive tips the top of the view toward the right.
pub fn rolled_view_matrix(eye: Vec3, forward: Vec3, _roll: f32) -> Mat4 {
    glam::camera::rh::view::look_at_mat4(eye, eye + forward, Vec3::Y)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    const ROLL: f32 = 0.2;

    /// The view's up axis in world space.
    fn up_of(view: Mat4) -> Vec3 {
        view.transpose().col(1).truncate()
    }

    fn forward_of(view: Mat4) -> Vec3 {
        -view.transpose().col(2).truncate()
    }

    fn eye_of(view: Mat4) -> Vec3 {
        view.inverse().w_axis.truncate()
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-4
    }

    #[test]
    fn a_right_shift_at_yaw_zero_moves_toward_negative_x() {
        let offset = lean_offset(0.0, 0.35);

        assert!(close(offset, Vec3::new(-0.35, 0.0, 0.0)), "{offset:?}");
    }

    #[test]
    fn a_left_shift_at_yaw_zero_moves_toward_positive_x() {
        let offset = lean_offset(0.0, -0.35);

        assert!(close(offset, Vec3::new(0.35, 0.0, 0.0)), "{offset:?}");
    }

    #[test]
    fn the_offset_turns_with_the_yaw_and_stays_horizontal() {
        let quarter = lean_offset(FRAC_PI_2, 0.35);
        let half = lean_offset(2.0 * FRAC_PI_2, 0.35);

        assert!(close(quarter, Vec3::new(0.0, 0.0, 0.35)), "{quarter:?}");
        assert!(close(half, Vec3::new(0.35, 0.0, 0.0)), "{half:?}");
    }

    #[test]
    fn no_shift_is_no_offset() {
        assert!(close(lean_offset(1.3, 0.0), Vec3::ZERO));
    }

    #[test]
    fn a_right_roll_tips_the_view_up_toward_the_right() {
        let eye = Vec3::new(1.0, 2.0, 3.0);

        let view = rolled_view_matrix(eye, Vec3::Z, ROLL);

        let up = up_of(view);
        assert!(up.x < 0.0, "right is -X at yaw 0: {up:?}");
        assert!((up.x + ROLL.sin()).abs() < 1e-4, "{up:?}");
        assert!((up.y - ROLL.cos()).abs() < 1e-4, "{up:?}");
        assert!(close(forward_of(view), Vec3::Z));
        assert!(close(eye_of(view), eye));
    }

    #[test]
    fn a_left_roll_tips_the_view_up_toward_the_left() {
        let eye = Vec3::new(1.0, 2.0, 3.0);

        let view = rolled_view_matrix(eye, Vec3::Z, -ROLL);

        let up = up_of(view);
        assert!(up.x > 0.0, "left is +X at yaw 0: {up:?}");
        assert!((up.x - ROLL.sin()).abs() < 1e-4, "{up:?}");
        assert!((up.y - ROLL.cos()).abs() < 1e-4, "{up:?}");
        assert!(close(forward_of(view), Vec3::Z));
        assert!(close(eye_of(view), eye));
    }

    #[test]
    fn no_roll_is_the_plain_level_view() {
        let eye = Vec3::new(0.0, 1.8, 0.0);
        let forward = Vec3::new(0.6, -0.2, 0.8).normalize();

        let view = rolled_view_matrix(eye, forward, 0.0);

        let plain = glam::camera::rh::view::look_at_mat4(eye, eye + forward, Vec3::Y);
        assert!(view.abs_diff_eq(plain, 1e-5), "{view:?} vs {plain:?}");
    }

    #[test]
    fn a_roll_turns_a_pitched_view_by_exactly_the_roll_about_forward() {
        let eye = Vec3::new(0.0, 1.8, 0.0);
        let forward = Vec3::new(0.0, -0.5, 1.0).normalize();
        let level = rolled_view_matrix(eye, forward, 0.0);

        let rolled = rolled_view_matrix(eye, forward, ROLL);

        let angle = up_of(level).angle_between(up_of(rolled));
        assert!((angle - ROLL).abs() < 1e-4, "{angle}");
        assert!(up_of(rolled).dot(forward).abs() < 1e-4);
        assert!(close(forward_of(rolled), forward));
    }
}
