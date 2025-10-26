use moho_core::controller::{ControllerInput, PlayerController, controller_to_camera};

/// Apply controller input to the provided `PlayerController` for a tick and
/// return the resulting camera tuple (view, proj, cam_pos).
///
/// This lets the application move the controller logic into `moho_sim` while
/// keeping the concrete `PlayerController` type owned by `moho_core`.
pub fn apply_controller_tick(pc: &mut PlayerController, input: &ControllerInput, dt: f32) -> (glam::Mat4, glam::Mat4, glam::Vec3) {
    pc.apply_input(input, dt);
    controller_to_camera(pc)
}
