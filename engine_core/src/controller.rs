use glam::{Mat4, Vec3};

/// Camera mode for different viewing styles
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraMode {
    FirstPerson,
    Isometric,
}

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
    pub camera_mode: CameraMode,
}

impl PlayerController {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            speed: 4.0,
            camera_mode: CameraMode::FirstPerson,
        }
    }

    pub fn apply_input(&mut self, input: &ControllerInput, dt: f32) {
        match self.camera_mode {
            CameraMode::FirstPerson => {
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
            CameraMode::Isometric => {
                // In isometric mode, movement is relative to camera view
                // Camera is at offset (10, 10, 10), looking down at the position
                // Calculate camera-relative directions (projected onto ground plane)
                let camera_offset = Vec3::new(10.0, 10.0, 10.0);
                let to_camera = camera_offset.normalize_or_zero();

                // Forward in camera space (away from camera, projected to ground)
                let forward = Vec3::new(-to_camera.x, 0.0, -to_camera.z).normalize_or_zero();

                // Right is perpendicular to forward on the ground plane
                let right = Vec3::new(-forward.z, 0.0, forward.x);

                let mut dir = Vec3::ZERO;
                dir += forward * input.forward;
                dir += right * input.right;
                // No up/down movement in isometric mode

                if dir.length_squared() > 0.0 {
                    self.position += dir.normalize_or_zero() * self.speed * dt;
                }
            }
        }
    }
}

/// Convert a PlayerController into a camera (view, proj, cam_pos).
pub fn controller_to_camera(pc: &PlayerController) -> (Mat4, Mat4, Vec3) {
    match pc.camera_mode {
        CameraMode::FirstPerson => {
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
        CameraMode::Isometric => {
            // Isometric camera: fixed angle looking down at 45 degrees
            let eye = pc.position + Vec3::new(10.0, 10.0, 10.0); // Offset above and to the side
            let center = pc.position; // Look at the controller position
            let up = Vec3::Y;
            let view = Mat4::look_at_rh(eye, center, up);
            let proj = Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
            (view, proj, eye)
        }
    }
}
