//! Shared contract between `moho_game` (domain) and `moho_renderer`
//! (infrastructure): GPU-facing data types and the traits game-domain types
//! implement to be rendered and persisted, without either crate depending on
//! the other.

mod instance;
mod material;
mod persistence;
mod ui_paint;
mod world_geometry;

pub use instance::{InstanceGpu, Renderable};
pub use material::{MaterialGpu, MaterialKey, RenderMaterial};
pub use persistence::{CameraDesc, LightDesc};
pub use ui_paint::{
    UiFilter, UiFrame, UiFrameSource, UiImage, UiMesh, UiRect, UiSampler, UiSource, UiTextureId,
    UiTextureSet, UiVertex, UiWrap,
};
pub use world_geometry::{WorldMesh, WorldMeshError, WorldMeshId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_gpu_size_matches_wgsl_layout() {
        assert_eq!(std::mem::size_of::<MaterialGpu>(), 32);
    }

    #[test]
    fn instance_gpu_size_matches_wgsl_layout() {
        // model (64) + material (4) + object_type (4) + padding (8) = 80 bytes
        assert_eq!(std::mem::size_of::<InstanceGpu>() % 16, 0);
        assert_eq!(std::mem::size_of::<InstanceGpu>(), 80);
    }

    #[test]
    fn ui_vertex_size_matches_ui_shader_vertex_layout() {
        // pos (8) + uv (8) + packed rgba (4), read as Float32x2, Float32x2, Uint32.
        assert_eq!(std::mem::size_of::<UiVertex>(), 20);
    }
}
