use crate::controller::{CameraMode, ControllerInput, PlayerController, controller_to_camera};
use crate::game_clock::GameClock;
use glam::{Mat4, Vec3};

/// Controller-focused simulation wrapper owning a PlayerController and its input state.
#[derive(Debug, Clone)]
pub struct SimulationController {
    pub player_controller: PlayerController,
    pub controller_input: ControllerInput,
    pub game_clock: GameClock,
}

impl SimulationController {
    /// Create a new SimulationController with a controller positioned at `position`.
    pub fn new(position: Vec3) -> Self {
        Self {
            player_controller: PlayerController::new(position),
            controller_input: ControllerInput::default(),
            game_clock: GameClock::default(),
        }
    }

    /// Create a new SimulationController with custom clock configuration.
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

    /// Mutable access to the inner ControllerInput so callers can set axis/deltas.
    pub fn controller_input_mut(&mut self) -> &mut ControllerInput {
        &mut self.controller_input
    }

    /// Apply the current controller input for this tick and return the camera
    /// tuple (view, proj, cam_pos) after applying movement/look deltas.
    /// Also advances the game clock.
    pub fn apply_input(&mut self, dt: f32) -> (Mat4, Mat4, Vec3) {
        self.player_controller
            .apply_input(&self.controller_input, dt);

        // Advance game clock
        self.game_clock.tick(dt);

        controller_to_camera(&self.player_controller)
    }

    /// Return the controller's current position
    pub fn position(&self) -> Vec3 {
        self.player_controller.position
    }

    /// Return yaw and pitch
    pub fn yaw_pitch(&self) -> (f32, f32) {
        (self.player_controller.yaw, self.player_controller.pitch)
    }

    /// Set position, yaw, and pitch together
    pub fn set_position_yaw_pitch(&mut self, pos: Vec3, yaw: f32, pitch: f32) {
        self.player_controller.position = pos;
        self.player_controller.yaw = yaw;
        self.player_controller.pitch = pitch;
    }

    /// Set camera mode
    pub fn set_camera_mode(&mut self, mode: CameraMode) {
        self.player_controller.camera_mode = mode;
    }

    /// Get camera mode
    pub fn camera_mode(&self) -> CameraMode {
        self.player_controller.camera_mode
    }

    /// Get reference to the game clock
    pub fn game_clock(&self) -> &GameClock {
        &self.game_clock
    }

    /// Get mutable reference to the game clock
    pub fn game_clock_mut(&mut self) -> &mut GameClock {
        &mut self.game_clock
    }

    /// Get sun and moon directions from the game clock
    pub fn celestial_directions(&self) -> (Vec3, Vec3) {
        self.game_clock.celestial_directions()
    }

    /// Get current time of day in hours (0.0-24.0)
    pub fn time_of_day(&self) -> f32 {
        self.game_clock.time_of_day()
    }

    /// Set time of day directly (for debugging/testing)
    pub fn set_time_of_day(&mut self, time: f32) {
        self.game_clock.set_time(time);
    }

    /// Point the camera toward `target`.
    /// - FPS mode: recomputes yaw/pitch to face target from current position
    /// - Isometric mode: sets rts_look_target so the RTS camera centers on target
    pub fn look_at(&mut self, target: Vec3) {
        self.player_controller.look_at(target);
    }
}

