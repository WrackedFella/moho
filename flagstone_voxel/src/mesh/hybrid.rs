//! Hybrid mesh generation for chunks containing both smooth and blocky geometry.
//!
//! This module detects mixed chunks and generates appropriate meshes for each
//! geometry type, then concatenates them into a unified mesh for rendering.

use super::super::face::FaceDirection;
use super::super::grid::{BlockPos, VoxelGrid};
use super::VoxelMesh;
use super::blocky::BlockyMeshGenerator;
use super::marching_cubes::MarchingCubes;
use glam::{IVec3, Vec3};

// Shift from marching-cubes field-index space to world space (excluding the chunk origin).
// Without it smooth terrain sits half a unit off-centre, misaligned with the blocky
// path and the grid used by raycasting. Two offsets fold into one `-0.5`:
// 1. `create_selective_density_field` pads by one cell, so index `i` is block `base + i - 1`.
// 2. A density sample stands for the whole block, so it belongs at the block centre (`+ 0.5`).
pub(crate) const DENSITY_SAMPLE_TO_WORLD: f32 = -0.5;

/// Edge length, in blocks, of a coarse (LOD 1) cell.
pub(crate) const COARSE_STRIDE: i32 = 2;

/// Whether the coarse cell `cell` of the chunk whose first block is `base` holds any
/// solid block. Samples into neighbouring chunks for cells outside `0..8`, so faces
/// between chunks cull correctly.
pub(crate) fn coarse_cell_solid(grid: &VoxelGrid, base: IVec3, cell: IVec3) -> bool {
    let origin = base + cell * COARSE_STRIDE;
    (0..COARSE_STRIDE).any(|dx| {
        (0..COARSE_STRIDE).any(|dy| {
            (0..COARSE_STRIDE).any(|dz| grid.is_solid_at(origin + IVec3::new(dx, dy, dz)))
        })
    })
}

/// The material a face of coarse cell `cell` (of the chunk whose first block is `base`)
/// shows: the face material of the first solid block, in x-then-z-then-y order, in the
/// 2x2 layer of the cell touching `face`; when that layer is empty, of the opposite layer.
pub(crate) fn coarse_face_material(
    grid: &VoxelGrid,
    base: IVec3,
    cell: IVec3,
    face: FaceDirection,
) -> u32 {
    let origin = base + cell * COARSE_STRIDE;
    let axis = face.axis();
    // The layer's two free axes, slowest first: x, then z, then y.
    let (slow, fast) = match axis {
        0 => (2, 1),
        1 => (0, 2),
        _ => (0, 1),
    };
    for inner in [false, true] {
        let layer = if face.is_positive() == inner {
            0
        } else {
            COARSE_STRIDE - 1
        };
        for i in 0..COARSE_STRIDE {
            for j in 0..COARSE_STRIDE {
                let mut offset = IVec3::ZERO;
                offset[axis] = layer;
                offset[slow] = i;
                offset[fast] = j;
                if let Some(material) = grid.face_material_at(origin + offset) {
                    return material;
                }
            }
        }
    }
    unreachable!("a solid coarse cell has a solid block in one of its two face layers")
}

/// The face material of the block that field sample `sample` of chunk `chunk_base`'s
/// padded density field stands for. The field is padded by one block, so sample `i` is
/// block `chunk_base + i - 1`.
///
/// # Panics
/// If the sample is air: only solid samples are looked up.
pub(crate) fn sample_face_material(grid: &VoxelGrid, chunk_base: IVec3, sample: [usize; 3]) -> u32 {
    let sample = IVec3::new(sample[0] as i32, sample[1] as i32, sample[2] as i32);
    grid.face_material_at(chunk_base + sample - IVec3::ONE)
        .expect("marching cubes asks for the material of solid samples only")
}

/// Classifies the content of a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkContent {
    /// All blocks are empty (no mesh needed)
    Empty,
    /// All blocks are smooth terrain
    AllSmooth,
    /// All blocks are blocky structures
    AllBlocky,
    /// Contains both smooth and blocky blocks
    Mixed,
}

/// Whether `lod` is meshed at full detail (LOD 0) rather than coarse; the one LOD rule
/// shared by chunk meshing and the cut caps.
pub(crate) fn is_fine_lod(lod: u8) -> bool {
    lod == 0
}

/// How the cut caps follow a chunk meshed at a level of detail, so they cover the same
/// cells as the mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshPlan {
    /// Full-detail meshing of the content found in the chunk.
    Fine(ChunkContent),
    /// Coarse blocky meshing, whatever the content.
    Coarse,
}

/// The plan for chunk `chunk_pos` meshed at `lod`.
pub(crate) fn mesh_plan(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32, lod: u8) -> MeshPlan {
    if is_fine_lod(lod) {
        let (content, _, _) = HybridMeshGenerator::analyze_chunk(grid, chunk_pos, chunk_size);
        MeshPlan::Fine(content)
    } else {
        MeshPlan::Coarse
    }
}

/// Whether the blocky mesher draws the block at `pos` in a `Mixed` chunk, where the
/// smooth mesher leaves blocky blocks out.
pub(crate) fn is_meshed_blocky_in_mixed(grid: &VoxelGrid, pos: IVec3) -> bool {
    grid.is_smooth_at(pos) == Some(false)
}

/// Hybrid mesh generator supporting both smooth and blocky geometry
pub struct HybridMeshGenerator;

impl HybridMeshGenerator {
    /// Analyze chunk content to determine mesh generation strategy
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `chunk_pos` - Chunk position in chunk coordinates
    /// * `chunk_size` - Size of chunks (typically 16)
    ///
    /// # Returns
    /// Classification of chunk content and counts of each type
    pub fn analyze_chunk(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
    ) -> (ChunkContent, usize, usize) {
        let base_pos = chunk_pos * chunk_size;
        let mut smooth_count = 0;
        let mut blocky_count = 0;

        // Scan all blocks in chunk
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    match grid.is_smooth_at(pos) {
                        Some(true) => smooth_count += 1,
                        Some(false) => blocky_count += 1,
                        None => {}
                    }
                }
            }
        }

        let content = if smooth_count == 0 && blocky_count == 0 {
            ChunkContent::Empty
        } else if blocky_count == 0 {
            ChunkContent::AllSmooth
        } else if smooth_count == 0 {
            ChunkContent::AllBlocky
        } else {
            ChunkContent::Mixed
        };

        (content, smooth_count, blocky_count)
    }

    /// Generate a mesh for a chunk, automatically selecting appropriate strategy
    ///
    /// # Arguments
    /// * `grid` - The voxel grid
    /// * `chunk_pos` - Chunk position in chunk coordinates
    /// * `chunk_size` - Size of chunks (typically 16)
    ///
    /// # Returns
    /// A VoxelMesh combining smooth and blocky geometry as appropriate
    pub fn generate_chunk_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        match Self::analyze_chunk(grid, chunk_pos, chunk_size).0 {
            ChunkContent::Empty => VoxelMesh::empty(),
            ChunkContent::AllSmooth => {
                Self::generate_smooth_mesh(grid, chunk_pos, chunk_size, false)
            }
            ChunkContent::AllBlocky => Self::generate_blocky_mesh(grid, chunk_pos, chunk_size),
            ChunkContent::Mixed => Self::generate_mixed_mesh(grid, chunk_pos, chunk_size),
        }
    }

    /// Generate mesh using Marching Cubes for smooth terrain, in world space.
    ///
    /// With `only_smooth`, blocky blocks are left out of the density field so a
    /// mixed chunk's terrain doesn't wrap its structures.
    fn generate_smooth_mesh(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
        only_smooth: bool,
    ) -> VoxelMesh {
        let density_field =
            Self::create_selective_density_field(grid, chunk_pos, chunk_size, only_smooth);
        let base = chunk_pos * chunk_size;
        let mut mesh = MarchingCubes::generate_mesh(&density_field, chunk_size as usize, |s| {
            sample_face_material(grid, base, s)
        });

        let offset = (chunk_pos * chunk_size).as_vec3() + Vec3::splat(DENSITY_SAMPLE_TO_WORLD);
        for vertex in &mut mesh.vertices {
            *vertex = (Vec3::from(*vertex) + offset).to_array();
        }

        mesh
    }

    /// Generate mesh using greedy meshing for blocky structures
    fn generate_blocky_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        let mut mesh = VoxelMesh::empty();
        let base_pos = chunk_pos * chunk_size;

        // Generate mesh for each block in chunk
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if grid.is_solid_at(pos) {
                        let block_mesh = BlockyMeshGenerator::generate_mesh(grid, pos);
                        Self::append_mesh(&mut mesh, &block_mesh, pos);
                    }
                }
            }
        }

        mesh
    }

    /// Generate mesh for mixed chunk (smooth + blocky)
    fn generate_mixed_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        let mut final_mesh = Self::generate_smooth_mesh(grid, chunk_pos, chunk_size, true);
        let base_pos = chunk_pos * chunk_size;

        // Generate blocky structure meshes (only for blocky blocks)
        for x in 0..chunk_size {
            for y in 0..chunk_size {
                for z in 0..chunk_size {
                    let pos = base_pos + IVec3::new(x, y, z);
                    if is_meshed_blocky_in_mixed(grid, pos) {
                        let block_mesh = BlockyMeshGenerator::generate_mesh(grid, pos);
                        Self::append_mesh(&mut final_mesh, &block_mesh, pos);
                    }
                }
            }
        }

        final_mesh
    }

    /// Create density field with optional filtering
    ///
    /// # Arguments
    /// * `only_smooth` - If true, only include smooth blocks in density
    pub(crate) fn create_selective_density_field(
        grid: &VoxelGrid,
        chunk_pos: IVec3,
        chunk_size: i32,
        only_smooth: bool,
    ) -> [[[f32; 18]; 18]; 18] {
        let mut field = [[[0.0f32; 18]; 18]; 18];
        let base_pos = chunk_pos * chunk_size;

        // Sample with padding (-1 to chunk_size+1)
        for (x, plane) in field.iter_mut().enumerate() {
            for (y, row) in plane.iter_mut().enumerate() {
                for (z, col) in row.iter_mut().enumerate() {
                    let world_pos = base_pos + IVec3::new(x as i32 - 1, y as i32 - 1, z as i32 - 1);

                    let is_solid = match grid.is_smooth_at(world_pos) {
                        Some(smooth) => !only_smooth || smooth,
                        None => false,
                    };

                    *col = if is_solid { 1.0 } else { 0.0 };
                }
            }
        }

        field
    }

    /// Append a block mesh to the chunk mesh with world space transform
    fn append_mesh(target: &mut VoxelMesh, source: &VoxelMesh, block_pos: BlockPos) {
        let base_index = target.vertices.len() as u32;

        // Add vertices with world space offset
        for &vertex in &source.vertices {
            target.vertices.push([
                vertex[0] + block_pos.x as f32,
                vertex[1] + block_pos.y as f32,
                vertex[2] + block_pos.z as f32,
            ]);
        }

        // Copy normals, AO, geometry type, and light data
        target.normals.extend_from_slice(&source.normals);
        target
            .ambient_occlusion
            .extend_from_slice(&source.ambient_occlusion);
        target
            .geometry_type
            .extend_from_slice(&source.geometry_type);
        target.light_level.extend_from_slice(&source.light_level);
        target
            .block_light_rgb
            .extend_from_slice(&source.block_light_rgb);
        target.sky_exposed.extend_from_slice(&source.sky_exposed);
        target.material.extend_from_slice(&source.material);

        // Add indices with offset
        for &index in &source.indices {
            target.indices.push(base_index + index);
        }
    }

    /// Generate a coarse (LOD 1) blocky mesh by sampling every 2 blocks.
    ///
    /// Each 2×2×2 block region is treated as a single coarse voxel. If any block in
    /// the region is solid the cell is solid, giving an 8³ effective resolution for a
    /// 16³ chunk. Face culling works against adjacent coarse cells (including cells in
    /// neighbouring chunks). Skirt quads are appended along the four vertical chunk
    /// edges to prevent seam cracks at LOD boundaries.
    pub fn generate_coarse_mesh(grid: &VoxelGrid, chunk_pos: IVec3, chunk_size: i32) -> VoxelMesh {
        const STRIDE: i32 = COARSE_STRIDE;
        let coarse_size = chunk_size / STRIDE; // 8 for chunk_size == 16
        let base = chunk_pos * chunk_size;
        let mut mesh = VoxelMesh::empty();
        let cell_solid =
            |cx: i32, cy: i32, cz: i32| coarse_cell_solid(grid, base, IVec3::new(cx, cy, cz));
        let face_material = |cx: i32, cy: i32, cz: i32, face: FaceDirection| {
            coarse_face_material(grid, base, IVec3::new(cx, cy, cz), face)
        };

        for cx in 0..coarse_size {
            for cy in 0..coarse_size {
                for cz in 0..coarse_size {
                    if !cell_solid(cx, cy, cz) {
                        continue;
                    }

                    let wx = base.x + cx * STRIDE;
                    let wy = base.y + cy * STRIDE;
                    let wz = base.z + cz * STRIDE;

                    // +X
                    if !cell_solid(cx + 1, cy, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx + STRIDE, wy, wz],
                            [wx + STRIDE, wy + STRIDE, wz],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE],
                            [wx + STRIDE, wy, wz + STRIDE],
                            [1.0, 0.0, 0.0],
                            face_material(cx, cy, cz, FaceDirection::PosX),
                        );
                        if cx == coarse_size - 1 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx + STRIDE, wy - STRIDE, wz],
                                [wx + STRIDE, wy, wz],
                                [wx + STRIDE, wy, wz + STRIDE],
                                [wx + STRIDE, wy - STRIDE, wz + STRIDE],
                                [1.0, 0.0, 0.0],
                                face_material(cx, cy, cz, FaceDirection::PosX),
                            );
                        }
                    }
                    // -X
                    if !cell_solid(cx - 1, cy, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx, wy, wz + STRIDE],
                            [wx, wy + STRIDE, wz + STRIDE],
                            [wx, wy + STRIDE, wz],
                            [wx, wy, wz],
                            [-1.0, 0.0, 0.0],
                            face_material(cx, cy, cz, FaceDirection::NegX),
                        );
                        if cx == 0 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx, wy - STRIDE, wz + STRIDE],
                                [wx, wy, wz + STRIDE],
                                [wx, wy, wz],
                                [wx, wy - STRIDE, wz],
                                [-1.0, 0.0, 0.0],
                                face_material(cx, cy, cz, FaceDirection::NegX),
                            );
                        }
                    }
                    // +Y
                    if !cell_solid(cx, cy + 1, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx, wy + STRIDE, wz],
                            [wx, wy + STRIDE, wz + STRIDE],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE],
                            [wx + STRIDE, wy + STRIDE, wz],
                            [0.0, 1.0, 0.0],
                            face_material(cx, cy, cz, FaceDirection::PosY),
                        );
                    }
                    // -Y
                    if !cell_solid(cx, cy - 1, cz) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx, wy, wz],
                            [wx + STRIDE, wy, wz],
                            [wx + STRIDE, wy, wz + STRIDE],
                            [wx, wy, wz + STRIDE],
                            [0.0, -1.0, 0.0],
                            face_material(cx, cy, cz, FaceDirection::NegY),
                        );
                    }
                    // +Z
                    if !cell_solid(cx, cy, cz + 1) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx, wy, wz + STRIDE],
                            [wx + STRIDE, wy, wz + STRIDE],
                            [wx + STRIDE, wy + STRIDE, wz + STRIDE],
                            [wx, wy + STRIDE, wz + STRIDE],
                            [0.0, 0.0, 1.0],
                            face_material(cx, cy, cz, FaceDirection::PosZ),
                        );
                        if cz == coarse_size - 1 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx, wy - STRIDE, wz + STRIDE],
                                [wx + STRIDE, wy - STRIDE, wz + STRIDE],
                                [wx + STRIDE, wy, wz + STRIDE],
                                [wx, wy, wz + STRIDE],
                                [0.0, 0.0, 1.0],
                                face_material(cx, cy, cz, FaceDirection::PosZ),
                            );
                        }
                    }
                    // -Z
                    if !cell_solid(cx, cy, cz - 1) {
                        Self::emit_coarse_quad(
                            &mut mesh,
                            [wx + STRIDE, wy, wz],
                            [wx, wy, wz],
                            [wx, wy + STRIDE, wz],
                            [wx + STRIDE, wy + STRIDE, wz],
                            [0.0, 0.0, -1.0],
                            face_material(cx, cy, cz, FaceDirection::NegZ),
                        );
                        if cz == 0 {
                            Self::emit_coarse_quad(
                                &mut mesh,
                                [wx + STRIDE, wy - STRIDE, wz],
                                [wx, wy - STRIDE, wz],
                                [wx, wy, wz],
                                [wx + STRIDE, wy, wz],
                                [0.0, 0.0, -1.0],
                                face_material(cx, cy, cz, FaceDirection::NegZ),
                            );
                        }
                    }
                }
            }
        }

        mesh
    }

    /// Emit a single quad (2 triangles) into a VoxelMesh with no AO and full brightness.
    fn emit_coarse_quad(
        mesh: &mut VoxelMesh,
        v0: [i32; 3],
        v1: [i32; 3],
        v2: [i32; 3],
        v3: [i32; 3],
        normal: [f32; 3],
        material: u32,
    ) {
        let base = mesh.vertices.len() as u32;
        for v in [v0, v1, v2, v3] {
            mesh.vertices.push([v[0] as f32, v[1] as f32, v[2] as f32]);
            mesh.normals.push(normal);
            mesh.ambient_occlusion.push(1.0);
            mesh.geometry_type.push(1);
            mesh.light_level.push(1.0);
            mesh.block_light_rgb.push([0.0, 0.0, 0.0]);
            mesh.sky_exposed.push(1.0);
            mesh.material.push(material);
        }
        mesh.indices.push(base);
        mesh.indices.push(base + 1);
        mesh.indices.push(base + 2);
        mesh.indices.push(base);
        mesh.indices.push(base + 2);
        mesh.indices.push(base + 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_analyze_empty_chunk() {
        let grid = VoxelGrid::new(16);
        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::Empty);
        assert_eq!(smooth, 0);
        assert_eq!(blocky, 0);
    }

    #[test]
    fn analyze_chunk_classifies_uniform_chunks() {
        let rows = [
            (0, ChunkContent::AllSmooth, 64, 0),
            (99, ChunkContent::AllSmooth, 64, 0),
            (100, ChunkContent::AllBlocky, 0, 64),
        ];

        for (material, expected_content, expected_smooth, expected_blocky) in rows {
            let mut grid = VoxelGrid::new(16);
            for x in 0..4 {
                for y in 0..4 {
                    for z in 0..4 {
                        grid.place_block(IVec3::new(x, y, z), material, None);
                    }
                }
            }

            let (content, smooth, blocky) =
                HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

            assert_eq!(content, expected_content, "material {material}");
            assert_eq!(smooth, expected_smooth, "material {material}");
            assert_eq!(blocky, expected_blocky, "material {material}");
        }
    }

    #[test]
    fn test_analyze_mixed_chunk() {
        let mut grid = VoxelGrid::new(16);

        // Add mix of smooth and blocky blocks
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    let pos = IVec3::new(x, y, z);
                    grid.place_block(pos, 0, None); // Smooth
                }
            }
        }

        for x in 2..4 {
            for y in 2..4 {
                for z in 2..4 {
                    let pos = IVec3::new(x, y, z);
                    grid.place_block(pos, 100, None); // Blocky
                }
            }
        }

        let (content, smooth, blocky) = HybridMeshGenerator::analyze_chunk(&grid, IVec3::ZERO, 16);

        assert_eq!(content, ChunkContent::Mixed);
        assert_eq!(smooth, 8);
        assert_eq!(blocky, 8);
    }

    #[test]
    fn test_generate_empty_chunk() {
        let grid = VoxelGrid::new(16);
        let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::ZERO, 16);

        assert_eq!(mesh.vertices.len(), 0);
        assert_eq!(mesh.indices.len(), 0);
    }

    #[test]
    fn blocky_chunk_mesh_sits_at_its_chunk_origin() {
        let mut grid = VoxelGrid::new(16);
        grid.place_block(IVec3::new(17, 3, -14), 100, None);

        let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, IVec3::new(1, 0, -1), 16);

        assert_eq!(mesh.vertices.len(), 24);
        let min = mesh
            .vertices
            .iter()
            .fold(Vec3::splat(f32::MAX), |acc, v| acc.min(Vec3::from(*v)));
        let max = mesh
            .vertices
            .iter()
            .fold(Vec3::splat(f32::MIN), |acc, v| acc.max(Vec3::from(*v)));
        assert_eq!(min, Vec3::new(17.0, 3.0, -14.0));
        assert_eq!(max, Vec3::new(18.0, 4.0, -13.0));
    }

    #[test]
    fn test_generate_coarse_mesh_empty_grid() {
        let grid = VoxelGrid::new(16);
        let mesh = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::ZERO, 16);
        assert_eq!(mesh.vertices.len(), 0);
        assert_eq!(mesh.indices.len(), 0);
    }

    #[test]
    fn test_generate_coarse_mesh_solid_chunk_has_geometry() {
        let mut grid = VoxelGrid::new(16);
        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    grid.place_block(IVec3::new(x, y, z), 0, None);
                }
            }
        }

        let mesh = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::ZERO, 16);

        assert_eq!(mesh.vertices.len(), 2560);
        assert_eq!(mesh.indices.len(), 3840);
        assert!(mesh.indices.iter().all(|&i| (i as usize) < 2560));
        let facing_pos_x = mesh
            .normals
            .iter()
            .filter(|n| **n == [1.0, 0.0, 0.0])
            .count();
        assert_eq!(facing_pos_x, 4 * 128, "128 quads facing +X");
    }

    #[test]
    fn test_generate_coarse_mesh_uses_stride2() {
        let mut single = VoxelGrid::new(16);
        single.place_block(IVec3::new(1, 1, 1), 0, None);
        let mut full_cell = VoxelGrid::new(16);
        for dx in 0..2 {
            for dy in 0..2 {
                for dz in 0..2 {
                    full_cell.place_block(IVec3::new(dx, dy, dz), 0, None);
                }
            }
        }

        let sparse = HybridMeshGenerator::generate_coarse_mesh(&single, IVec3::ZERO, 16);
        let dense = HybridMeshGenerator::generate_coarse_mesh(&full_cell, IVec3::ZERO, 16);

        assert!(!sparse.vertices.is_empty());
        assert_eq!(sparse.vertices, dense.vertices);
        assert_eq!(sparse.indices, dense.indices);
    }

    type Quad = ([i32; 3], [i32; 3], [i32; 3]);

    /// The 2x2-block quads covering the outside of the box `lo..hi`, as (normal, min, max).
    fn box_quads(lo: [i32; 3], hi: [i32; 3]) -> Vec<Quad> {
        let mut quads = Vec::new();
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for sign in [-1, 1] {
                let plane = if sign < 0 { lo[axis] } else { hi[axis] };
                for a in (lo[u]..hi[u]).step_by(2) {
                    for b in (lo[v]..hi[v]).step_by(2) {
                        let (mut normal, mut min, mut max) = ([0; 3], [0; 3], [0; 3]);
                        normal[axis] = sign;
                        (min[axis], max[axis]) = (plane, plane);
                        (min[u], max[u]) = (a, a + 2);
                        (min[v], max[v]) = (b, b + 2);
                        quads.push((normal, min, max));
                    }
                }
            }
        }
        quads
    }

    fn mesh_quads(mesh: &VoxelMesh) -> Vec<Quad> {
        assert_eq!(mesh.vertices.len() % 4, 0);
        let mut quads: Vec<Quad> = mesh
            .vertices
            .chunks(4)
            .zip(mesh.normals.chunks(4))
            .map(|(verts, normals)| {
                assert!(normals.iter().all(|n| *n == normals[0]));
                let mut min = [i32::MAX; 3];
                let mut max = [i32::MIN; 3];
                for v in verts {
                    for a in 0..3 {
                        assert_eq!(v[a].fract(), 0.0, "vertex {v:?} off the integer lattice");
                        min[a] = min[a].min(v[a] as i32);
                        max[a] = max[a].max(v[a] as i32);
                    }
                }
                let mut corners: Vec<[i32; 3]> =
                    verts.iter().map(|v| v.map(|c| c as i32)).collect();
                assert!(
                    corners
                        .iter()
                        .all(|c| (0..3).all(|a| c[a] == min[a] || c[a] == max[a])),
                    "quad {verts:?} has a vertex off its bounding rectangle's corners"
                );
                corners.sort_unstable();
                corners.dedup();
                assert_eq!(corners.len(), 4, "quad {verts:?} repeats a corner");
                (normals[0].map(|c| c as i32), min, max)
            })
            .collect();
        quads.sort_unstable();
        quads
    }

    fn assert_coarse_fixture_in_chunk(chunk_pos: IVec3) {
        let origin = chunk_pos * 16;
        let mut grid = VoxelGrid::new(16);
        let mut place = |local: [i32; 3]| {
            grid.place_block(origin + IVec3::from(local), 0, None);
        };
        for x in 4..8 {
            for y in 4..8 {
                for z in 4..8 {
                    place([x, y, z]);
                }
            }
        }
        place([11, 11, 11]);
        // Blocks against each horizontal chunk edge, where a skirt quad hangs one cell below.
        place([0, 8, 8]);
        place([15, 8, 8]);
        place([8, 8, 15]);
        place([8, 8, 0]);

        let mesh = HybridMeshGenerator::generate_coarse_mesh(&grid, chunk_pos, 16);

        let mut expected = box_quads([4, 4, 4], [8, 8, 8]);
        expected.extend(box_quads([10, 10, 10], [12, 12, 12]));
        expected.extend(box_quads([0, 8, 8], [2, 10, 10]));
        expected.extend(box_quads([14, 8, 8], [16, 10, 10]));
        expected.extend(box_quads([8, 8, 14], [10, 10, 16]));
        expected.extend(box_quads([8, 8, 0], [10, 10, 2]));
        expected.push(([-1, 0, 0], [0, 6, 8], [0, 8, 10]));
        expected.push(([1, 0, 0], [16, 6, 8], [16, 8, 10]));
        expected.push(([0, 0, 1], [8, 6, 16], [10, 8, 16]));
        expected.push(([0, 0, -1], [8, 6, 0], [10, 8, 0]));
        let o = origin.to_array();
        let mut expected: Vec<Quad> = expected
            .into_iter()
            .map(|(n, min, max)| {
                (
                    n,
                    [0, 1, 2].map(|a| min[a] + o[a]),
                    [0, 1, 2].map(|a| max[a] + o[a]),
                )
            })
            .collect();
        expected.sort_unstable();
        assert_eq!(mesh_quads(&mesh), expected);
        assert_eq!(mesh.indices.len(), mesh.vertices.len() / 4 * 6);
    }

    #[test]
    fn coarse_mesh_emits_exposed_faces_on_the_stride_lattice() {
        assert_coarse_fixture_in_chunk(IVec3::ZERO);
    }

    #[test]
    fn coarse_mesh_offsets_quads_by_the_chunk_origin() {
        assert_coarse_fixture_in_chunk(IVec3::new(1, 0, -1));
    }

    type Block = ([i32; 3], u32, Option<u32>);

    fn grid_with(blocks: impl IntoIterator<Item = Block>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for (pos, material, resource) in blocks {
            grid.place_block(IVec3::from(pos), material, resource);
        }
        grid
    }

    /// Blocks `lo..=hi` on every axis, all of one material.
    fn solid_box(lo: [i32; 3], hi: [i32; 3], material: u32) -> Vec<Block> {
        let mut blocks = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    blocks.push(([x, y, z], material, None));
                }
            }
        }
        blocks
    }

    /// Each triangle's centroid, its vertex normal's y, and its three vertex materials.
    fn triangles(mesh: &VoxelMesh) -> Vec<(Vec3, f32, [u32; 3])> {
        assert_eq!(
            mesh.material.len(),
            mesh.vertices.len(),
            "one material per vertex"
        );
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|tri| {
                let at = |k: usize| Vec3::from(mesh.vertices[tri[k] as usize]);
                (
                    (at(0) + at(1) + at(2)) / 3.0,
                    mesh.normals[tri[0] as usize][1],
                    tri.map(|i| mesh.material[i as usize]),
                )
            })
            .collect()
    }

    fn chunk_mesh(grid: &VoxelGrid) -> VoxelMesh {
        HybridMeshGenerator::generate_chunk_mesh(grid, IVec3::ZERO, 16)
    }

    #[test]
    fn exposed_smooth_block_carries_its_face_material() {
        let rows = [
            ("soil", 1, None, 1, 2),
            ("stone", 2, None, 2, 1),
            ("stone with iron ore", 2, Some(1), 3, 1),
        ];

        for (name, material, resource, expected, slab) in rows {
            let mut blocks = solid_box([2, 0, 2], [13, 7, 13], slab);
            blocks.push(([8, 8, 8], material, resource));
            let mesh = chunk_mesh(&grid_with(blocks));
            let probe = Vec3::splat(8.5);

            let all = triangles(&mesh);
            let near: Vec<_> = all
                .iter()
                .filter(|(c, ..)| c.distance(probe) < 0.4)
                .collect();
            let far: Vec<_> = all
                .iter()
                .filter(|(c, ..)| c.distance(probe) > 2.0)
                .collect();

            assert!(!near.is_empty(), "{name}: triangles around the block");
            for (c, _, materials) in &near {
                assert_eq!(*materials, [expected; 3], "{name}: triangle at {c:?}");
            }
            assert!(!far.is_empty(), "{name}: triangles on the slab");
            for (c, _, materials) in &far {
                assert_eq!(*materials, [slab; 3], "{name}: slab triangle at {c:?}");
            }
        }
    }

    #[test]
    fn upward_triangles_over_grass_carry_grass() {
        let mut blocks = solid_box([2, 0, 2], [13, 5, 13], 2);
        blocks.extend(solid_box([2, 6, 2], [9, 6, 13], 2));
        blocks.extend(solid_box([2, 7, 2], [9, 7, 13], 0));
        let mesh = chunk_mesh(&grid_with(blocks));

        let flat_up = |height: f32| {
            triangles(&mesh)
                .into_iter()
                .filter(|(c, normal_y, _)| *normal_y > 0.9 && (c.y - height).abs() < 1e-3)
                .collect::<Vec<_>>()
        };
        let plateau: Vec<_> = flat_up(8.0)
            .into_iter()
            .filter(|(c, ..)| (3.0..9.0).contains(&c.x))
            .collect();
        let lower: Vec<_> = flat_up(6.0)
            .into_iter()
            .filter(|(c, ..)| c.x > 11.0)
            .collect();

        assert!(!plateau.is_empty(), "the grass top has flat triangles");
        for (c, _, materials) in &plateau {
            assert_eq!(*materials, [0; 3], "grass top at {c:?}");
        }
        assert!(!lower.is_empty(), "the lower ground has flat triangles");
        for (c, _, materials) in &lower {
            assert_eq!(*materials, [2; 3], "stone top at {c:?}");
        }
    }

    #[test]
    fn mining_soil_exposes_stone_material() {
        let soil = IVec3::new(8, 6, 8);
        let mut blocks = solid_box([2, 0, 2], [13, 5, 13], 2);
        blocks.push(([8, 6, 8], 1, None));
        let mut grid = grid_with(blocks);
        let before = chunk_mesh(&grid);
        assert!(before.material.contains(&1), "the soil block draws as soil");

        assert!(grid.clear_block(soil));
        let after = chunk_mesh(&grid);

        assert!(!after.material.contains(&1), "no vertex still carries soil");
        let over_hole: Vec<_> = triangles(&after)
            .into_iter()
            .filter(|(c, ..)| (c.x - 8.5).abs() < 0.7 && (c.z - 8.5).abs() < 0.7 && c.y >= 5.9)
            .collect();
        assert!(!over_hole.is_empty(), "the hole has a floor");
        for (c, _, materials) in &over_hole {
            assert_eq!(*materials, [2; 3], "exposed face at {c:?}");
        }
        assert!(after.material.iter().all(|&m| m == 2));
    }

    #[test]
    fn border_triangles_take_the_material_of_a_neighbouring_chunks_block() {
        let grid = grid_with([([15, 8, 8], 1, None), ([16, 8, 8], 2, None)]);

        let mesh = chunk_mesh(&grid);

        let all = triangles(&mesh);
        let soil_side: Vec<_> = all.iter().filter(|(c, ..)| c.x < 15.999).collect();
        let stone_side: Vec<_> = all.iter().filter(|(c, ..)| c.x > 16.001).collect();
        assert!(
            !soil_side.is_empty(),
            "the block inside the chunk is meshed"
        );
        assert!(
            !stone_side.is_empty(),
            "the border cell reaches the padding block"
        );
        for (c, _, materials) in soil_side {
            assert_eq!(*materials, [1; 3], "triangle at {c:?}");
        }
        for (c, _, materials) in stone_side {
            assert_eq!(*materials, [2; 3], "triangle at {c:?}");
        }
    }

    #[test]
    fn smooth_triangles_in_a_chunk_off_the_origin_carry_their_own_blocks_material() {
        let chunk = IVec3::new(1, 1, 0);
        let grid = grid_with([([20, 20, 4], 2, None), ([21, 20, 4], 1, None)]);

        let mesh = HybridMeshGenerator::generate_chunk_mesh(&grid, chunk, 16);

        let all = triangles(&mesh);
        let stone: Vec<_> = all.iter().filter(|(c, ..)| c.x < 20.99).collect();
        let soil: Vec<_> = all.iter().filter(|(c, ..)| c.x > 21.01).collect();
        assert!(!stone.is_empty() && !soil.is_empty());
        for (c, _, materials) in stone {
            assert_eq!(*materials, [2; 3], "triangle at {c:?}");
        }
        for (c, _, materials) in soil {
            assert_eq!(*materials, [1; 3], "triangle at {c:?}");
        }
    }

    #[test]
    fn smooth_triangles_in_a_mixed_chunk_never_carry_a_blocky_blocks_material() {
        let mut blocks = solid_box([2, 0, 2], [10, 3, 10], 2);
        blocks.push(([6, 4, 6], 100, None));
        blocks.push(([7, 4, 6], 101, None));
        let mesh = chunk_mesh(&grid_with(blocks));

        assert!(mesh.geometry_type.contains(&0) && mesh.geometry_type.contains(&1));
        for (material, geometry) in mesh.material.iter().zip(&mesh.geometry_type) {
            match geometry {
                0 => assert_eq!(*material, 2, "smooth vertex"),
                _ => assert!([100, 101].contains(material), "blocky vertex {material}"),
            }
        }
    }

    fn cell(cx: i32, cy: i32, cz: i32) -> IVec3 {
        IVec3::new(cx, cy, cz)
    }

    /// The block of coarse cell `(0, 0, 0)` at `offset` (each 0 or 1).
    fn cell_block(offset: [i32; 3], material: u32, resource: Option<u32>) -> Block {
        (offset, material, resource)
    }

    /// The two blocks of the 2x2 layer of cell `(0, 0, 0)` touching `face`, as offsets.
    fn layer_offsets(face: FaceDirection, inner: bool) -> Vec<[i32; 3]> {
        let (axis, positive) = match face {
            FaceDirection::PosX => (0, true),
            FaceDirection::NegX => (0, false),
            FaceDirection::PosY => (1, true),
            FaceDirection::NegY => (1, false),
            FaceDirection::PosZ => (2, true),
            FaceDirection::NegZ => (2, false),
        };
        let layer = i32::from(positive != inner);
        let mut offsets = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                let mut offset = [0; 3];
                offset[axis] = layer;
                offset[(axis + 1) % 3] = a;
                offset[(axis + 2) % 3] = b;
                offsets.push(offset);
            }
        }
        offsets
    }

    const FACES: [FaceDirection; 6] = [
        FaceDirection::PosX,
        FaceDirection::NegX,
        FaceDirection::PosY,
        FaceDirection::NegY,
        FaceDirection::PosZ,
        FaceDirection::NegZ,
    ];

    #[test]
    fn coarse_face_takes_the_first_solid_block_of_its_face_layer() {
        for face in FACES {
            let touching = layer_offsets(face, false);
            let opposite = layer_offsets(face, true);
            let mut blocks: Vec<Block> = opposite
                .iter()
                .map(|&offset| cell_block(offset, 1, None))
                .collect();
            blocks.push(cell_block(touching[2], 2, None));
            blocks.push(cell_block(touching[3], 100, None));
            let grid = grid_with(blocks);

            let material = coarse_face_material(&grid, IVec3::ZERO, cell(0, 0, 0), face);

            assert_eq!(material, 2, "{face:?}: skips the empty blocks of the layer");
        }
        let ore_on_top = grid_with([cell_block([1, 1, 0], 2, Some(1))]);
        assert_eq!(
            coarse_face_material(&ore_on_top, IVec3::ZERO, cell(0, 0, 0), FaceDirection::PosY),
            3,
            "an ore block gives its resource's material"
        );
    }

    #[test]
    fn coarse_face_falls_back_to_the_inner_layer() {
        for face in FACES {
            let opposite = layer_offsets(face, true);
            let grid = grid_with([cell_block(opposite[3], 2, None)]);

            let material = coarse_face_material(&grid, IVec3::ZERO, cell(0, 0, 0), face);

            assert_eq!(material, 2, "{face:?}: the touching layer is empty");
        }
    }

    #[test]
    fn coarse_face_layer_order_is_x_then_z_then_y() {
        let grid = grid_with([
            cell_block([1, 1, 0], 1, None),
            cell_block([0, 1, 1], 2, None),
        ]);

        let top = coarse_face_material(&grid, IVec3::ZERO, cell(0, 0, 0), FaceDirection::PosY);

        assert_eq!(top, 2, "x varies slowest: (0, 1) comes before (1, 0)");
        let column = grid_with([
            cell_block([0, 1, 0], 1, None),
            cell_block([0, 0, 1], 2, None),
        ]);
        let side = coarse_face_material(&column, IVec3::ZERO, cell(0, 0, 0), FaceDirection::NegX);
        assert_eq!(
            side, 1,
            "within a layer on x, z before y: (z 0, y 1) comes before (z 1, y 0)"
        );
    }

    #[test]
    fn coarse_face_reads_the_cell_at_its_chunk_offset() {
        let grid = grid_with([([18, -14, 5], 2, None), ([19, -13, 5], 1, None)]);
        let base = IVec3::new(16, -16, 0);

        let top = coarse_face_material(&grid, base, cell(1, 1, 2), FaceDirection::PosY);
        let bottom = coarse_face_material(&grid, base, cell(1, 1, 2), FaceDirection::NegY);

        assert_eq!(top, 1);
        assert_eq!(bottom, 2);
    }

    #[test]
    fn coarse_quads_and_skirts_carry_their_faces_material() {
        let grid = grid_with([([14, 0, 0], 1, None), ([15, 1, 1], 2, None)]);

        let mesh = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::ZERO, 16);

        assert_eq!(mesh.material.len(), mesh.vertices.len());
        assert!(!mesh.vertices.is_empty());
        for (normal, material) in mesh.normals.iter().zip(&mesh.material) {
            let positive = normal.iter().sum::<f32>() > 0.0;
            assert_eq!(*material, if positive { 2 } else { 1 }, "normal {normal:?}");
        }
        let skirts = mesh
            .vertices
            .iter()
            .zip(&mesh.normals)
            .filter(|(v, n)| **n == [1.0, 0.0, 0.0] && v[1] < 0.0)
            .count();
        assert!(skirts > 0, "the +X skirt hangs below the cell");
    }

    fn block_strategy() -> impl Strategy<Value = Block> {
        (
            (0..7i32, 0..7i32, 0..7i32),
            prop::sample::select(vec![1u32, 2, 100, 101]),
            prop::sample::select(vec![None, Some(1u32)]),
        )
            .prop_map(|((x, y, z), material, resource)| ([x, y, z], material, resource))
    }

    fn face_materials(grid: &VoxelGrid, blocks: &[Block]) -> Vec<u32> {
        blocks
            .iter()
            .map(|(_, material, resource)| {
                grid.material_registry.face_material(*material, *resource)
            })
            .collect()
    }

    fn assert_one_material_per_triangle(
        mesh: &VoxelMesh,
        allowed: &[u32],
    ) -> Result<(), TestCaseError> {
        prop_assert_eq!(mesh.material.len(), mesh.vertices.len());
        for (k, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            let materials = tri.map(|i| mesh.material[i as usize]);
            prop_assert!(
                materials[0] == materials[1] && materials[1] == materials[2],
                "triangle {} mixes {:?}",
                k,
                materials
            );
            prop_assert!(
                allowed.contains(&materials[0]),
                "triangle {} carries {}, not the face material of any placed block {:?}",
                k,
                materials[0],
                allowed
            );
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn every_triangle_has_one_material(
            blocks in prop::collection::vec(block_strategy(), 1..120),
            cut in 0.0f32..7.0,
        ) {
            let grid = grid_with(blocks.clone());
            let allowed = face_materials(&grid, &blocks);

            let fine = chunk_mesh(&grid);
            let coarse = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::ZERO, 16);
            let caps = [0u8, 1].map(|lod| crate::mesh::cut_cap_mesh(&grid, IVec3::ZERO, lod, cut));

            assert_one_material_per_triangle(&fine, &allowed)?;
            assert_one_material_per_triangle(&coarse, &allowed)?;
            for cap in &caps {
                assert_one_material_per_triangle(cap, &allowed)?;
            }
        }
    }
}
