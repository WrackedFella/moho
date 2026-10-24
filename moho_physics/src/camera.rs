//! First-person camera built from a character's eye and look direction.

use crate::movement::turned_look;
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

/// Eye (`position + eye`) blended between the last two ticks by `alpha` (0 = previous,
/// 1 = current); look from the current tick turned by `pending` (yaw, pitch deltas), pitch
/// clamped at [`PITCH_LIMIT`](crate::PITCH_LIMIT).
pub fn first_person_view(
    previous: &CharacterState,
    current: &CharacterState,
    alpha: f32,
    pending: (f32, f32),
) -> FirstPersonView {
    let (yaw, pitch) = turned_look((current.yaw, current.pitch), pending);
    let (yaw_sin, yaw_cos) = yaw.sin_cos();
    let (pitch_sin, pitch_cos) = pitch.sin_cos();
    FirstPersonView {
        eye: (previous.position + previous.eye).lerp(current.position + current.eye, alpha),
        forward: Vec3::new(yaw_sin * pitch_cos, pitch_sin, yaw_cos * pitch_cos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PITCH_LIMIT;

    const EPS: f32 = 1e-5;

    /// A standing state whose eye is 1.6 m above `position`.
    fn state_at(position: Vec3, yaw: f32, pitch: f32) -> CharacterState {
        CharacterState {
            position,
            velocity: Vec3::ZERO,
            yaw,
            pitch,
            grounded: true,
            jump_held: false,
            eye: Vec3::Y * 1.6,
        }
    }

    #[test]
    fn eye_is_position_plus_state_eye_at_alpha_one() {
        let at_origin = CharacterState {
            eye: Vec3::new(0.1, 1.4, -0.2),
            ..state_at(Vec3::new(2.0, 1.0, -3.0), 0.0, 0.0)
        };

        let view = first_person_view(&at_origin, &at_origin, 1.0, (0.0, 0.0));

        assert!(
            view.eye.distance(Vec3::new(2.1, 2.4, -3.2)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn eye_blends_position_plus_eye_between_ticks() {
        let previous = CharacterState {
            eye: Vec3::new(0.0, 1.0, 0.0),
            ..state_at(Vec3::ZERO, 0.0, 0.0)
        };
        let current = CharacterState {
            eye: Vec3::new(0.0, 2.0, 0.0),
            ..state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0)
        };

        let view = first_person_view(&previous, &current, 0.5, (0.0, 0.0));

        assert!(
            view.eye.distance(Vec3::new(0.5, 1.5, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn forward_follows_yaw_and_pitch() {
        let pitch = 30.0_f32.to_radians();
        let looking_up = state_at(Vec3::new(2.0, 1.0, -3.0), 0.0, pitch);

        let view = first_person_view(&looking_up, &looking_up, 1.0, (0.0, 0.0));

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

        let view = first_person_view(&previous, &current, 0.5, (0.0, 0.0));

        assert!(
            view.eye.distance(Vec3::new(0.5, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn zero_alpha_puts_eye_at_previous_tick() {
        let previous = state_at(Vec3::ZERO, 0.0, 0.0);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 0.0, (0.0, 0.0));

        assert!(
            view.eye.distance(Vec3::new(0.0, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }

    #[test]
    fn forward_tilts_a_turned_look_up_by_pitch() {
        let pitch = 30.0_f32.to_radians();
        let facing_x = state_at(Vec3::ZERO, 90.0_f32.to_radians(), pitch);

        let view = first_person_view(&facing_x, &facing_x, 1.0, (0.0, 0.0));

        let expected = Vec3::new(pitch.cos(), pitch.sin(), 0.0);
        assert!(view.forward.distance(expected) < EPS, "{view:?}");
    }

    #[test]
    fn look_ignores_previous_tick() {
        let previous = state_at(Vec3::ZERO, 1.0, -0.4);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 0.5, (0.0, 0.0));

        assert!(view.forward.distance(Vec3::Z) < EPS, "{view:?}");
    }

    #[test]
    fn pending_yaw_turns_the_view_from_the_current_tick() {
        let current = state_at(Vec3::ZERO, 0.25, 0.1);

        let view = first_person_view(&current, &current, 1.0, (0.5, 0.0));

        let expected = state_at(Vec3::ZERO, 0.75, 0.1);
        let want = first_person_view(&expected, &expected, 1.0, (0.0, 0.0));
        assert!(view.forward.distance(want.forward) < EPS, "{view:?}");
    }

    #[test]
    fn pending_pitch_tilts_the_view_up_and_down() {
        let level = state_at(Vec3::ZERO, 0.0, 0.0);

        let up = first_person_view(&level, &level, 1.0, (0.0, 0.3));
        let down = first_person_view(&level, &level, 1.0, (0.0, -0.3));

        assert!(
            up.forward
                .distance(Vec3::new(0.0, 0.3_f32.sin(), 0.3_f32.cos()))
                < EPS
        );
        assert!(
            down.forward
                .distance(Vec3::new(0.0, -0.3_f32.sin(), 0.3_f32.cos()))
                < EPS
        );
    }

    #[test]
    fn pending_pitch_is_clamped_at_the_limit() {
        let up_forward = Vec3::new(0.0, PITCH_LIMIT.sin(), PITCH_LIMIT.cos());
        let down_forward = Vec3::new(0.0, -PITCH_LIMIT.sin(), PITCH_LIMIT.cos());
        let cases = [
            (PITCH_LIMIT, 0.5, up_forward),
            (-PITCH_LIMIT, -0.5, down_forward),
            (PITCH_LIMIT - 0.1, 2.0, up_forward),
            (-PITCH_LIMIT + 0.1, -2.0, down_forward),
        ];

        for (pitch, pending_pitch, expected) in cases {
            let current = state_at(Vec3::ZERO, 0.0, pitch);

            let view = first_person_view(&current, &current, 1.0, (0.0, pending_pitch));

            assert_eq!(view.forward, expected, "from {pitch} by {pending_pitch}");
        }
    }

    #[test]
    fn pending_look_matches_a_step_with_the_same_deltas_exactly() {
        let start = state_at(Vec3::ZERO, 0.37, 1.2);
        let (yaw_delta, pitch_delta) = (0.123_456_7_f32, 0.9_f32);
        let mut stepped = start;
        stepped.yaw += yaw_delta;
        stepped.pitch = (stepped.pitch + pitch_delta).clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let drawn = first_person_view(&start, &start, 1.0, (yaw_delta, pitch_delta));
        let landed = first_person_view(&stepped, &stepped, 1.0, (0.0, 0.0));

        assert_eq!(drawn.forward, landed.forward);
    }

    #[test]
    fn pending_look_leaves_the_eye_alone() {
        let previous = state_at(Vec3::ZERO, 0.0, 0.0);
        let current = state_at(Vec3::new(1.0, 0.0, 0.0), 0.0, 0.0);

        let view = first_person_view(&previous, &current, 0.5, (1.0, 0.5));

        assert!(
            view.eye.distance(Vec3::new(0.5, 1.6, 0.0)) < EPS,
            "{view:?}"
        );
    }
}
