//! Plain-data UI paint contract: what a game hands the renderer each frame to
//! draw its UI. Names no GPU or UI-toolkit type. Logical units are points;
//! physical pixels = points * `pixels_per_point`.

/// Game-assigned UI texture id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiTextureId(pub u64);

/// One UI vertex: position in points, uv in 0..1, premultiplied sRGBA colour.
/// 20 bytes: `pos` f32x2, `uv` f32x2, `color` u8x4.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// Axis-aligned rect in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// Triangle list clipped to `clip_rect`, sampling `texture`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiMesh {
    pub clip_rect: UiRect,
    pub texture: UiTextureId,
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
}

/// Texture sampling filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum UiFilter {
    Nearest,
    #[default]
    Linear,
}

/// Texture coordinate wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum UiWrap {
    #[default]
    ClampToEdge,
    Repeat,
    MirroredRepeat,
}

/// Sampler settings for a UI texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct UiSampler {
    pub magnification: UiFilter,
    pub minification: UiFilter,
    pub wrap: UiWrap,
}

/// Row-major premultiplied sRGBA pixels; `pixels.len() == size[0] * size[1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiImage {
    pub size: [u32; 2],
    pub pixels: Vec<[u8; 4]>,
}

/// Create/replace a texture (`pos: None`) or update a sub-region at `pos` (pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct UiTextureSet {
    pub id: UiTextureId,
    pub pos: Option<[u32; 2]>,
    pub image: UiImage,
    pub sampler: UiSampler,
}

/// One frame of UI paint data. `textures_set` applies in order before
/// drawing; `textures_free` after drawing.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiFrame {
    pub pixels_per_point: f32,
    pub textures_set: Vec<UiTextureSet>,
    pub meshes: Vec<UiMesh>,
    pub textures_free: Vec<UiTextureId>,
}

/// Supplies the UI paint data each frame. `size_in_pixels` is the render target size.
pub trait UiFrameSource {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame>;
}
