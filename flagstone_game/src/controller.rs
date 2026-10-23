use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, UVec2, Vec2, Vec3};

// ── Camera / movement defaults ─────────────────────────────────────────
/// Least vertical gap the strategy camera keeps above the surface under its eye, in metres.
pub const CAMERA_CLEARANCE: f32 = 1.0;
/// Width of the screen-edge band that pans the strategy camera, in physical pixels.
pub const EDGE_PAN_MARGIN_PX: f32 = 16.0;
const DEFAULT_MOVE_SPEED: f32 = 4.0;
const SPRINT_SPEED_MULTIPLIER: f32 = 1.5;
/// Nearly ±π/2 (≈ ±88.3°), prevents gimbal lock at the poles.
const PITCH_LIMIT_RAD: f32 = 1.54;
const ISOMETRIC_CAMERA_OFFSET: Vec3 = Vec3::new(10.0, 10.0, 10.0);
const RTS_MIN_HEIGHT: f32 = 5.0;
const RTS_MAX_HEIGHT: f32 = 50.0;
const RTS_DEFAULT_HEIGHT: f32 = 20.0;
/// Rate, per second, at which the focus height eases toward the ground under it.
const RTS_FOLLOW_RATE: f32 = 6.0;

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

    /// This controller with its look turned by the deltas, pitch clamped exactly as
    /// [`apply_input`](Self::apply_input) does; nothing else changes.
    pub fn turned(&self, yaw_delta: f32, pitch_delta: f32) -> Self {
        let mut turned = *self;
        turned.turn(yaw_delta, pitch_delta);
        turned
    }

    fn turn(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch = (self.pitch + pitch_delta).clamp(-PITCH_LIMIT_RAD, PITCH_LIMIT_RAD);
    }

    /// Advance the controller by `dt` seconds of `input`.
    ///
    /// `ground_height(x, z)` is the surface height at a world column, `None` when the
    /// column is not loaded. It only affects `Isometric` mode: the focus height eases
    /// toward the ground under the focus, then the focus is raised so the eye stays at
    /// least [`CAMERA_CLEARANCE`] above the surface under the eye. Each rule is skipped
    /// when its own column is `None`. Pan speed scales with `rts_height`.
    pub fn apply_input(
        &mut self,
        input: &ControllerInput,
        dt: f32,
        ground_height: impl Fn(f32, f32) -> Option<f32>,
    ) {
        match self.camera_mode {
            CameraMode::FirstPerson => {
                self.turn(input.yaw_delta, input.pitch_delta);

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
                    let zoom_scaled_speed = self.speed * self.rts_height / RTS_DEFAULT_HEIGHT;
                    self.rts_look_target += dir.normalize_or_zero() * zoom_scaled_speed * dt;
                }

                // Follow: ease the focus height toward the ground under the focus.
                if let Some(ground) = ground_height(self.rts_look_target.x, self.rts_look_target.z)
                {
                    let ease = 1.0 - (-RTS_FOLLOW_RATE * dt).exp();
                    self.rts_look_target.y += (ground - self.rts_look_target.y) * ease;
                }

                // Clamp: raise the whole camera when the eye would sit inside the terrain.
                let eye = self.rts_look_target
                    + ISOMETRIC_CAMERA_OFFSET * self.rts_height / RTS_DEFAULT_HEIGHT;
                if let Some(surface) = ground_height(eye.x, eye.z) {
                    let shortfall = surface + CAMERA_CLEARANCE - eye.y;
                    if shortfall > 0.0 {
                        self.rts_look_target.y += shortfall;
                    }
                }
            }
        }
    }
}

/// Pan axes `(right, forward)`, each in `-1.0..=1.0`, for a pointer at `pointer` physical
/// pixels in a window of `viewport` pixels. Screen top is forward, screen right is right.
/// Zero away from the `margin_px` band at each edge, and when the pointer is `None` (left
/// the window) or outside the viewport.
pub fn edge_pan_axes(pointer: Option<(f32, f32)>, viewport: UVec2, margin_px: f32) -> Vec2 {
    let Some((x, y)) = pointer else {
        return Vec2::ZERO;
    };
    let (w, h) = (viewport.x as f32, viewport.y as f32);
    if !(0.0..w).contains(&x) || !(0.0..h).contains(&y) {
        return Vec2::ZERO;
    }
    let band = |pos: f32, extent: f32| {
        if pos < margin_px {
            -1.0
        } else if pos >= extent - margin_px {
            1.0
        } else {
            0.0
        }
    };
    Vec2::new(band(x, w), -band(y, h))
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
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, FRAC_PI_6, SQRT_2};

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
    const LOOK_AT_TOLERANCE: f32 = 1e-4;

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
                position: start + Vec3::new(-2.0 * SQRT_2, 0.0, 2.0 * SQRT_2),
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
                position: start + Vec3::new(0.0, 2.0 * SQRT_2, 2.0 * SQRT_2),
                ..unmoved
            },
            Case {
                name: "forward follows pitch",
                input: ControllerInput {
                    forward: 1.0,
                    pitch_delta: FRAC_PI_6,
                    ..Default::default()
                },
                position: start + Vec3::new(0.0, 2.0, 2.0 * 3.0_f32.sqrt()),
                pitch: FRAC_PI_6,
                ..unmoved
            },
            Case {
                name: "forward combines non-zero yaw and pitch",
                input: ControllerInput {
                    forward: 1.0,
                    ..Default::default()
                },
                start_yaw: FRAC_PI_2,
                start_pitch: FRAC_PI_3,
                position: start + Vec3::new(2.0, 2.0 * 3.0_f32.sqrt(), 0.0),
                yaw: FRAC_PI_2,
                pitch: FRAC_PI_3,
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
                look_target: start + Vec3::new(2.0 * SQRT_2, 0.0, -2.0 * SQRT_2),
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
                look_target: start + Vec3::new(-2.0 * SQRT_2, 0.0, -2.0 * SQRT_2),
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

            pc.apply_input(&case.input, case.dt, |_, _| None);

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

        assert!(
            (pc.yaw - 0.321_75).abs() < LOOK_AT_TOLERANCE,
            "yaw {}",
            pc.yaw
        );
        assert!(
            (pc.pitch - 0.442_99).abs() < LOOK_AT_TOLERANCE,
            "pitch {}",
            pc.pitch
        );
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

            assert!(
                (pc.yaw - yaw).abs() < LOOK_AT_TOLERANCE,
                "{offset}: yaw {}",
                pc.yaw
            );
            assert!(
                pc.pitch.abs() < LOOK_AT_TOLERANCE,
                "{offset}: pitch {}",
                pc.pitch
            );
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

        assert!((up - 1.54).abs() < LOOK_AT_TOLERANCE, "up {up}");
        assert!((down + 1.54).abs() < LOOK_AT_TOLERANCE, "down {down}");
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

    #[test]
    fn look_at_target_exactly_epsilon_away_resets_pitch_to_level() {
        let mut pc = PlayerController::new(Vec3::ZERO);
        pc.pitch = 0.8;

        pc.look_at(Vec3::new(0.0, f32::EPSILON, 0.0));

        assert_eq!(pc.pitch, 0.0);
    }

    #[test]
    fn turned_adds_the_deltas_to_yaw_and_pitch() {
        let mut controller = PlayerController::new(Vec3::new(1.0, 2.0, 3.0));
        controller.yaw = 0.5;
        controller.pitch = 0.25;

        let turned = controller.turned(0.125, -0.5);

        assert_eq!(turned.yaw, 0.625);
        assert_eq!(turned.pitch, -0.25);
        assert_eq!(turned.position, controller.position);
        assert_eq!(turned.speed, controller.speed);
        assert_eq!(turned.camera_mode, controller.camera_mode);
        assert_eq!(controller.yaw, 0.5, "the original is not changed");
    }

    #[test]
    fn turned_clamps_pitch_like_apply_input() {
        struct Row {
            yaw: f32,
            pitch: f32,
            yaw_delta: f32,
            pitch_delta: f32,
            dt: f32,
            clamped: bool,
        }
        let row = |yaw, pitch, yaw_delta, pitch_delta, dt, clamped| Row {
            yaw,
            pitch,
            yaw_delta,
            pitch_delta,
            dt,
            clamped,
        };
        let rows = [
            row(0.0, PITCH_LIMIT_RAD, 0.3, 0.5, 0.0, true),
            row(0.0, -PITCH_LIMIT_RAD, 0.3, -0.5, 0.0, true),
            row(0.0, PITCH_LIMIT_RAD - 0.1, 0.3, 2.0, 0.0, true),
            row(0.0, -PITCH_LIMIT_RAD + 0.1, 0.3, -2.0, 0.0, true),
            row(0.37, 1.2, 0.123_456_7, 0.9, 1.0 / 60.0, true),
        ];

        for r in rows {
            let mut controller = PlayerController::new(Vec3::ZERO);
            controller.yaw = r.yaw;
            controller.pitch = r.pitch;
            let mut applied = controller;

            let turned = controller.turned(r.yaw_delta, r.pitch_delta);
            applied.apply_input(
                &ControllerInput {
                    yaw_delta: r.yaw_delta,
                    pitch_delta: r.pitch_delta,
                    ..Default::default()
                },
                r.dt,
                |_, _| None,
            );

            let case = format!("from {} by {}", r.pitch, r.pitch_delta);
            assert_eq!(turned.pitch, applied.pitch, "{case}");
            assert_eq!(turned.yaw, applied.yaw, "{case}");
            assert_eq!(turned.pitch.abs() == PITCH_LIMIT_RAD, r.clamped, "{case}");
        }
    }

    mod strategy_camera {
        use super::*;
        use proptest::prelude::*;

        const TICK: f32 = 1.0 / 60.0;
        const VIEWPORT: UVec2 = UVec2::new(1280, 720);
        const MARGIN: f32 = 16.0;
        const NO_GROUND: fn(f32, f32) -> Option<f32> = |_, _| None;

        fn strategy_at(focus: Vec3, height: f32) -> PlayerController {
            let mut pc = PlayerController::new(Vec3::new(1.0, 2.0, 3.0));
            pc.camera_mode = CameraMode::Isometric;
            pc.rts_look_target = focus;
            pc.rts_height = height;
            pc
        }

        fn eye_of(pc: &PlayerController) -> Vec3 {
            controller_to_camera(pc).2
        }

        fn sign(v: f32) -> f32 {
            if v == 0.0 { 0.0 } else { v.signum() }
        }

        fn horizontal(v: Vec3) -> Vec3 {
            Vec3::new(v.x, 0.0, v.z)
        }

        #[test]
        fn isometric_pan_moves_the_view_target_and_not_the_pawn() {
            let mut pc = strategy_at(Vec3::new(1.0, 2.0, 3.0), RTS_DEFAULT_HEIGHT);
            let pawn = pc.position;
            let eye_before = eye_of(&pc);
            let input = ControllerInput {
                forward: 1.0,
                ..Default::default()
            };

            for _ in 0..60 {
                pc.apply_input(&input, TICK, NO_GROUND);
            }

            let panned = horizontal(pc.rts_look_target) - Vec3::new(1.0, 0.0, 3.0);
            assert_eq!(pc.position, pawn);
            assert!(
                panned.dot(Vec3::new(-1.0, 0.0, -1.0).normalize()) > 3.9,
                "panned {panned}"
            );
            assert!(eye_of(&pc).distance(eye_before) > 3.9);
        }

        #[test]
        fn edge_pan_axes_point_toward_the_nearest_edge() {
            let (w, h) = (VIEWPORT.x as f32, VIEWPORT.y as f32);
            let (cx, cy) = (w / 2.0, h / 2.0);
            let inside = MARGIN - 1.0;
            let outside = MARGIN + 1.0;
            // (name, pointer, expected sign of (right, forward))
            let table = [
                ("left edge", Some((0.0, cy)), (-1.0, 0.0)),
                ("right edge", Some((w - 1.0, cy)), (1.0, 0.0)),
                ("top edge", Some((cx, 0.0)), (0.0, 1.0)),
                ("bottom edge", Some((cx, h - 1.0)), (0.0, -1.0)),
                ("top left", Some((0.0, 0.0)), (-1.0, 1.0)),
                ("top right", Some((w - 1.0, 0.0)), (1.0, 1.0)),
                ("bottom left", Some((0.0, h - 1.0)), (-1.0, -1.0)),
                ("bottom right", Some((w - 1.0, h - 1.0)), (1.0, -1.0)),
                ("centre", Some((cx, cy)), (0.0, 0.0)),
                (
                    "just inside the left margin",
                    Some((inside, cy)),
                    (-1.0, 0.0),
                ),
                (
                    "just outside the left margin",
                    Some((outside, cy)),
                    (0.0, 0.0),
                ),
                ("just inside the top margin", Some((cx, inside)), (0.0, 1.0)),
                (
                    "just outside the top margin",
                    Some((cx, outside)),
                    (0.0, 0.0),
                ),
                (
                    "just inside the right margin",
                    Some((w - inside, cy)),
                    (1.0, 0.0),
                ),
                (
                    "just outside the bottom margin",
                    Some((cx, h - outside)),
                    (0.0, 0.0),
                ),
                ("pointer left the window", None, (0.0, 0.0)),
                (
                    "pointer beyond the viewport",
                    Some((w + 50.0, cy)),
                    (0.0, 0.0),
                ),
                (
                    "pointer left of the viewport",
                    Some((-50.0, cy)),
                    (0.0, 0.0),
                ),
                (
                    "pointer below the viewport",
                    Some((cx, h + 50.0)),
                    (0.0, 0.0),
                ),
            ];

            for (name, pointer, (right, forward)) in table {
                let axes = edge_pan_axes(pointer, VIEWPORT, MARGIN);

                assert_eq!(sign(axes.x), right, "{name}: {axes}");
                assert_eq!(sign(axes.y), forward, "{name}: {axes}");
            }
        }

        #[test]
        fn edge_pan_axes_follow_the_given_margin_and_viewport() {
            let wide_margin = 40.0;
            let small = UVec2::new(800, 600);
            // (name, viewport, margin, pointer, expected sign of (right, forward))
            let table = [
                (
                    "inside a 40 px margin",
                    VIEWPORT,
                    wide_margin,
                    (30.0, 360.0),
                    (-1.0, 0.0),
                ),
                (
                    "outside a 40 px margin",
                    VIEWPORT,
                    wide_margin,
                    (50.0, 360.0),
                    (0.0, 0.0),
                ),
                (
                    "right edge of an 800 px window",
                    small,
                    MARGIN,
                    (799.0, 300.0),
                    (1.0, 0.0),
                ),
                (
                    "1000 px is off an 800 px window",
                    small,
                    MARGIN,
                    (1000.0, 300.0),
                    (0.0, 0.0),
                ),
                (
                    "bottom edge of a 600 px window",
                    small,
                    MARGIN,
                    (400.0, 599.0),
                    (0.0, -1.0),
                ),
                (
                    "centre of a 600 px window",
                    small,
                    MARGIN,
                    (400.0, 300.0),
                    (0.0, 0.0),
                ),
            ];

            for (name, viewport, margin, pointer, (right, forward)) in table {
                let axes = edge_pan_axes(Some(pointer), viewport, margin);

                assert_eq!(sign(axes.x), right, "{name}: {axes}");
                assert_eq!(sign(axes.y), forward, "{name}: {axes}");
            }
        }

        proptest! {
            #[test]
            fn edge_pan_axes_stay_within_unit_range_and_vanish_in_the_interior(
                width in 200u32..2000,
                height in 200u32..1200,
                margin in 4.0f32..64.0,
                x_fraction in -0.2f32..1.2,
                y_fraction in -0.2f32..1.2,
            ) {
                let (w, h) = (width as f32, height as f32);
                let (x, y) = (x_fraction * w, y_fraction * h);

                let axes = edge_pan_axes(Some((x, y)), UVec2::new(width, height), margin);

                prop_assert!(axes.x.abs() <= 1.0 && axes.y.abs() <= 1.0, "{axes}");
                let interior = x > margin && x < w - margin && y > margin && y < h - margin;
                if interior {
                    prop_assert_eq!(axes, Vec2::ZERO);
                }
                let on_screen = (0.0..w).contains(&x) && (0.0..h).contains(&y);
                if !on_screen {
                    prop_assert_eq!(axes, Vec2::ZERO);
                }
                if on_screen && x < margin {
                    prop_assert!(axes.x < 0.0, "{axes}");
                }
                if on_screen && x >= w - margin {
                    prop_assert!(axes.x > 0.0, "{axes}");
                }
                if on_screen && y < margin {
                    prop_assert!(axes.y > 0.0, "{axes}");
                }
                if on_screen && y >= h - margin {
                    prop_assert!(axes.y < 0.0, "{axes}");
                }
            }

            #[test]
            fn eye_stays_above_a_hillside_at_every_zoom_height(
                base in -10.0f32..40.0,
                slope_x in -1.5f32..1.5,
                slope_z in -1.5f32..1.5,
                focus_x in -50.0f32..50.0,
                focus_z in -50.0f32..50.0,
                height in RTS_MIN_HEIGHT..RTS_MAX_HEIGHT,
                zoom_delta in -5.0f32..5.0,
            ) {
                let surface = move |x: f32, z: f32| base + slope_x * x + slope_z * z;
                let focus = Vec3::new(focus_x, surface(focus_x, focus_z), focus_z);
                let mut pc = strategy_at(focus, height);
                let input = ControllerInput {
                    zoom_delta,
                    ..Default::default()
                };

                pc.apply_input(&input, TICK, |x, z| Some(surface(x, z)));

                let eye = eye_of(&pc);
                let floor = surface(eye.x, eye.z) + CAMERA_CLEARANCE;
                prop_assert!(eye.y >= floor - 1e-3, "eye {eye} below {floor}");
            }
        }

        #[test]
        fn clamp_raises_the_eye_to_exactly_the_clearance_above_the_surface() {
            let mut pc = strategy_at(Vec3::ZERO, RTS_DEFAULT_HEIGHT);

            pc.apply_input(&ControllerInput::default(), TICK, |x, _| {
                Some(if x < 5.0 { 0.0 } else { 30.0 })
            });

            let eye = eye_of(&pc);
            assert!(
                (eye.y - (30.0 + CAMERA_CLEARANCE)).abs() < 1e-3,
                "eye {eye}"
            );
            assert_eq!(horizontal(pc.rts_look_target), Vec3::ZERO);
            assert_eq!(pc.rts_height, RTS_DEFAULT_HEIGHT);
        }

        #[test]
        fn focus_height_eases_toward_the_ground_without_popping_at_a_cliff() {
            // (name, focus x, start height, ground west of x = 0, ground east of it)
            let rows = [
                ("drop", -20.0, 10.0, 0.0, 10.0),
                ("rise", -1.0, 0.0, 10.0, 0.0),
            ];

            for (name, focus_x, start, west, east) in rows {
                let ground = move |x: f32, _: f32| Some(if x < 0.0 { west } else { east });
                let mut pc = strategy_at(Vec3::new(focus_x, start, 0.0), RTS_MIN_HEIGHT);
                let target = west;

                pc.apply_input(&ControllerInput::default(), TICK, ground);

                let first_step = (pc.rts_look_target.y - start).abs();
                assert!(first_step > 0.0, "{name}: the focus did not move");
                assert!(
                    first_step < 0.5 * (target - start).abs(),
                    "{name}: popped by {first_step}"
                );
                let mut previous = pc.rts_look_target.y;
                for _ in 0..600 {
                    pc.apply_input(&ControllerInput::default(), TICK, ground);

                    let y = pc.rts_look_target.y;
                    assert!(
                        (target - y) * (target - start) >= 0.0,
                        "{name}: overshot to {y}"
                    );
                    assert!(
                        (y - target).abs() <= (previous - target).abs(),
                        "{name}: moved away"
                    );
                    previous = y;
                }
                assert!(
                    (previous - target).abs() < 0.05,
                    "{name}: settled at {previous}"
                );
            }
        }

        #[test]
        fn unloaded_ground_leaves_the_focus_height_unchanged() {
            let mut pc = strategy_at(Vec3::new(0.0, 7.0, 0.0), RTS_MIN_HEIGHT);
            let input = ControllerInput {
                forward: 1.0,
                right: 1.0,
                ..Default::default()
            };

            for _ in 0..120 {
                pc.apply_input(&input, TICK, NO_GROUND);
            }

            assert_eq!(pc.rts_look_target.y, 7.0);
            assert!(
                horizontal(pc.rts_look_target).length() > 0.5,
                "the pan still moves"
            );
        }

        #[test]
        fn unloaded_columns_are_skipped_by_the_rule_that_needs_them() {
            let idle = ControllerInput::default();

            // Only the focus column is loaded: the focus still eases to its ground.
            let mut focus_only = strategy_at(Vec3::new(0.0, 7.0, 0.0), RTS_MIN_HEIGHT);
            for _ in 0..600 {
                focus_only.apply_input(&idle, TICK, |x, _| (x < 1.0).then_some(0.0));
            }

            // Only the eye column is loaded, under a wall: the eye is still raised.
            let mut eye_only = strategy_at(Vec3::new(0.0, 7.0, 0.0), RTS_MIN_HEIGHT);
            eye_only.apply_input(&idle, TICK, |x, _| (x >= 1.0).then_some(30.0));

            assert!(
                focus_only.rts_look_target.y.abs() < 0.05,
                "focus settled at {}",
                focus_only.rts_look_target.y
            );
            let eye = eye_of(&eye_only);
            assert!(
                (eye.y - (30.0 + CAMERA_CLEARANCE)).abs() < 1e-3,
                "eye {eye}"
            );
        }

        #[test]
        fn camera_does_not_stay_raised_after_leaving_a_hill() {
            let mut pc = strategy_at(Vec3::ZERO, RTS_DEFAULT_HEIGHT);
            let idle = ControllerInput::default();
            let wall = |x: f32, _: f32| Some(if x >= 5.0 { 30.0 } else { 0.0 });
            let flat = |_: f32, _: f32| Some(0.0);

            for _ in 0..60 {
                pc.apply_input(&idle, TICK, wall);
            }
            let raised = pc.rts_look_target.y;
            for _ in 0..600 {
                pc.apply_input(&idle, TICK, flat);
            }

            assert!(raised > 20.0, "the wall raised the focus to {raised}");
            assert!(
                pc.rts_look_target.y.abs() < 0.05,
                "focus stayed at {}",
                pc.rts_look_target.y
            );
        }

        #[test]
        fn pan_speed_grows_with_zoom_height() {
            // (rts_height, metres panned in one second)
            for (height, expected) in [(5.0, 1.0), (20.0, 4.0), (50.0, 10.0)] {
                let mut pc = strategy_at(Vec3::ZERO, height);
                let input = ControllerInput {
                    forward: 1.0,
                    ..Default::default()
                };

                pc.apply_input(&input, 1.0, NO_GROUND);

                let moved = horizontal(pc.rts_look_target).length();
                assert!(
                    (moved - expected).abs() < 1e-3,
                    "height {height}: moved {moved}, expected {expected}"
                );
            }
        }
    }
}
