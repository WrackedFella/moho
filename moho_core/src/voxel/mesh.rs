//! Mesh generation for voxel chunks.
//!
//! - `BlockyMeshGenerator`: greedy meshing with per-vertex ambient occlusion
//! - `HybridMeshGenerator`: dispatches smooth terrain (Marching Cubes) vs blocky structures
//! - `MarchingCubes`: smooth isosurface extraction for terrain

mod blocky;
mod hybrid;
mod marching_cubes;

pub use blocky::BlockyMeshGenerator;
pub use hybrid::{ChunkContent, HybridMeshGenerator};
pub use marching_cubes::MarchingCubes;

/// Intermediate mesh data produced by mesh generators and consumed by `VoxelChunk`.
/// Not stored per-block; lives only as a transient during chunk mesh generation.
#[derive(Debug, Clone)]
pub struct VoxelMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub ambient_occlusion: Vec<f32>,
    pub geometry_type: Vec<u32>,
    pub light_level: Vec<f32>,
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
        }
    }
}
