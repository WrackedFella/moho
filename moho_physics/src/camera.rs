//! First-person camera built from a character's eye and look direction.

use crate::world::CharacterState;
use glam::{Mat4, Vec3};

/// Where a first-person camera sits and looks for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstPersonView {
    pub eye: Vec3,
    pub forward: Vec3,
}

impl FirstPersonView {
    /// Right-handed view matrix, +Y up.
    pub fn view_matrix(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye, self.eye + self.forward, Vec3::Y)
    }
}

/// Eye blended between the last two ticks by `alpha` (0 = previous, 1 = current);
/// look from the current tick.
pub fn first_person_view(
    previous: &CharacterState,
    current: &CharacterState,
    eye_height: f32,
    alpha: f32,
) -> FirstPersonView {
    let (yaw_sin, yaw_cos) = current.yaw.sin_cos();
    let (pitch_sin, pitch_cos) = current.pitch.sin_cos();
    FirstPersonView {
        eye: previous.position.lerp(current.position, alpha) + Vec3::Y * eye_height,
        forward: Vec3::new(yaw_sin * pitch_cos, pitch_sin, yaw_cos * pitch_cos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn state_at(position: Vec3, yaw: f32, pitch: f32) -> CharacterState {
        CharacterState {
            position,
            velocity: Vec3::ZERO,
            yaw,
            pitch,
            grounded: true,
        }
    }

    #[test]
    fn eye_is_position_plus_eye_height_at_alpha_one() {
        let at_origin = state_at(Vec3::ZERO, 0.0, 0.0);

        let view = first_person_view(&at_origin, &at_origin, 1.6, 1.0);

        assert!(
            view.eye.distance(Vec3::new(0.0, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn forward_follows_yaw_and_pitch() {
        let pitch = 30.0_f32.to_radians();
        let looking_up = state_at(Vec3::new(2.0, 1.0, -3.0), 0.0, pitch);

        let view = first_person_view(&looking_up, &looking_up, 1.6, 1.0);

        let expected = Vec3::new(0.0, pitch.sin(), pitch.cos());
        assert!(view.forward.distance(expected) < EPS, "{view:?}");
        let matrix = view.view_matrix();
        assert!(
            matrix.transform_point3(view.eye).length() < EPS,
            "the eye is the view origin"
        );
        let ahead = matrix.transform_vector3(view.forward);
        assert!(
            ahead.distance(Vec3::NEG_Z) < EPS,
            "forward is -Z in view space: {ahead:?}"
        );
    }

    #[test]
    fn half_alpha_puts_eye_halfway_between_ticks() {
        let previous = state_at(Vec3::ZERO, 0.0, 0.0);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 1.6, 0.5);

        assert!(
            view.eye.distance(Vec3::new(0.5, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn zero_alpha_puts_eye_at_previous_tick() {
        let previous = state_at(Vec3::ZERO, 0.0, 0.0);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 1.6, 0.0);

        assert!(
            view.eye.distance(Vec3::new(0.0, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn forward_tilts_a_turned_look_up_by_pitch() {
        let pitch = 30.0_f32.to_radians();
        let facing_x = state_at(Vec3::ZERO, 90.0_f32.to_radians(), pitch);

        let view = first_person_view(&facing_x, &facing_x, 0.0, 1.0);

        let expected = Vec3::new(pitch.cos(), pitch.sin(), 0.0);
        assert!(view.forward.distance(expected) < EPS, "{view:?}");
    }

    #[test]
    fn look_ignores_previous_tick() {
        let previous = state_at(Vec3::ZERO, 1.0, -0.4);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 0.0, 0.5);

        assert!(view.forward.distance(Vec3::Z) < EPS, "{view:?}");
    }
}
