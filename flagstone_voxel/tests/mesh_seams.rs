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

/// Compares the geometry (vertex positions and indices) and the ambient occlusion. Normals
/// come from central differences, so a block at local 15 also shades the positive
/// neighbour; that stale shading is a known seam that opens no gap and is out of scope here.
fn assert_same_mesh(
    chunk: IVec3,
    incremental: &VoxelMesh,
    full: &VoxelMesh,
) -> Result<(), TestCaseError> {
    assert_same_stream(chunk, "vertices", &incremental.vertices, &full.vertices)?;
    assert_same_stream(chunk, "indices", &incremental.indices, &full.indices)?;
    assert_same_stream(
        chunk,
        "ambient occlusion",
        &incremental.ambient_occlusion,
        &full.ambient_occlusion,
    )
}

/// Fails naming the chunk, the stream and the first differing index, not the whole arrays.
fn assert_same_stream<T: PartialEq + std::fmt::Debug>(
    chunk: IVec3,
    stream: &str,
    incremental: &[T],
    full: &[T],
) -> Result<(), TestCaseError> {
    let first_difference =
        (0..incremental.len().max(full.len())).find(|&i| incremental.get(i) != full.get(i));
    prop_assert!(
        first_difference.is_none(),
        "{} of chunk {} differ at index {:?}: incremental {:?}, full {:?} (lengths {} and {})",
        stream,
        chunk,
        first_difference,
        first_difference.map(|i| incremental.get(i)),
        first_difference.map(|i| full.get(i)),
        incremental.len(),
        full.len()
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
        heights in prop::collection::vec(4..9i32, 9),
        extra in prop::collection::vec((0..48i32, 0..10i32, 0..48i32), 0..200),
        removals in prop::collection::vec((seam_biased(48), 0..8i32, seam_biased(48)), 1..12),
    ) {
        let mut grid = VoxelGrid::new(CHUNK);
        for x in 0..48 {
            for z in 0..48 {
                let height = heights[((x / 16) * 3 + z / 16) as usize];
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

/// Ground of height 8 over the 48 x 48 region with a two-block wall raised at `wall_x`
/// beside the ground voxel (16, 8, 24), whose contact AO then darkens. The vertex on
/// that voxel's floor is emitted by the chunk at `vertex_chunk`, one chunk away from the
/// chunk holding the wall.
fn assert_wall_shades_the_neighbouring_chunk(wall_x: i32, vertex_chunk: IVec3) {
    let mut grid = VoxelGrid::new(CHUNK);
    for x in 0..48 {
        for z in 0..48 {
            fill_column(&mut grid, x, z, 8);
        }
    }
    let region = chunk_box([0, -1, 0], [3, 1, 3]);
    let mut incremental = mesh_all(&grid, region.clone());

    for y in 8..10 {
        for z in 23..=25 {
            let block = IVec3::new(wall_x, y, z);
            grid.mutator().place(block, SOIL, None);
            remesh(&grid, &mut incremental, chunks_meshed_from(block, CHUNK));
        }
    }

    let full = mesh_all(&grid, region);
    let ao_at = |mesh: &VoxelMesh, vertex: [f32; 3]| -> Vec<f32> {
        mesh.vertices
            .iter()
            .zip(&mesh.ambient_occlusion)
            .filter(|(v, _)| **v == vertex)
            .map(|(_, ao)| *ao)
            .collect()
    };
    let shaded = [16.5, 8.0, 24.5];
    let open = [(vertex_chunk.x * CHUNK) as f32 + 4.5, 8.0, 20.5];
    let want = ao_at(&full[&vertex_chunk], shaded);
    let got = ao_at(&incremental[&vertex_chunk], shaded);
    let open_ao = ao_at(&full[&vertex_chunk], open);
    assert!(!want.is_empty() && !open_ao.is_empty(), "both vertices are meshed");
    assert!(
        want.iter().all(|w| open_ao.iter().all(|o| w < o)),
        "the wall darkens the ground: {want:?} vs open {open_ao:?}"
    );
    assert_eq!(
        got, want,
        "chunk {vertex_chunk} shades the ground as a full remesh does"
    );
}

#[test]
fn ambient_occlusion_follows_a_wall_one_block_into_the_next_chunk() {
    assert_wall_shades_the_neighbouring_chunk(17, IVec3::new(0, 0, 1));
}

#[test]
fn ambient_occlusion_follows_a_wall_on_the_far_edge_of_the_previous_chunk() {
    assert_wall_shades_the_neighbouring_chunk(15, IVec3::new(1, 0, 1));
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

    for (x, z) in [(20, 19), (26, 25), (37, 36), (32, 32), (16, 17)] {
        let top = IVec3::new(x, x.min(z) / 2 + 1, z);
        let own = flagstone_voxel::VoxelGrid::get_chunk_pos(top, CHUNK);
        let before = meshes[&own].vertices.clone();
        assert!(grid.clear_block(top));

        remesh(&grid, &mut meshes, chunks_meshed_from(top, CHUNK));

        assert_ne!(
            meshes[&own].vertices, before,
            "removing {top} changes {own}"
        );
    }

    assert_eq!(open_edges(&meshes, lo, hi), 0);
}

const STRUCTURE: u32 = 100;

/// Rolling terrain heights, spherical cave cuts `(x, y, z, radius)`, floating slabs
/// `(x, y, z, size, thickness)` and structure blocks.
type Shape = (
    Vec<i32>,
    Vec<(i32, i32, i32, i32)>,
    Vec<(i32, i32, i32, i32, i32)>,
    Vec<(i32, i32, i32)>,
);

fn shape() -> impl Strategy<Value = Shape> {
    (
        prop::collection::vec(5..10i32, 4),
        prop::collection::vec((seam_biased(32), 1..8i32, seam_biased(32), 2..5i32), 0..4),
        prop::collection::vec(
            (
                seam_biased(24),
                10..16i32,
                seam_biased(24),
                3..9i32,
                1..3i32,
            ),
            0..3,
        ),
        prop::collection::vec((seam_biased(32), 0..10i32, seam_biased(32)), 0..12),
    )
}

fn box_offsets(size: i32, height: i32) -> Vec<IVec3> {
    let mut offsets = Vec::new();
    for x in 0..size {
        for y in 0..height {
            for z in 0..size {
                offsets.push(IVec3::new(x, y, z));
            }
        }
    }
    offsets
}

fn build(shape: &Shape) -> VoxelGrid {
    let (heights, caves, slabs, structures) = shape;
    let mut grid = VoxelGrid::new(CHUNK);
    for x in 0..32 {
        for z in 0..32 {
            fill_column(&mut grid, x, z, heights[((x / 16) * 2 + z / 16) as usize]);
        }
    }
    for &(x, y, z, size, thick) in slabs {
        for offset in box_offsets(size, thick) {
            grid.mutator()
                .place(IVec3::new(x, y, z) + offset, SOIL, None);
        }
    }
    for &(x, y, z, r) in caves {
        for offset in box_offsets(2 * r + 1, 2 * r + 1) {
            let d = offset - IVec3::splat(r);
            if d.length_squared() <= r * r {
                grid.clear_block(IVec3::new(x, y, z) + d);
            }
        }
    }
    for &(x, y, z) in structures {
        grid.mutator().place(IVec3::new(x, y, z), STRUCTURE, None);
    }
    grid
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn surface_stays_closed_after_removal_in_caves_overhangs_and_mixed_chunks(
        shape in shape(),
        (x, above, z) in (seam_biased(32), 0..20i32, seam_biased(32)),
    ) {
        let mut grid = build(&shape);
        let region = chunk_box([0, 0, 0], [2, 2, 2]);
        let (lo, hi) = ([-0.5, -0.5, -0.5], [31.5, 31.5, 31.5]);
        let mut meshes = mesh_all(&grid, region);
        prop_assume!(open_edges(&meshes, lo, hi) == 0);
        let block = (0..=above)
            .rev()
            .map(|y| IVec3::new(x, y, z))
            .find(|&p| grid.material_at(p).is_some());
        prop_assume!(block.is_some());
        let block = block.expect("assumed present above");
        assert!(grid.clear_block(block));

        remesh(&grid, &mut meshes, chunks_meshed_from(block, CHUNK));

        prop_assert_eq!(open_edges(&meshes, lo, hi), 0, "removing {} opened an edge", block);
    }
}
