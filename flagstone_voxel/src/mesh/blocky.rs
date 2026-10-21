//! Blocky mesh generation with per-vertex ambient occlusion.
//!
//! Generates cube-based meshes for blocks classified as "blocky" (crafted materials
//! like planks, bricks, etc.). Each vertex has ambient occlusion computed based on
//! the blocks beside it (two edge neighbors and the diagonal one).

use super::VoxelMesh;
use crate::face::FaceDirection;
use crate::grid::{BlockPos, VoxelGrid};
use glam::IVec3;

/// Generates cube meshes with per-vertex ambient occlusion for blocky materials
pub struct BlockyMeshGenerator;

impl BlockyMeshGenerator {
    /// Generate a cube mesh with ambient occlusion for a single block
    ///
    /// # Arguments
    /// * `grid` - The voxel grid (needed to query neighbors for AO)
    /// * `position` - The block position in the grid
    ///
    /// # Returns
    /// A `VoxelMesh` with 24 vertices (4 per face), normals, and AO values
    ///
    /// # Panics
    /// If `position` is air.
    pub fn generate_mesh(grid: &VoxelGrid, position: BlockPos) -> VoxelMesh {
        let mut mesh = VoxelMesh::empty();

        let material = grid
            .face_material_at(position)
            .expect("the block at `position` is solid");

        // +X face (right): check neighbors in +X direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::PosX, material);

        // -X face (left): check neighbors in -X direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::NegX, material);

        // +Y face (top): check neighbors in +Y direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::PosY, material);

        // -Y face (bottom): check neighbors in -Y direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::NegY, material);

        // +Z face (front): check neighbors in +Z direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::PosZ, material);

        // -Z face (back): check neighbors in -Z direction
        Self::add_face(&mut mesh, grid, position, FaceDirection::NegZ, material);

        mesh
    }

    /// Add a single face to the mesh with AO computed for each vertex
    fn add_face(
        mesh: &mut VoxelMesh,
        grid: &VoxelGrid,
        position: BlockPos,
        direction: FaceDirection,
        face_material: u32,
    ) {
        let base_index = mesh.vertices.len() as u32;

        // Get face geometry (vertices and normal)
        let (verts, normal) = direction.vertices_and_normal();

        // Add vertices
        for vert in &verts {
            mesh.vertices.push(*vert);
        }

        // Add normals (all 4 vertices share the same normal)
        for _ in 0..4 {
            mesh.normals.push(normal);
        }

        mesh.ambient_occlusion.extend(
            verts
                .iter()
                .map(|&vert| FaceDirection::vertex_ao(grid, position, vert, normal)),
        );

        mesh.material.extend([face_material; 4]);

        // Mark as blocky geometry (0 = smooth, 1 = blocky)
        for _ in 0..4 {
            mesh.geometry_type.push(1);
        }

        // The block itself is opaque and always dark; the face shows the air it faces.
        let front = position + IVec3::from_array(normal.map(|c| c as i32));
        for _ in 0..4 {
            mesh.push_light(grid.block_light_rgb_at(front), grid.sky_exposed_at(front));
        }

        // Add indices (2 triangles per face)
        mesh.indices.push(base_index);
        mesh.indices.push(base_index + 1);
        mesh.indices.push(base_index + 2);
        mesh.indices.push(base_index);
        mesh.indices.push(base_index + 2);
        mesh.indices.push(base_index + 3);
    }
}

/// Blocky-mesh geometry and per-vertex AO for `FaceDirection`.
impl FaceDirection {
    /// Get the 4 vertices and normal for this face
    fn vertices_and_normal(&self) -> ([[f32; 3]; 4], [f32; 3]) {
        match self {
            FaceDirection::PosX => (
                [
                    [1.0, 0.0, 0.0], // bottom-back
                    [1.0, 1.0, 0.0], // top-back
                    [1.0, 1.0, 1.0], // top-front
                    [1.0, 0.0, 1.0], // bottom-front
                ],
                [1.0, 0.0, 0.0],
            ),
            FaceDirection::NegX => (
                [
                    [0.0, 0.0, 1.0], // bottom-front
                    [0.0, 1.0, 1.0], // top-front
                    [0.0, 1.0, 0.0], // top-back
                    [0.0, 0.0, 0.0], // bottom-back
                ],
                [-1.0, 0.0, 0.0],
            ),
            FaceDirection::PosY => (
                [
                    [0.0, 1.0, 0.0], // back-left
                    [0.0, 1.0, 1.0], // front-left
                    [1.0, 1.0, 1.0], // front-right
                    [1.0, 1.0, 0.0], // back-right
                ],
                [0.0, 1.0, 0.0],
            ),
            FaceDirection::NegY => (
                [
                    [0.0, 0.0, 0.0], // back-left
                    [1.0, 0.0, 0.0], // back-right
                    [1.0, 0.0, 1.0], // front-right
                    [0.0, 0.0, 1.0], // front-left
                ],
                [0.0, -1.0, 0.0],
            ),
            FaceDirection::PosZ => (
                [
                    [0.0, 0.0, 1.0], // bottom-left
                    [1.0, 0.0, 1.0], // bottom-right
                    [1.0, 1.0, 1.0], // top-right
                    [0.0, 1.0, 1.0], // top-left
                ],
                [0.0, 0.0, 1.0],
            ),
            FaceDirection::NegZ => (
                [
                    [1.0, 0.0, 0.0], // bottom-right
                    [0.0, 0.0, 0.0], // bottom-left
                    [0.0, 1.0, 0.0], // top-left
                    [1.0, 1.0, 0.0], // top-right
                ],
                [0.0, 0.0, -1.0],
            ),
        }
    }

    /// Ambient occlusion for one face vertex, from the blocks around that vertex.
    ///
    /// `vertex` is a unit-cube corner on the face and `normal` the face normal. On each
    /// in-plane axis the vertex lies on the `+` or `-` side of the block, which picks
    /// the two edge neighbours (`side1`, `side2`) and the diagonal one in front of the
    /// face. Two solid sides give maximum occlusion; otherwise each solid neighbour adds
    /// one step.
    ///
    /// Returns a value in `[0.0, 1.0]`: 1.0 is unoccluded, 0.4 fully occluded.
    fn vertex_ao(grid: &VoxelGrid, pos: BlockPos, vertex: [f32; 3], normal: [f32; 3]) -> f32 {
        let n = IVec3::from_array(normal.map(|c| c as i32));
        let mut steps = [IVec3::ZERO; 2];
        let mut in_plane = (0..3).filter(|&axis| n[axis] == 0);
        for step in &mut steps {
            let axis = in_plane
                .next()
                .expect("a face normal is axis-aligned, leaving two in-plane axes");
            step[axis] = if vertex[axis] == 1.0 { 1 } else { -1 };
        }
        let [e1, e2] = steps;

        let s1 = u8::from(grid.is_solid_at(pos + n + e1));
        let s2 = u8::from(grid.is_solid_at(pos + n + e2));
        let d = u8::from(grid.is_solid_at(pos + n + e1 + e2));

        let occlusion = if s1 == 1 && s2 == 1 {
            3.0
        } else {
            f32::from(s1 + s2 + d)
        };

        1.0 - (occlusion * 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolated_block_has_no_occlusion() {
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None); // Blocky material ID

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        // All vertices should have full brightness (1.0) since no neighbors
        for ao in mesh.ambient_occlusion {
            assert_eq!(ao, 1.0, "Isolated block should have no occlusion");
        }
    }

    #[test]
    fn test_corner_block_has_occlusion() {
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None);
        grid.place_block(pos + IVec3::new(1, 1, 0), 100, None);
        grid.place_block(pos + IVec3::new(0, 1, 1), 100, None);

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        let ao_on_top_face_at = |vertex: [f32; 3]| {
            let matches: Vec<f32> = mesh
                .vertices
                .iter()
                .zip(&mesh.normals)
                .zip(&mesh.ambient_occlusion)
                .filter(|((v, n), _)| **v == vertex && **n == [0.0, 1.0, 0.0])
                .map(|(_, ao)| *ao)
                .collect();
            assert_eq!(matches.len(), 1, "one +Y vertex at {vertex:?}");
            matches[0]
        };
        assert!((ao_on_top_face_at([1.0, 1.0, 1.0]) - 0.4).abs() < 1e-6);
        assert!((ao_on_top_face_at([0.0, 1.0, 0.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_mesh_structure() {
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None);

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        // 6 faces * 4 vertices = 24 vertices
        assert_eq!(mesh.vertices.len(), 24, "Should have 24 vertices");
        assert_eq!(mesh.normals.len(), 24, "Should have 24 normals");
        assert_eq!(mesh.ambient_occlusion.len(), 24, "Should have 24 AO values");

        // 6 faces * 2 triangles * 3 indices = 36 indices
        assert_eq!(mesh.indices.len(), 36, "Should have 36 indices");
    }

    #[test]
    fn isolated_block_faces_lie_on_the_plane_their_normal_names() {
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None);

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        assert_eq!(mesh.vertices.len(), 24);
        for (verts, normals) in mesh.vertices.chunks(4).zip(mesh.normals.chunks(4)) {
            let n = normals[0];
            assert!(normals.iter().all(|m| *m == n), "face shares one normal");
            let axis = (0..3)
                .find(|&a| n[a] != 0.0)
                .expect("normal has a nonzero axis");
            let plane = if n[axis] > 0.0 { 1.0 } else { 0.0 };
            for v in verts {
                assert_eq!(v[axis], plane, "vertex {v:?} off plane for normal {n:?}");
            }
            for other in (0..3).filter(|&a| a != axis) {
                assert!(
                    verts.iter().any(|v| v[other] == 0.0) && verts.iter().any(|v| v[other] == 1.0),
                    "face with normal {n:?} does not span 0..1 on axis {other}"
                );
            }
        }
    }

    #[test]
    fn all_indices_in_range() {
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None);

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        let referenced: std::collections::BTreeSet<u32> = mesh.indices.iter().copied().collect();
        assert_eq!(referenced, (0..24).collect());
    }

    /// Expected AO of a block's vertex, from geometry alone: the occluders are the three
    /// blocks diagonally outside the face beside that vertex.
    fn expected_ao(grid: &VoxelGrid, pos: BlockPos, vertex: [f32; 3], normal: [f32; 3]) -> f32 {
        let n = IVec3::new(normal[0] as i32, normal[1] as i32, normal[2] as i32);
        let in_plane: Vec<usize> = (0..3).filter(|&a| normal[a] == 0.0).collect();
        let step = |axis: usize| {
            let mut e = IVec3::ZERO;
            e[axis] = if vertex[axis] == 0.0 { -1 } else { 1 };
            e
        };
        let (e1, e2) = (step(in_plane[0]), step(in_plane[1]));
        let solid = |offset: IVec3| grid.is_solid_at(pos + offset);
        let (side1, side2, corner) = (solid(n + e1), solid(n + e2), solid(n + e1 + e2));

        if side1 && side2 {
            0.4
        } else {
            1.0 - 0.2 * [side1, side2, corner].iter().filter(|&&s| s).count() as f32
        }
    }

    proptest::proptest! {
        #[test]
        fn vertex_ao_matches_occluders_beside_each_vertex(
            occupied in proptest::collection::vec(proptest::bool::ANY, 27),
        ) {
            let mut grid = VoxelGrid::new(16);
            let pos = IVec3::new(16, 16, 16);
            grid.place_block(pos, 100, None);
            for (i, solid) in occupied.iter().enumerate() {
                let offset = IVec3::new(i as i32 % 3 - 1, (i as i32 / 3) % 3 - 1, i as i32 / 9 - 1);
                if *solid && offset != IVec3::ZERO {
                    grid.place_block(pos + offset, 100, None);
                }
            }

            let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

            proptest::prop_assert_eq!(mesh.vertices.len(), 24);
            for ((vertex, normal), ao) in mesh
                .vertices
                .iter()
                .zip(&mesh.normals)
                .zip(&mesh.ambient_occlusion)
            {
                let expected = expected_ao(&grid, pos, *vertex, *normal);
                proptest::prop_assert!(
                    (ao - expected).abs() < 1e-6,
                    "vertex {:?} on face {:?}: AO {}, expected {}",
                    vertex, normal, ao, expected
                );
            }
        }
    }

    #[test]
    fn face_vertices_carry_scaled_block_and_sky_light() {
        // (block light, sky exposed, expected combined light)
        let rows = [
            ([3u8, 6, 9], false, 0.6),
            ([3, 6, 9], true, 0.7),
            ([15, 0, 0], true, 1.0),
            ([0, 0, 0], true, 0.1),
            ([0, 0, 0], false, 0.0),
        ];

        for (rgb, sky, expected_light) in rows {
            let mut grid = VoxelGrid::new(16);
            let pos = IVec3::new(16, 16, 16);
            grid.place_block(pos, 100, None);
            for step in [
                IVec3::X,
                IVec3::NEG_X,
                IVec3::Y,
                IVec3::NEG_Y,
                IVec3::Z,
                IVec3::NEG_Z,
            ] {
                grid.set_block_light_rgb(pos + step, rgb);
                grid.set_sky_exposed(pos + step, sky);
            }

            let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

            let expected_rgb = rgb.map(|c| f32::from(c) / 15.0);
            let expected_sky = if sky { 1.0 } else { 0.0 };
            assert_eq!(mesh.light_level.len(), 24);
            assert_eq!(mesh.block_light_rgb.len(), 24);
            assert_eq!(mesh.sky_exposed.len(), 24);
            for i in 0..24 {
                assert!(
                    (mesh.light_level[i] - expected_light).abs() < 1e-6,
                    "{rgb:?} sky {sky}: light {} not {expected_light}",
                    mesh.light_level[i]
                );
                for (got, want) in mesh.block_light_rgb[i].iter().zip(&expected_rgb) {
                    assert!(
                        (got - want).abs() < 1e-6,
                        "{rgb:?} sky {sky}: channels {:?}, expected {expected_rgb:?}",
                        mesh.block_light_rgb[i]
                    );
                }
                assert_eq!(mesh.sky_exposed[i], expected_sky);
            }
        }
    }

    #[test]
    fn each_face_carries_the_light_in_front_of_it() {
        // (face normal, light in front of the face, sky exposed in front of it)
        let fronts = [
            ([1.0f32, 0.0, 0.0], [10u8, 0, 0], false),
            ([-1.0, 0.0, 0.0], [1, 2, 3], true),
            ([0.0, 1.0, 0.0], [4, 5, 6], false),
            ([0.0, -1.0, 0.0], [7, 8, 9], true),
            ([0.0, 0.0, 1.0], [11, 12, 13], false),
            ([0.0, 0.0, -1.0], [14, 15, 0], true),
        ];
        let mut grid = VoxelGrid::new(16);
        let pos = IVec3::new(16, 16, 16);
        grid.place_block(pos, 100, None);
        grid.set_block_light_rgb(pos, [15, 15, 15]);
        grid.set_sky_exposed(pos, true);
        for (normal, rgb, sky) in fronts {
            let front = pos + IVec3::new(normal[0] as i32, normal[1] as i32, normal[2] as i32);
            grid.set_block_light_rgb(front, rgb);
            grid.set_sky_exposed(front, sky);
        }

        let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

        for (normal, rgb, sky) in fronts {
            let face: Vec<usize> = (0..mesh.normals.len())
                .filter(|&i| mesh.normals[i] == normal)
                .collect();
            assert_eq!(face.len(), 4, "four vertices on the {normal:?} face");
            for i in face {
                let expected_rgb = rgb.map(|c| f32::from(c) / 15.0);
                for (got, want) in mesh.block_light_rgb[i].iter().zip(&expected_rgb) {
                    assert!(
                        (got - want).abs() < 1e-6,
                        "{normal:?} face: block light {:?}, expected {expected_rgb:?}",
                        mesh.block_light_rgb[i]
                    );
                }
                assert_eq!(
                    mesh.sky_exposed[i],
                    if sky { 1.0 } else { 0.0 },
                    "{normal:?} face sky exposure"
                );
            }
        }
    }

    #[test]
    fn every_face_carries_the_block_face_material() {
        let rows = [
            (100, None, 100, "unmapped blocky block keeps its id"),
            (105, None, 105, "another blocky id"),
            (100, Some(1), 3, "iron ore draws in the ore material"),
            (100, Some(9), 100, "an unmapped resource keeps the block id"),
        ];

        for (material, resource, expected, why) in rows {
            let mut grid = VoxelGrid::new(16);
            let pos = IVec3::new(16, 16, 16);
            grid.place_block(pos, material, resource);

            let mesh = BlockyMeshGenerator::generate_mesh(&grid, pos);

            assert_eq!(mesh.material, vec![expected; 24], "{why}");
        }
    }
}
