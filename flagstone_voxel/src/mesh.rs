//! Mesh generation for voxel chunks.
//!
//! - `BlockyMeshGenerator`: greedy meshing with per-vertex ambient occlusion
//! - `HybridMeshGenerator`: dispatches smooth terrain (Marching Cubes) vs blocky structures
//! - `MarchingCubes`: smooth isosurface extraction for terrain

mod blocky;
mod cap;
mod contact_ao;
mod hybrid;
mod marching_cubes;

pub use blocky::BlockyMeshGenerator;
pub use cap::{CAP_PROBE, cap_chunk_layers, cut_cap_mesh};
pub(crate) use hybrid::is_fine_lod;
pub use hybrid::{ChunkContent, HybridMeshGenerator, chunks_meshed_from};
pub use marching_cubes::{MarchingCubes, SurfaceSample};

/// Intermediate mesh data produced by mesh generators and consumed by `VoxelChunk`.
/// Not stored per-block; lives only as a transient during chunk mesh generation.
#[derive(Debug, Clone)]
pub struct VoxelMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub ambient_occlusion: Vec<f32>,
    pub geometry_type: Vec<u32>,
    /// Single-channel light level (0..=1). Kept for renderer compat while the
    /// GPU pipeline is being migrated to per-channel RGB. Computed as
    /// `max(block_light_rgb) / 15` blended with `sky_exposed`.
    pub light_level: Vec<f32>,
    /// Per-vertex RGB block-light, each channel 0..=1 (raw value / 15).
    pub block_light_rgb: Vec<[f32; 3]>,
    /// Per-vertex sky-exposure factor (0.0 = underground, 1.0 = open sky).
    pub sky_exposed: Vec<f32>,
    /// Per-vertex voxel material id (resource-mapped, see
    /// `MaterialRegistry::face_material`). One material per triangle.
    pub material: Vec<u32>,
}

impl VoxelMesh {
    pub fn empty() -> Self {
        VoxelMesh {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            ambient_occlusion: Vec::new(),
            geometry_type: Vec::new(),
            light_level: Vec::new(),
            block_light_rgb: Vec::new(),
            sky_exposed: Vec::new(),
            material: Vec::new(),
        }
    }

    /// Appends one vertex's light channels from the raw block light and sky exposure of
    /// the voxel it reads; `light_level` is the compat blend of the two.
    pub(crate) fn push_light(&mut self, block_light: [u8; 3], sky_exposed: bool) {
        let rgb = block_light.map(|c| f32::from(c) / 15.0);
        let sky = if sky_exposed { 1.0f32 } else { 0.0 };
        let brightest = rgb[0].max(rgb[1]).max(rgb[2]);
        self.light_level.push((brightest + sky * 0.1).min(1.0));
        self.block_light_rgb.push(rgb);
        self.sky_exposed.push(sky);
    }
}
