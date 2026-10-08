use crate::controller::{CameraMode, ControllerInput, PlayerController, controller_to_camera};
use crate::game_clock::GameClock;
use glam::{Mat4, Vec3};

/// Owns the player controller, its pending input and the game clock, and steps them together.
#[derive(Debug, Clone)]
pub struct SimulationController {
    pub player_controller: PlayerController,
    pub controller_input: ControllerInput,
    pub game_clock: GameClock,
}

impl SimulationController {
    pub fn new(position: Vec3) -> Self {
        Self {
            player_controller: PlayerController::new(position),
            controller_input: ControllerInput::default(),
            game_clock: GameClock::default(),
        }
    }

    pub fn with_clock(
        position: Vec3,
        day_length: f32,
        night_length: f32,
        initial_time: f32,
    ) -> Self {
        Self {
            player_controller: PlayerController::new(position),
            controller_input: ControllerInput::default(),
            game_clock: GameClock::new(initial_time, day_length, night_length),
        }
    }

    pub fn controller_input_mut(&mut self) -> &mut ControllerInput {
        &mut self.controller_input
    }

    /// Applies the pending input, advances the game clock by `dt`, and returns
    /// the resulting camera `(view, proj, cam_pos)`.
    pub fn apply_input(&mut self, dt: f32) -> (Mat4, Mat4, Vec3) {
        self.player_controller
            .apply_input(&self.controller_input, dt);

        self.game_clock.tick(dt);

        controller_to_camera(&self.player_controller)
    }

    pub fn position(&self) -> Vec3 {
        self.player_controller.position
    }

    pub fn yaw_pitch(&self) -> (f32, f32) {
        (self.player_controller.yaw, self.player_controller.pitch)
    }

    pub fn set_position_yaw_pitch(&mut self, pos: Vec3, yaw: f32, pitch: f32) {
        self.player_controller.position = pos;
        self.player_controller.yaw = yaw;
        self.player_controller.pitch = pitch;
    }

    pub fn set_camera_mode(&mut self, mode: CameraMode) {
        self.player_controller.camera_mode = mode;
    }

    pub fn camera_mode(&self) -> CameraMode {
        self.player_controller.camera_mode
    }

    pub fn game_clock(&self) -> &GameClock {
        &self.game_clock
    }

    pub fn game_clock_mut(&mut self) -> &mut GameClock {
        &mut self.game_clock
    }

    pub fn celestial_directions(&self) -> (Vec3, Vec3) {
        self.game_clock.celestial_directions()
    }

    /// Current time of day in hours, `0.0..24.0`.
    pub fn time_of_day(&self) -> f32 {
        self.game_clock.time_of_day()
    }

    /// Sets the time of day in hours; values outside `0.0..24.0` wrap.
    pub fn set_time_of_day(&mut self, time: f32) {
        self.game_clock.set_time(time);
    }

    /// Points the camera at `target`: FPS mode recomputes yaw/pitch, isometric
    /// mode recentres `rts_look_target`.
    pub fn look_at(&mut self, target: Vec3) {
        self.player_controller.look_at(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn assert_vec3_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < EPS,
            "expected {expected:?}, got {actual:?}"
        );
    }

    mod input {
        use super::*;

        #[test]
        fn apply_input_when_moving_forward_advances_position_and_returns_matching_camera() {
            let mut sim = SimulationController::new(Vec3::new(1.0, 2.0, 3.0));
            sim.controller_input_mut().forward = 1.0;

            let (view, proj, cam_pos) = sim.apply_input(0.5);

            assert_vec3_near(sim.position(), Vec3::new(1.0, 2.0, 5.0));
            assert_vec3_near(cam_pos, Vec3::new(1.0, 2.0, 5.0));
            let (expected_view, expected_proj, expected_pos) =
                controller_to_camera(&sim.player_controller);
            assert_eq!(view, expected_view);
            assert_eq!(proj, expected_proj);
            assert_eq!(cam_pos, expected_pos);
        }

        #[test]
        fn apply_input_when_ticked_advances_game_clock() {
            let mut sim = SimulationController::with_clock(Vec3::ZERO, 120.0, 120.0, 12.0);

            sim.apply_input(1.0);

            assert!((sim.time_of_day() - 12.1).abs() < EPS);
            assert_eq!(sim.game_clock().elapsed_seconds(), 1.0);
        }
    }

    mod pose {
        use super::*;

        #[test]
        fn set_position_yaw_pitch_when_called_is_reflected_by_getters() {
            let mut sim = SimulationController::new(Vec3::ZERO);

            sim.set_position_yaw_pitch(Vec3::new(3.0, 4.0, 5.0), 0.7, 0.3);

            assert_eq!(sim.position(), Vec3::new(3.0, 4.0, 5.0));
            assert_eq!(sim.yaw_pitch(), (0.7, 0.3));
        }

        #[test]
        fn look_at_when_first_person_turns_yaw_and_pitch_toward_target() {
            let mut sim = SimulationController::new(Vec3::ZERO);

            sim.look_at(Vec3::new(1.0, 1.0, 1.0));

            let (yaw, pitch) = sim.yaw_pitch();
            assert!((yaw - std::f32::consts::FRAC_PI_4).abs() < EPS);
            assert!((pitch - (1.0_f32 / 3.0_f32.sqrt()).asin()).abs() < EPS);
        }

        #[test]
        fn look_at_when_isometric_sets_rts_look_target() {
            let mut sim = SimulationController::new(Vec3::ZERO);
            sim.set_camera_mode(CameraMode::Isometric);
            assert_eq!(sim.camera_mode(), CameraMode::Isometric);

            sim.look_at(Vec3::new(7.0, 0.0, -3.0));

            assert_eq!(
                sim.player_controller.rts_look_target,
                Vec3::new(7.0, 0.0, -3.0)
            );
            assert_eq!(sim.yaw_pitch(), (0.0, 0.0));
        }
    }

    mod clock {
        use super::*;

        #[test]
        fn with_clock_when_given_initial_time_starts_at_that_time() {
            let sim = SimulationController::with_clock(Vec3::ZERO, 100.0, 50.0, 9.25);

            assert_eq!(sim.time_of_day(), 9.25);
            assert_eq!(sim.game_clock().time_of_day(), 9.25);
        }

        #[test]
        fn game_clock_mut_when_time_set_is_observed_through_accessors() {
            let mut sim = SimulationController::new(Vec3::ZERO);

            sim.game_clock_mut().set_time(15.5);

            assert_eq!(sim.game_clock().time_of_day(), 15.5);
            assert_eq!(sim.time_of_day(), 15.5);
        }

        #[test]
        fn set_time_of_day_when_out_of_range_wraps() {
            let mut sim = SimulationController::new(Vec3::ZERO);

            sim.set_time_of_day(30.5);

            assert!((sim.time_of_day() - 6.5).abs() < EPS);
        }

        #[test]
        fn celestial_directions_when_noon_matches_clock_and_has_high_sun() {
            let sim = SimulationController::with_clock(Vec3::ZERO, 600.0, 420.0, 12.0);

            let (sun, moon) = sim.celestial_directions();

            assert_eq!((sun, moon), sim.game_clock().celestial_directions());
            assert!(sun.y > 0.7);
            assert_eq!(moon, Vec3::NEG_Y);
        }
    }
}
