use crate::events::Event;
use glam::Vec3;
use std::any::Any;

/// Graphics and rendering events
#[derive(Clone, Debug)]
pub enum GraphicsEvent {
    /// Light position changed (sun)
    LightPositionChanged { position: Vec3, intensity: f32 },

    /// Sun direction changed (yaw, pitch in radians)
    SunDirectionChanged { yaw: f32, pitch: f32 },

    /// Time of day changed
    TimeOfDayChanged { time: f32, sun_angle: f32 },

    /// Graphics setting changed
    SettingChanged { setting: GraphicsSetting },

    /// Rendering mode changed
    RenderModeChanged { mode: RenderMode },

    /// Debug view mode changed
    DebugViewChanged { mode: u32 },
}

#[derive(Clone, Debug)]
pub enum GraphicsSetting {
    AntiAliasing(AntiAliasingMode),
    ShadowQuality(ShadowQuality),
    ShadowDistance(f32),
    VSync(bool),
    FrameRateLimit(Option<u32>),
}

#[derive(Clone, Debug)]
pub enum AntiAliasingMode {
    None,
    MSAA2x,
    MSAA4x,
    MSAA8x,
    FXAA,
    TAA,
}

#[derive(Clone, Debug)]
pub enum ShadowQuality {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Clone, Debug)]
pub enum RenderMode {
    Normal,
    Wireframe,
    DebugCollision,
}

impl Event for GraphicsEvent {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
