//! Remeshing after a block is removed must leave the terrain surface closed across chunk seams.

use std::collections::HashMap;

use flagstone_voxel::{HybridMeshGenerator, VoxelGrid, VoxelMesh, chunks_meshed_from};
use glam::IVec3;
use proptest::prelude::*;

const CHUNK: i32 = 16;
const SOIL: u32 = 1;

type Meshes = HashMap<IVec3, VoxelMesh>;

fn mesh_all(grid: &VoxelGrid, chunks: impl IntoIterator<Item = IVec3>) -> Meshes {
    chunks
        .into_iter()
        .map(|c| (c, HybridMeshGenerator::generate_chunk_mesh(grid, c, CHUNK)))
        .collect()
}

fn chunk_box(lo: [i32; 3], hi: [i32; 3]) -> Vec<IVec3> {
    let mut chunks = Vec::new();
    for x in lo[0]..hi[0] {
        for y in lo[1]..hi[1] {
            for z in lo[2]..hi[2] {
                chunks.push(IVec3::new(x, y, z));
            }
        }
    }
    chunks
}

/// Remeshes `chunks` (those present in `meshes`) from `grid`, leaving the rest stale.
fn remesh(grid: &VoxelGrid, meshes: &mut Meshes, chunks: impl IntoIterator<Item = IVec3>) {
    for chunk in chunks {
        if let Some(slot) = meshes.get_mut(&chunk) {
            *slot = HybridMeshGenerator::generate_chunk_mesh(grid, chunk, CHUNK);
        }
    }
}

/// Mesh edges used by exactly one triangle once vertices are welded by position, ignoring
/// edges that lie along the outer boundary of the region `[lo, hi]` (in mesh space).
fn open_edges(meshes: &Meshes, lo: [f32; 3], hi: [f32; 3]) -> usize {
    const BOUNDARY_MARGIN: f32 = 1.0;
    let mut ids: HashMap<[i32; 3], u32> = HashMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uses: HashMap<(u32, u32), u32> = HashMap::new();

    for mesh in meshes.values() {
        let welded: Vec<u32> = mesh
            .vertices
            .iter()
            .map(|v| {
                let key = v.map(|c| (c * 1000.0).round() as i32);
                *ids.entry(key).or_insert_with(|| {
                    positions.push(*v);
                    (positions.len() - 1) as u32
                })
            })
            .collect();
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| welded[i as usize]);
            for (p, q) in [(a, b), (b, c), (c, a)] {
                if p != q {
                    *uses.entry((p.min(q), p.max(q))).or_insert(0) += 1;
                }
            }
        }
    }

    let on_outer_boundary = |p: u32, q: u32| {
        let (p, q) = (positions[p as usize], positions[q as usize]);
        (0..3).any(|a| {
            (p[a] <= lo[a] + BOUNDARY_MARGIN && q[a] <= lo[a] + BOUNDARY_MARGIN)
                || (p[a] >= hi[a] - BOUNDARY_MARGIN && q[a] >= hi[a] - BOUNDARY_MARGIN)
        })
    };
    uses.iter()
        .filter(|&(&(p, q), &count)| count == 1 && !on_outer_boundary(p, q))
        .count()
}

fn fill_column(grid: &mut VoxelGrid, x: i32, z: i32, height: i32) {
    for y in 0..height {
        grid.mutator().place(IVec3::new(x, y, z), SOIL, None);
    }
}

#[test]
fn edge_removal_remeshing_the_dependents_leaves_no_open_edge() {
    let mut grid = VoxelGrid::new(CHUNK);
    for x in 0..48 {
        for z in 0..48 {
            fill_column(&mut grid, x, z, 8);
        }
    }
    let region = chunk_box([0, 0, 0], [3, 1, 3]);
    let (lo, hi) = ([-0.5, -0.5, -0.5], [47.5, 15.5, 47.5]);
    let mut meshes = mesh_all(&grid, region);
    assert_eq!(open_edges(&meshes, lo, hi), 0, "the slab starts closed");
    let mined = IVec3::new(16, 7, 24);
    assert!(grid.clear_block(mined));
    let mut own_chunk_only = meshes.clone();

    remesh(&grid, &mut own_chunk_only, [IVec3::new(1, 0, 1)]);
    remesh(&grid, &mut meshes, chunks_meshed_from(mined, CHUNK));

    assert!(
        open_edges(&own_chunk_only, lo, hi) > 0,
        "remeshing only the block's chunk leaves the seam open"
    );
    assert_eq!(open_edges(&meshes, lo, hi), 0);
}

fn assert_same_mesh(
    chunk: IVec3,
    incremental: &VoxelMesh,
    full: &VoxelMesh,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        &incremental.vertices,
        &full.vertices,
        "vertices of {}",
        chunk
    );
    prop_assert_eq!(&incremental.indices, &full.indices, "indices of {}", chunk);
    prop_assert_eq!(&incremental.normals, &full.normals, "normals of {}", chunk);
    prop_assert_eq!(
        &incremental.material,
        &full.material,
        "materials of {}",
        chunk
    );
    Ok(())
}

fn seam_biased(max: i32) -> impl Strategy<Value = i32> {
    prop_oneof![
        prop::sample::select(
            vec![0, 1, 15, 16, 17, 31, 32, 33, 47]
                .into_iter()
                .filter(|c| *c < max)
                .collect::<Vec<_>>()
        ),
        0..max,
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn remeshing_the_dependents_equals_a_full_remesh(
        heights in prop::collection::vec(1..9i32, 9),
        extra in prop::collection::vec((0..48i32, 0..10i32, 0..48i32), 0..200),
        removals in prop::collection::vec((seam_biased(48), 0..8i32, seam_biased(48)), 1..12),
    ) {
        let mut grid = VoxelGrid::new(CHUNK);
        for x in 0..48 {
            for z in 0..48 {
                let height = heights[((x / 16) * 3 + z / 16) as usize].max(4);
                fill_column(&mut grid, x, z, height);
            }
        }
        for (x, y, z) in extra {
            grid.mutator().place(IVec3::new(x, y, z), SOIL, None);
        }
        let region = chunk_box([0, -1, 0], [3, 1, 3]);
        let mut incremental = mesh_all(&grid, region.clone());

        for (x, y, z) in removals {
            let block = IVec3::new(x, y, z);
            grid.clear_block(block);
            remesh(&grid, &mut incremental, chunks_meshed_from(block, CHUNK));
        }

        let full = mesh_all(&grid, region);
        for (chunk, mesh) in &incremental {
            assert_same_mesh(*chunk, mesh, &full[chunk])?;
        }
    }
}

#[test]
fn surface_stays_closed_after_removal_beside_an_intersection() {
    let mut grid = VoxelGrid::new(CHUNK);
    for x in 0..48 {
        for z in 0..48 {
            fill_column(&mut grid, x, z, x.min(z) / 2 + 2);
        }
    }
    let region = chunk_box([0, 0, 0], [3, 2, 3]);
    let (lo, hi) = ([-0.5, -0.5, -0.5], [47.5, 31.5, 47.5]);
    let mut meshes = mesh_all(&grid, region);
    assert_eq!(open_edges(&meshes, lo, hi), 0, "the ridge starts closed");

    for (x, z) in [(20, 19), (26, 25), (37, 36)] {
        let top = IVec3::new(x, x.min(z) / 2 + 1, z);
        assert!(grid.clear_block(top));
        remesh(&grid, &mut meshes, chunks_meshed_from(top, CHUNK));
    }

    assert_eq!(open_edges(&meshes, lo, hi), 0);
}
