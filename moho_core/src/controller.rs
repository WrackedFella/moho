use glam::{Mat4, Vec3};

// ── Camera / movement defaults ─────────────────────────────────────────
const DEFAULT_MOVE_SPEED: f32 = 4.0;
const SPRINT_SPEED_MULTIPLIER: f32 = 1.5;
/// Nearly ±π/2 (≈ ±88.3°), prevents gimbal lock at the poles.
const PITCH_LIMIT_RAD: f32 = 1.54;
const ISOMETRIC_CAMERA_OFFSET: Vec3 = Vec3::new(10.0, 10.0, 10.0);
const RTS_MIN_HEIGHT: f32 = 5.0;
const RTS_MAX_HEIGHT: f32 = 50.0;
const RTS_DEFAULT_HEIGHT: f32 = 20.0;

const DEFAULT_FOV_DEG: f32 = 45.0;
const DEFAULT_ASPECT_RATIO: f32 = 16.0 / 9.0;
const NEAR_CLIP: f32 = 0.1;
/// Extended to keep the skybox visible.
const FAR_CLIP: f32 = 1500.0;

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
    pub sprint: bool,
    pub zoom_delta: f32,
}

/// A minimal first-person player controller stored as a component.
#[derive(Clone, Copy, Debug)]
pub struct PlayerController {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub camera_mode: CameraMode,
    pub rts_height: f32,
}

impl PlayerController {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            speed: DEFAULT_MOVE_SPEED,
            camera_mode: CameraMode::FirstPerson,
            rts_height: RTS_DEFAULT_HEIGHT,
        }
    }

    pub fn apply_input(&mut self, input: &ControllerInput, dt: f32) {
        match self.camera_mode {
            CameraMode::FirstPerson => {
                // Apply look deltas
                self.yaw += input.yaw_delta;
                self.pitch =
                    (self.pitch + input.pitch_delta).clamp(-PITCH_LIMIT_RAD, PITCH_LIMIT_RAD);

                // Build horizontal forward direction (for right vector and pitch-less reference)
                let forward_horizontal =
                    Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos()).normalize_or_zero();

                // Right vector stays perpendicular to forward in horizontal plane (for strafe)
                let right = Vec3::new(-forward_horizontal.z, 0.0, forward_horizontal.x);

                // Forward vector includes both yaw and pitch - so forward/back follows camera look angle
                let pitch_cos = self.pitch.cos();
                let pitch_sin = self.pitch.sin();
                let forward = Vec3::new(
                    self.yaw.sin() * pitch_cos,
                    pitch_sin,
                    self.yaw.cos() * pitch_cos,
                )
                .normalize_or_zero();

                let mut dir = Vec3::ZERO;
                dir += forward * input.forward;
                dir += right * input.right;
                dir += Vec3::Y * input.up;

                if dir.length_squared() > 0.0 {
                    // Apply sprint multiplier if active
                    let speed = if input.sprint {
                        self.speed * SPRINT_SPEED_MULTIPLIER
                    } else {
                        self.speed
                    };
                    self.position += dir.normalize_or_zero() * speed * dt;
                }
            }
            CameraMode::Isometric => {
                // RTS camera: birds-eye view with scroll-wheel zoom
                // Adjust height based on zoom input
                self.rts_height = (self.rts_height - input.zoom_delta * 2.0)
                    .clamp(RTS_MIN_HEIGHT, RTS_MAX_HEIGHT);

                // Movement is relative to camera view
                // Camera is at offset relative to position, looking down
                let camera_offset = ISOMETRIC_CAMERA_OFFSET;
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
            let proj = Mat4::perspective_rh(
                DEFAULT_FOV_DEG.to_radians(),
                DEFAULT_ASPECT_RATIO,
                NEAR_CLIP,
                FAR_CLIP,
            );
            (view, proj, eye)
        }
        CameraMode::Isometric => {
            // RTS camera: birds-eye view with zoomable height
            // Camera distance scales with height for consistent zoom feel
            let height_ratio = pc.rts_height / RTS_DEFAULT_HEIGHT;
            let camera_offset = ISOMETRIC_CAMERA_OFFSET * height_ratio;
            let eye = pc.position + camera_offset;
            let center = pc.position;
            let up = Vec3::Y;
            let view = Mat4::look_at_rh(eye, center, up);
            let proj = Mat4::perspective_rh(
                DEFAULT_FOV_DEG.to_radians(),
                DEFAULT_ASPECT_RATIO,
                NEAR_CLIP,
                FAR_CLIP,
            );
            (view, proj, eye)
        }
    }
}
