use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
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
    /// FPS pawn world position. Frozen while in Isometric mode.
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
    pub camera_mode: CameraMode,
    pub rts_height: f32,
    /// RTS camera look-at target. Panned independently from `position`.
    /// Re-centered on `position` each time the player enters Isometric mode.
    pub rts_look_target: Vec3,
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
            rts_look_target: position,
        }
    }

    /// Point the camera toward `target`.
    /// - FirstPerson: recomputes yaw and pitch to face target from current position.
    /// - Isometric: sets rts_look_target so the RTS camera centers on target.
    pub fn look_at(&mut self, target: Vec3) {
        match self.camera_mode {
            CameraMode::FirstPerson => {
                let delta = target - self.position;
                self.yaw = delta.x.atan2(delta.z);
                let len = delta.length();
                self.pitch = if len > f32::EPSILON {
                    (delta.y / len)
                        .asin()
                        .clamp(-PITCH_LIMIT_RAD, PITCH_LIMIT_RAD)
                } else {
                    0.0
                };
            }
            CameraMode::Isometric => {
                self.rts_look_target = target;
            }
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

                // Movement pans rts_look_target (FPS pawn position stays frozen)
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
                    self.rts_look_target += dir.normalize_or_zero() * self.speed * dt;
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
            let view = look_at_mat4(eye, center, up);
            let proj = perspective(
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
            let eye = pc.rts_look_target + camera_offset;
            let center = pc.rts_look_target;
            let up = Vec3::Y;
            let view = look_at_mat4(eye, center, up);
            let proj = perspective(
                DEFAULT_FOV_DEG.to_radians(),
                DEFAULT_ASPECT_RATIO,
                NEAR_CLIP,
                FAR_CLIP,
            );
            (view, proj, eye)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, SQRT_2};

    use super::*;

    /// Golden matrices from glam 0.30 (`Mat4::look_at_rh`, `Mat4::perspective_rh`);
    /// camera output must not drift across glam upgrades.
    const PREVIOUS_PERSPECTIVE: [[f32; 4]; 4] = [
        [1.357_995, 0.0, 0.0, 0.0],
        [0.0, 2.414_213_4, 0.0, 0.0],
        [0.0, 0.0, -1.000_066_6, -1.0],
        [0.0, 0.0, -0.100_006_66, 0.0],
    ];
    const PREVIOUS_FIRST_PERSON_VIEW: [[f32; 4]; 4] = [
        [-0.764_842_33, 0.190_379_28, -0.615_444_6, 0.0],
        [0.0, 0.955_336_63, 0.295_520_16, 0.0],
        [0.644_217_55, 0.226_026_33, -0.730_681_84, 0.0],
        [5.515_614_5, -1.351_679_4, -2.398_115_9, 1.0],
    ];
    const PREVIOUS_ISOMETRIC_VIEW: [[f32; 4]; 4] = [
        [0.707_106_77, -0.408_248_28, 0.577_350_26, 0.0],
        [0.0, 0.816_496_55, 0.577_350_26, 0.0],
        [-0.707_106_77, -0.408_248_28, 0.577_350_26, 0.0],
        [-4.242_641_4, 0.816_496_85, -27.135_464, 1.0],
    ];
    /// Absorbs last-bit rounding differences between glam versions.
    const TOLERANCE: f32 = 1e-5;

    fn assert_matrix(actual: Mat4, expected: [[f32; 4]; 4]) {
        let expected = Mat4::from_cols_array_2d(&expected);
        assert!(
            actual.abs_diff_eq(expected, TOLERANCE),
            "actual {actual:?}\nexpected {expected:?}"
        );
    }

    #[test]
    fn view_projection_matches_previous_glam_output() {
        let mut first_person = PlayerController::new(Vec3::new(3.0, 2.0, -5.0));
        first_person.yaw = 0.7;
        first_person.pitch = -0.3;
        let (view, proj, eye) = controller_to_camera(&first_person);
        assert_matrix(view, PREVIOUS_FIRST_PERSON_VIEW);
        assert_matrix(proj, PREVIOUS_PERSPECTIVE);
        assert_eq!(eye, Vec3::new(3.0, 2.0, -5.0));

        let mut isometric = PlayerController::new(Vec3::ZERO);
        isometric.camera_mode = CameraMode::Isometric;
        isometric.rts_height = 30.0;
        isometric.rts_look_target = Vec3::new(4.0, 0.0, -2.0);
        let (view, proj, eye) = controller_to_camera(&isometric);
        assert_matrix(view, PREVIOUS_ISOMETRIC_VIEW);
        assert_matrix(proj, PREVIOUS_PERSPECTIVE);
        assert_eq!(eye, Vec3::new(19.0, 15.0, 13.0));
    }

    #[test]
    fn movement_rules() {
        const TOLERANCE: f32 = 1e-3;
        struct Case {
            name: &'static str,
            mode: CameraMode,
            input: ControllerInput,
            dt: f32,
            start_yaw: f32,
            start_pitch: f32,
            position: Vec3,
            yaw: f32,
            pitch: f32,
            look_target: Vec3,
            height: f32,
        }
        let start = Vec3::new(1.0, 2.0, 3.0);
        let unmoved = Case {
            name: "",
            mode: CameraMode::FirstPerson,
            input: ControllerInput::default(),
            dt: 1.0,
            start_yaw: 0.0,
            start_pitch: 0.0,
            position: start,
            yaw: 0.0,
            pitch: 0.0,
            look_target: start,
            height: 20.0,
        };
        let cases = [
            Case {
                name: "sprint scales speed 1.5x",
                input: ControllerInput {
                    forward: 1.0,
                    sprint: true,
                    ..Default::default()
                },
                dt: 0.5,
                position: Vec3::new(1.0, 2.0, 6.0),
                ..unmoved
            },
            Case {
                name: "pitch clamps up",
                input: ControllerInput {
                    pitch_delta: 10.0,
                    ..Default::default()
                },
                pitch: 1.54,
                ..unmoved
            },
            Case {
                name: "pitch clamps down",
                input: ControllerInput {
                    pitch_delta: -10.0,
                    ..Default::default()
                },
                pitch: -1.54,
                ..unmoved
            },
            Case {
                name: "isometric forward pans the look target, not the pawn",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    forward: 1.0,
                    ..Default::default()
                },
                look_target: start + Vec3::new(-2.828, 0.0, -2.828),
                ..unmoved
            },
            Case {
                name: "isometric up does nothing",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    up: 1.0,
                    ..Default::default()
                },
                ..unmoved
            },
            Case {
                name: "zoom in clamps to min height",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    zoom_delta: 100.0,
                    ..Default::default()
                },
                height: 5.0,
                ..unmoved
            },
            Case {
                name: "zoom out clamps to max height",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    zoom_delta: -100.0,
                    ..Default::default()
                },
                height: 50.0,
                ..unmoved
            },
            Case {
                name: "forward follows a non-zero yaw",
                input: ControllerInput {
                    forward: 1.0,
                    yaw_delta: FRAC_PI_2,
                    ..Default::default()
                },
                position: Vec3::new(5.0, 2.0, 3.0),
                yaw: FRAC_PI_2,
                ..unmoved
            },
            Case {
                name: "strafe right is perpendicular to a diagonal yaw",
                input: ControllerInput {
                    right: 1.0,
                    yaw_delta: FRAC_PI_4,
                    ..Default::default()
                },
                position: start + Vec3::new(-2.8284, 0.0, 2.8284),
                yaw: FRAC_PI_4,
                ..unmoved
            },
            Case {
                name: "strafe right at yaw zero moves toward negative x",
                input: ControllerInput {
                    right: 1.0,
                    ..Default::default()
                },
                position: Vec3::new(-3.0, 2.0, 3.0),
                ..unmoved
            },
            Case {
                name: "up moves along world y",
                input: ControllerInput {
                    up: 1.0,
                    yaw_delta: 1.0,
                    ..Default::default()
                },
                position: Vec3::new(1.0, 6.0, 3.0),
                yaw: 1.0,
                ..unmoved
            },
            Case {
                name: "forward and up combine into one normalized direction",
                input: ControllerInput {
                    forward: 1.0,
                    up: 1.0,
                    ..Default::default()
                },
                position: start + Vec3::new(0.0, 2.8284, 2.8284),
                ..unmoved
            },
            Case {
                name: "forward follows pitch",
                input: ControllerInput {
                    forward: 1.0,
                    pitch_delta: FRAC_PI_6,
                    ..Default::default()
                },
                position: start + Vec3::new(0.0, 2.0, 3.4641),
                pitch: FRAC_PI_6,
                ..unmoved
            },
            Case {
                name: "look deltas add to the existing yaw and pitch without moving",
                input: ControllerInput {
                    yaw_delta: 0.5,
                    pitch_delta: 0.3,
                    ..Default::default()
                },
                start_yaw: 0.4,
                start_pitch: 0.2,
                yaw: 0.9,
                pitch: 0.5,
                ..unmoved
            },
            Case {
                name: "dt scales distance",
                input: ControllerInput {
                    forward: 1.0,
                    ..Default::default()
                },
                dt: 0.25,
                position: Vec3::new(1.0, 2.0, 4.0),
                ..unmoved
            },
            Case {
                name: "isometric right pans along the ground perpendicular to forward",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    right: 1.0,
                    ..Default::default()
                },
                look_target: start + Vec3::new(2.8284, 0.0, -2.8284),
                ..unmoved
            },
            Case {
                name: "isometric forward and right combine into one normalized pan",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    forward: 1.0,
                    right: 1.0,
                    ..Default::default()
                },
                look_target: start + Vec3::new(0.0, 0.0, -4.0),
                ..unmoved
            },
            Case {
                name: "isometric pan scales with dt",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    forward: 1.0,
                    ..Default::default()
                },
                dt: 0.5,
                look_target: start + Vec3::new(-SQRT_2, 0.0, -SQRT_2),
                ..unmoved
            },
            Case {
                name: "isometric ignores sprint and look deltas",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    forward: 1.0,
                    sprint: true,
                    yaw_delta: 1.0,
                    pitch_delta: 1.0,
                    ..Default::default()
                },
                look_target: start + Vec3::new(-2.8284, 0.0, -2.8284),
                ..unmoved
            },
            Case {
                name: "zoom moves two height units per notch",
                mode: CameraMode::Isometric,
                input: ControllerInput {
                    zoom_delta: 1.0,
                    ..Default::default()
                },
                height: 18.0,
                ..unmoved
            },
        ];

        for case in cases {
            let mut pc = PlayerController::new(start);
            pc.camera_mode = case.mode;
            pc.yaw = case.start_yaw;
            pc.pitch = case.start_pitch;

            pc.apply_input(&case.input, case.dt);

            let name = case.name;
            assert!(
                pc.position.abs_diff_eq(case.position, TOLERANCE),
                "{name}: position {}",
                pc.position
            );
            assert!(
                (pc.yaw - case.yaw).abs() < TOLERANCE,
                "{name}: yaw {}",
                pc.yaw
            );
            assert!(
                (pc.pitch - case.pitch).abs() < TOLERANCE,
                "{name}: pitch {}",
                pc.pitch
            );
            assert!(
                pc.rts_look_target.abs_diff_eq(case.look_target, TOLERANCE),
                "{name}: look target {}",
                pc.rts_look_target
            );
            assert!(
                (pc.rts_height - case.height).abs() < TOLERANCE,
                "{name}: height {}",
                pc.rts_height
            );
        }
    }

    #[test]
    fn look_at_first_person_faces_target() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let mut pc = PlayerController::new(start);

        pc.look_at(start + Vec3::new(2.0, 3.0, 6.0));

        assert!((pc.yaw - 0.321_75).abs() < 1e-4, "yaw {}", pc.yaw);
        assert!((pc.pitch - 0.442_99).abs() < 1e-4, "pitch {}", pc.pitch);
        let facing = Vec3::new(
            pc.yaw.sin() * pc.pitch.cos(),
            pc.pitch.sin(),
            pc.yaw.cos() * pc.pitch.cos(),
        );
        assert!(facing.abs_diff_eq(Vec3::new(2.0, 3.0, 6.0) / 7.0, 1e-4));
        assert_eq!(pc.position, start);
        assert_eq!(pc.rts_look_target, start);
    }

    #[test]
    fn look_at_first_person_resolves_yaw_for_rear_and_side_targets() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let cases = [
            (Vec3::new(2.0, 0.0, -2.0), 3.0 * FRAC_PI_4),
            (Vec3::new(-4.0, 0.0, 0.0), -FRAC_PI_2),
            (Vec3::new(0.0, 0.0, 5.0), 0.0),
        ];

        for (offset, yaw) in cases {
            let mut pc = PlayerController::new(start);

            pc.look_at(start + offset);

            assert!((pc.yaw - yaw).abs() < 1e-4, "{offset}: yaw {}", pc.yaw);
            assert!(pc.pitch.abs() < 1e-4, "{offset}: pitch {}", pc.pitch);
        }
    }

    #[test]
    fn look_at_first_person_clamps_pitch_when_target_is_straight_above_or_below() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let mut pc = PlayerController::new(start);

        pc.look_at(start + Vec3::Y);
        let up = pc.pitch;
        pc.look_at(start - Vec3::Y);
        let down = pc.pitch;

        assert!((up - 1.54).abs() < 1e-4, "up {up}");
        assert!((down + 1.54).abs() < 1e-4, "down {down}");
    }

    #[test]
    fn look_at_own_position_resets_pitch_to_level() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let mut pc = PlayerController::new(start);
        pc.pitch = 0.8;
        pc.yaw = 1.0;

        pc.look_at(start);

        assert_eq!(pc.pitch, 0.0);
    }

    #[test]
    fn look_at_isometric_moves_only_the_target() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let mut pc = PlayerController::new(start);
        pc.camera_mode = CameraMode::Isometric;
        pc.yaw = 0.4;
        pc.pitch = 0.2;

        pc.look_at(Vec3::new(9.0, 8.0, 7.0));

        assert_eq!(pc.rts_look_target, Vec3::new(9.0, 8.0, 7.0));
        assert_eq!(pc.position, start);
        assert_eq!(pc.yaw, 0.4);
        assert_eq!(pc.pitch, 0.2);
    }
}
