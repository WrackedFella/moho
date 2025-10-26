use bincode::{Decode, Encode};
use crc32fast::Hasher;
use glam::{Mat4, Vec3};
use moho_core::controller::{CameraMode, ControllerInput, PlayerController, controller_to_camera};
use serde::{Deserialize, Serialize};

const SNAP_MAGIC: &[u8; 4] = b"MOHO";
const SNAP_VERSION: u16 = 1;

#[derive(Serialize, Deserialize, Encode, Decode)]
struct SimulationSnapshot {
    // position as tuple to avoid needing glam serde support
    pos: (f32, f32, f32),
    yaw: f32,
    pitch: f32,
    camera_mode: u8,
    // controller input snapshot
    forward: f32,
    right: f32,
    up: f32,
    yaw_delta: f32,
    pitch_delta: f32,
}

impl From<&SimulationController> for SimulationSnapshot {
    fn from(s: &SimulationController) -> Self {
        let p = s.player_controller.position;
        let mode = match s.player_controller.camera_mode {
            CameraMode::FirstPerson => 0u8,
            CameraMode::Isometric => 1u8,
        };
        SimulationSnapshot {
            pos: (p.x, p.y, p.z),
            yaw: s.player_controller.yaw,
            pitch: s.player_controller.pitch,
            camera_mode: mode,
            forward: s.controller_input.forward,
            right: s.controller_input.right,
            up: s.controller_input.up,
            yaw_delta: s.controller_input.yaw_delta,
            pitch_delta: s.controller_input.pitch_delta,
        }
    }
}

impl TryFrom<SimulationSnapshot> for SimulationController {
    type Error = String;
    fn try_from(ss: SimulationSnapshot) -> Result<Self, Self::Error> {
        let pos = Vec3::new(ss.pos.0, ss.pos.1, ss.pos.2);
        let mut pc = PlayerController::new(pos);
        pc.yaw = ss.yaw;
        pc.pitch = ss.pitch;
        pc.camera_mode = match ss.camera_mode {
            0 => CameraMode::FirstPerson,
            1 => CameraMode::Isometric,
            _ => return Err(format!("unknown camera_mode: {}", ss.camera_mode)),
        };
        let ci = ControllerInput {
            forward: ss.forward,
            right: ss.right,
            up: ss.up,
            yaw_delta: ss.yaw_delta,
            pitch_delta: ss.pitch_delta,
        };
        Ok(SimulationController {
            player_controller: pc,
            controller_input: ci,
        })
    }
}

/// Snapshot layout: [MAGIC(4)][VERSION(u16 LE)][PAYLOAD_LEN(u32 LE)][CHECKSUM(u32 LE)][PAYLOAD...]
impl SimulationController {
    /// Create a validated snapshot (header + bincode payload)
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        let snap: SimulationSnapshot = self.into();
        let payload =
            bincode::encode_to_vec(&snap, bincode::config::standard()).expect("serialize snapshot");

        let mut hasher = Hasher::new();
        hasher.update(&payload);
        let checksum = hasher.finalize();

        let mut out = Vec::with_capacity(4 + 2 + 4 + 4 + payload.len());
        out.extend_from_slice(SNAP_MAGIC);
        out.extend_from_slice(&SNAP_VERSION.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Restore from snapshot bytes produced by `snapshot_bytes`.
    pub fn restore_from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 14 {
            return Err("snapshot too small".into());
        }
        if &bytes[0..4] != SNAP_MAGIC {
            return Err("bad snapshot magic".into());
        }
        let ver = u16::from_le_bytes([bytes[4], bytes[5]]);
        if ver != SNAP_VERSION {
            return Err(format!("unsupported snapshot version: {}", ver));
        }
        let payload_len = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        let checksum = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);
        if bytes.len() != 14 + payload_len {
            return Err("snapshot length mismatch".into());
        }
        let payload = &bytes[14..];
        let mut hasher = Hasher::new();
        hasher.update(payload);
        let calc = hasher.finalize();
        if calc != checksum {
            return Err("checksum mismatch".into());
        }
        let (ss, _len): (SimulationSnapshot, usize) =
            bincode::decode_from_slice(payload, bincode::config::standard())
                .map_err(|e| format!("deser error: {}", e))?;
        SimulationController::try_from(ss)
    }
}

/// Controller-focused simulation wrapper owning a PlayerController and its input state.
/// This is exposed as `SimulationController` to avoid colliding with the lightweight
/// headless `Simulation` used by unit tests.
#[derive(Debug, Clone)]
pub struct SimulationController {
    pub player_controller: PlayerController,
    pub controller_input: ControllerInput,
}

impl SimulationController {
    /// Create a new SimulationController with a controller positioned at `position`.
    pub fn new(position: Vec3) -> Self {
        Self {
            player_controller: PlayerController::new(position),
            controller_input: ControllerInput::default(),
        }
    }

    /// Mutable access to the inner ControllerInput so callers can set axis/deltas.
    pub fn controller_input_mut(&mut self) -> &mut ControllerInput {
        &mut self.controller_input
    }

    /// Apply the current controller input for this tick and return the camera
    /// tuple (view, proj, cam_pos) after applying movement/look deltas.
    pub fn apply_input(&mut self, dt: f32) -> (Mat4, Mat4, Vec3) {
        self.player_controller
            .apply_input(&self.controller_input, dt);
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
}
