//! Every mesher emits triangles that wind counter-clockwise seen from outside the
//! solid, the front-face convention the renderer culls against.

use flagstone_voxel::{
    BlockyMeshGenerator, HybridMeshGenerator, MarchingCubes, VoxelGrid, VoxelMesh,
};
use glam::{IVec3, Vec3};

const BLOCKY_MATERIAL: u32 = 100;
const SMOOTH_MATERIAL: u32 = 1;

fn position(mesh: &VoxelMesh, index: u32) -> Vec3 {
    Vec3::from_array(mesh.vertices[index as usize])
}

/// Asserts each triangle's geometric normal agrees with the vertex normal of its first
/// corner, and returns the distinct vertex normals seen.
fn assert_triangles_wind_along_vertex_normals(mesh: &VoxelMesh) -> Vec<[f32; 3]> {
    assert!(!mesh.indices.is_empty(), "mesh has no triangles");
    assert_eq!(mesh.indices.len() % 3, 0);

    let mut seen: Vec<[f32; 3]> = Vec::new();
    for tri in mesh.indices.chunks(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| position(mesh, i));
        let geometric = (b - a).cross(c - a);
        let face_normal = Vec3::from_array(mesh.normals[tri[0] as usize]);

        assert!(
            geometric.dot(face_normal) > 0.0,
            "triangle {a:?} {b:?} {c:?} with face normal {face_normal:?} winds clockwise from outside"
        );
        if !seen.contains(&mesh.normals[tri[0] as usize]) {
            seen.push(mesh.normals[tri[0] as usize]);
        }
    }
    seen
}

#[test]
fn blocky_block_faces_wind_outward() {
    let mut grid = VoxelGrid::new(16);
    let pos = IVec3::new(5, 5, 5);
    grid.mutator().place(pos, BLOCKY_MATERIAL, None);

    let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

    let normals = assert_triangles_wind_along_vertex_normals(&mesh);
    assert_eq!(normals.len(), 6, "an isolated block shows all six faces");
}

#[test]
fn coarse_mesh_faces_wind_outward() {
    let mut grid = VoxelGrid::new(16);
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                grid.mutator()
                    .place(IVec3::new(x, y, z), SMOOTH_MATERIAL, None);
            }
        }
    }
    grid.mutator()
        .place(IVec3::new(20, 4, 4), SMOOTH_MATERIAL, None);

    let solid = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::ZERO, 16);
    let lone = HybridMeshGenerator::generate_coarse_mesh(&grid, IVec3::new(1, 0, 0), 16);

    let solid_normals = assert_triangles_wind_along_vertex_normals(&solid);
    assert_eq!(solid_normals.len(), 6, "solid chunk exposes all six faces");
    let lone_normals = assert_triangles_wind_along_vertex_normals(&lone);
    assert_eq!(lone_normals.len(), 6, "lone block exposes all six faces");
}

#[test]
fn every_marching_cubes_cell_winds_away_from_solid_corners() {
    let corner = |i: usize| Vec3::from_array(MarchingCubes::CORNER_OFFSETS[i].map(|c| c as f32));

    for cube_index in 1..=254u8 {
        let is_solid = |i: usize| cube_index & (1 << i) != 0;
        // Each vertex is the midpoint of a cell edge with one solid and one empty end.
        let solid_to_empty = |vertex: Vec3| {
            let crossing = (0..8)
                .filter(|&i| is_solid(i))
                .flat_map(|i| (0..8).filter(|&j| !is_solid(j)).map(move |j| (i, j)))
                .find(|&(i, j)| ((corner(i) + corner(j)) / 2.0).abs_diff_eq(vertex, 1e-6));
            let (i, j) = crossing.unwrap_or_else(|| {
                panic!("cube index {cube_index}: {vertex:?} is on no crossing edge")
            });
            corner(j) - corner(i)
        };

        let triangles: Vec<[Vec3; 3]> = MarchingCubes::cell_triangles(cube_index).collect();

        assert!(
            !triangles.is_empty(),
            "cube index {cube_index} has no triangles"
        );
        for [a, b, c] in triangles {
            let geometric = (b - a).cross(c - a);
            let leaving_solid: f32 = [a, b, c]
                .into_iter()
                .map(|v| geometric.dot(solid_to_empty(v)))
                .sum();
            assert!(
                leaving_solid > 0.0,
                "cube index {cube_index}: triangle {a:?} {b:?} {c:?} winds towards the solid corners"
            );
        }
    }
}
