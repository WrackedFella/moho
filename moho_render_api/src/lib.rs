//! Shared contract between `moho_game` (domain) and `moho_renderer`
//! (infrastructure): GPU-facing data types and the traits game-domain types
//! implement to be rendered and persisted, without either crate depending on
//! the other.

mod instance;
mod material;
mod persistence;

pub use instance::{InstanceGpu, Renderable};
pub use material::{MaterialGpu, MaterialKey, RenderMaterial};
pub use persistence::{CameraDesc, LightDesc};

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
    fn instance_gpu_is_pod_and_zeroable() {
        fn assert_pod<T: bytemuck::Pod + bytemuck::Zeroable>() {}
        assert_pod::<InstanceGpu>();
    }
}
