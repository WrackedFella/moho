use glam::IVec3;
use moho_core::voxel::{VoxelChunk, VoxelGrid};

const TOLERANCE: f32 = 1e-4;
const SMOOTH_MATERIAL: u32 = 1;
const BLOCKY_MATERIAL: u32 = 100;
const SMOOTH_GEOMETRY: u32 = 0;
const BLOCKY_GEOMETRY: u32 = 1;

fn grid_with(blocks: &[(IVec3, u32)]) -> VoxelGrid {
    let mut grid = VoxelGrid::new(16);
    for &(pos, material) in blocks {
        grid.mutator().place(pos, material, None);
    }
    grid
}

/// Axis-aligned bounds of the vertices whose geometry type is `geometry`.
fn bbox_of(chunk: &VoxelChunk, geometry: u32) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let selected = chunk
        .vertices()
        .iter()
        .zip(chunk.geometry_type())
        .filter(|&(_, &g)| g == geometry);
    for (p, _) in selected {
        for a in 0..3 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    (min, max)
}

#[test]
fn mixed_chunk_indices_are_in_range() {
    let grid = grid_with(&[
        (IVec3::new(5, 5, 5), SMOOTH_MATERIAL),
        (IVec3::new(12, 12, 12), BLOCKY_MATERIAL),
    ]);

    let chunk = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);

    let count = chunk.vertices().len();
    assert!(!chunk.indices().is_empty(), "expected indexed geometry");
    assert!(
        chunk.geometry_type().contains(&SMOOTH_GEOMETRY)
            && chunk.geometry_type().contains(&BLOCKY_GEOMETRY),
        "expected both smooth and blocky vertices in the mixed chunk"
    );
    let worst = chunk.indices().iter().copied().max().unwrap_or(0);
    assert!(
        (worst as usize) < count,
        "index {worst} out of range for {count} vertices"
    );
}

#[test]
fn mixed_chunk_smooth_part_is_aligned() {
    let smooth = IVec3::new(5, 5, 5);
    let grid = grid_with(&[
        (smooth, SMOOTH_MATERIAL),
        (IVec3::new(12, 12, 12), BLOCKY_MATERIAL),
    ]);

    let chunk = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, 0);

    let (min, max) = bbox_of(&chunk, SMOOTH_GEOMETRY);
    assert!(
        min[0].is_finite(),
        "expected smooth vertices in mixed chunk"
    );
    for a in 0..3 {
        let expected = smooth[a] as f32 + 0.5;
        let center = (min[a] + max[a]) * 0.5;
        assert!(
            (center - expected).abs() < TOLERANCE,
            "axis {a}: smooth part of mixed chunk centred at {center}, expected {expected} \
             (bbox {min:?}..{max:?})"
        );
    }
}

/// How a case's bounds are checked.
enum Span {
    /// The bbox centre must equal this value on every axis.
    Centre(f32),
    /// The bbox must be exactly `[lo, hi]` on every axis.
    Exact(f32, f32),
}

struct Case {
    name: &'static str,
    blocks: Vec<(IVec3, u32)>,
    lod: u8,
    geometry: u32,
    span: Span,
}

/// Every mesh path must occupy the world-space cube that the voxel grid,
/// raycasting and the other mesh paths assume: block `V` fills `[V, V+1]`.
///
/// Regression guard for smooth terrain once rendering half a unit off-centre,
/// which made mining hit the block behind the visible slope.
#[test]
fn mesh_is_centered_on_the_voxel_it_represents() {
    let origin = IVec3::new(4, 4, 4);
    let cell: Vec<(IVec3, u32)> = (0..8)
        .map(|i| {
            let offset = IVec3::new(i & 1, (i >> 1) & 1, (i >> 2) & 1);
            (origin + offset, SMOOTH_MATERIAL)
        })
        .collect();
    let cases = [
        Case {
            name: "smooth block at (5,5,5), LOD 0",
            blocks: vec![(IVec3::new(5, 5, 5), SMOOTH_MATERIAL)],
            lod: 0,
            geometry: SMOOTH_GEOMETRY,
            span: Span::Centre(5.5),
        },
        Case {
            name: "blocky block at (5,5,5), LOD 0",
            blocks: vec![(IVec3::new(5, 5, 5), BLOCKY_MATERIAL)],
            lod: 0,
            geometry: BLOCKY_GEOMETRY,
            span: Span::Exact(5.0, 6.0),
        },
        Case {
            name: "smooth 2x2x2 cell at (4,4,4), LOD 1",
            blocks: cell,
            lod: 1,
            // The coarse path emits blocky-typed cubes even for smooth materials.
            geometry: BLOCKY_GEOMETRY,
            span: Span::Exact(4.0, 6.0),
        },
    ];

    let mut failures = Vec::new();
    for case in cases {
        let grid = grid_with(&case.blocks);

        let chunk = VoxelChunk::from_grid_lod(&grid, IVec3::ZERO, case.lod);

        let (min, max) = bbox_of(&chunk, case.geometry);
        for a in 0..3 {
            let ok = match case.span {
                Span::Centre(c) => ((min[a] + max[a]) * 0.5 - c).abs() < TOLERANCE,
                Span::Exact(lo, hi) => {
                    (min[a] - lo).abs() < TOLERANCE && (max[a] - hi).abs() < TOLERANCE
                }
            };
            if !ok {
                failures.push(format!(
                    "case '{}': axis {a} bbox {min:?}..{max:?} does not match expected span",
                    case.name
                ));
                break;
            }
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
