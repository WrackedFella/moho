use bincode::{Decode, Encode};

/// Serializable descriptor for a dynamic point light.
///
/// Used to persist lights alongside scene data.
#[derive(Encode, Decode, Clone, Debug)]
pub struct LightDesc {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub enabled: bool,
}

/// Serializable descriptor for camera pose.
#[derive(Encode, Decode, Clone, Debug)]
pub struct CameraDesc {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}
