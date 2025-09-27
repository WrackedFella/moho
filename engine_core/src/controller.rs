use glam::{Mat4, Vec3};

/// Simple controller input collected from events.
#[derive(Clone, Copy, Debug, Default)]
pub struct ControllerInput {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
    pub yaw_delta: f32,
    pub pitch_delta: f32,
}

/// A minimal first-person player controller stored as a component.
#[derive(Clone, Copy, Debug)]
pub struct PlayerController {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl PlayerController {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            speed: 4.0,
        }
    }

    pub fn apply_input(&mut self, input: &ControllerInput, dt: f32) {
        // Apply look deltas
        self.yaw += input.yaw_delta;
        self.pitch = (self.pitch + input.pitch_delta).clamp(-1.54, 1.54);

        // Build forward/right vectors from yaw (assume Y up)
        let forward = Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos()).normalize_or_zero();
        let right = Vec3::new(-forward.z, 0.0, forward.x);

        let mut dir = Vec3::ZERO;
        dir += forward * input.forward;
        dir += right * input.right;
        dir += Vec3::Y * input.up;

        if dir.length_squared() > 0.0 {
            self.position += dir.normalize_or_zero() * self.speed * dt;
        }
    }
}

/// Convert a PlayerController into a camera (view, proj, cam_pos).
pub fn controller_to_camera(pc: &PlayerController) -> (Mat4, Mat4, Vec3) {
    // FPS-style camera: build a forward direction from yaw (around Y)
    // and pitch (up/down) and look along that direction from the eye.
    let eye = pc.position;
    // Spherical -> Cartesian conversion
    let cy = pc.yaw.cos();
    let sy = pc.yaw.sin();
    let cp = pc.pitch.cos();
    let sp = pc.pitch.sin();
    let forward = Vec3::new(sy * cp, sp, cy * cp).normalize_or_zero();
    let center = eye + forward;
    let up = Vec3::Y;
    let view = Mat4::look_at_rh(eye, center, up);
    let proj = Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
    (view, proj, eye)
}
