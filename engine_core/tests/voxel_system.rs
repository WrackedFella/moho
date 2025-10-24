use engine_core::voxel::*;use engine_core::voxel::*;use engine_core::voxel::*;

use engine_core::materials::MaterialType;

use glam::IVec3;use engine_core::materials::MaterialType;use engine_core::materials::MaterialType;



// FaceDirection testsuse glam::IVec3;use glam::IVec3;

#[test]

fn test_face_direction_offsets() {

    assert_eq!(FaceDirection::PosX.offset(), IVec3::new(1, 0, 0));

    assert_eq!(FaceDirection::NegX.offset(), IVec3::new(-1, 0, 0));// FaceDirection tests#[cfg(test)]

    assert_eq!(FaceDirection::PosY.offset(), IVec3::new(0, 1, 0));

    assert_eq!(FaceDirection::NegY.offset(), IVec3::new(0, -1, 0));#[test]mod tests {

    assert_eq!(FaceDirection::PosZ.offset(), IVec3::new(0, 0, 1));

    assert_eq!(FaceDirection::NegZ.offset(), IVec3::new(0, 0, -1));fn test_face_direction_offsets() {    use super::*;

}
    assert_eq!(FaceDirection::PosX.offset(), IVec3::new(1, 0, 0));

    assert_eq!(FaceDirection::NegX.offset(), IVec3::new(-1, 0, 0));    // FaceDirection tests

    assert_eq!(FaceDirection::PosY.offset(), IVec3::new(0, 1, 0));    #[test]

    assert_eq!(FaceDirection::NegY.offset(), IVec3::new(0, -1, 0));    fn test_face_direction_offsets() {

    assert_eq!(FaceDirection::PosZ.offset(), IVec3::new(0, 0, 1));        assert_eq!(FaceDirection::PosX.offset(), IVec3::new(1, 0, 0));

    assert_eq!(FaceDirection::NegZ.offset(), IVec3::new(0, 0, -1));        assert_eq!(FaceDirection::NegX.offset(), IVec3::new(-1, 0, 0));

}        assert_eq!(FaceDirection::PosY.offset(), IVec3::new(0, 1, 0));

        assert_eq!(FaceDirection::NegY.offset(), IVec3::new(0, -1, 0));

#[test]        assert_eq!(FaceDirection::PosZ.offset(), IVec3::new(0, 0, 1));

fn test_face_direction_all() {        assert_eq!(FaceDirection::NegZ.offset(), IVec3::new(0, 0, -1));

    let all = FaceDirection::all();    }

    assert_eq!(all.len(), 6);

    assert!(all.contains(&FaceDirection::PosX));    #[test]

    assert!(all.contains(&FaceDirection::NegX));    fn test_face_direction_all() {

    assert!(all.contains(&FaceDirection::PosY));        let all = FaceDirection::all();

    assert!(all.contains(&FaceDirection::NegY));        assert_eq!(all.len(), 6);

    assert!(all.contains(&FaceDirection::PosZ));        assert!(all.contains(&FaceDirection::PosX));

    assert!(all.contains(&FaceDirection::NegZ));        assert!(all.contains(&FaceDirection::NegX));

}        assert!(all.contains(&FaceDirection::PosY));

        assert!(all.contains(&FaceDirection::NegY));

#[test]        assert!(all.contains(&FaceDirection::PosZ));

fn test_face_direction_vertex_ranges() {        assert!(all.contains(&FaceDirection::NegZ));

    assert_eq!(FaceDirection::PosX.vertex_range(), (0, 4));    }

    assert_eq!(FaceDirection::NegX.vertex_range(), (4, 4));

    assert_eq!(FaceDirection::PosY.vertex_range(), (8, 4));    #[test]

    assert_eq!(FaceDirection::NegY.vertex_range(), (12, 4));    fn test_face_direction_vertex_ranges() {

    assert_eq!(FaceDirection::PosZ.vertex_range(), (16, 4));        assert_eq!(FaceDirection::PosX.vertex_range(), (0, 4));

    assert_eq!(FaceDirection::NegZ.vertex_range(), (20, 4));        assert_eq!(FaceDirection::NegX.vertex_range(), (4, 4));

}        assert_eq!(FaceDirection::PosY.vertex_range(), (8, 4));

        assert_eq!(FaceDirection::NegY.vertex_range(), (12, 4));

#[test]        assert_eq!(FaceDirection::PosZ.vertex_range(), (16, 4));

fn test_face_direction_index_ranges() {        assert_eq!(FaceDirection::NegZ.vertex_range(), (20, 4));

    assert_eq!(FaceDirection::PosX.index_range(), (0, 6));    }

    assert_eq!(FaceDirection::NegX.index_range(), (6, 6));

    assert_eq!(FaceDirection::PosY.index_range(), (12, 6));    #[test]

    assert_eq!(FaceDirection::NegY.index_range(), (18, 6));    fn test_face_direction_index_ranges() {

    assert_eq!(FaceDirection::PosZ.index_range(), (24, 6));        assert_eq!(FaceDirection::PosX.index_range(), (0, 6));

    assert_eq!(FaceDirection::NegZ.index_range(), (30, 6));        assert_eq!(FaceDirection::NegX.index_range(), (6, 6));

}        assert_eq!(FaceDirection::PosY.index_range(), (12, 6));

        assert_eq!(FaceDirection::NegY.index_range(), (18, 6));

// VoxelBlock tests        assert_eq!(FaceDirection::PosZ.index_range(), (24, 6));

#[test]        assert_eq!(FaceDirection::NegZ.index_range(), (30, 6));

fn test_voxel_block_creation() {    }

    let pos = IVec3::new(1, 2, 3);

    let block = VoxelBlock::new(pos, 1);    // VoxelBlock tests

    #[test]

    assert_eq!(block.position, pos);    fn test_voxel_block_creation() {

    assert_eq!(block.material_id, 1);        let pos = IVec3::new(1, 2, 3);

    assert!(block.resource_id.is_none());        let block = VoxelBlock::new(pos, 1);

    assert_eq!(block.mesh_data.vertices.len(), 0); // Empty mesh initially

}        assert_eq!(block.position, pos);

        assert_eq!(block.material_id, 1);

#[test]        assert!(block.resource_id.is_none());

fn test_voxel_block_world_position() {        assert_eq!(block.mesh_data.vertices.len(), 0); // Empty mesh initially

    let block = VoxelBlock::new(IVec3::new(1, 2, 3), 0);    }

    let world_pos = block.world_position();

    #[test]

    // Block center should be at (1.5, 2.5, 3.5) for integer coordinates    fn test_voxel_block_world_position() {

    assert_eq!(world_pos, glam::Vec3::new(1.5, 2.5, 3.5));        let block = VoxelBlock::new(IVec3::new(1, 2, 3), 0);

}        let world_pos = block.world_position();



// VoxelGrid tests        // Block center should be at (1.5, 2.5, 3.5) for integer coordinates

#[test]        assert_eq!(world_pos, glam::Vec3::new(1.5, 2.5, 3.5));

fn test_voxel_grid_creation() {    }

    let grid = VoxelGrid::new(16);

    assert_eq!(grid.chunk_size(), 16);    // VoxelGrid tests

    assert_eq!(grid.block_positions().count(), 0);    #[test]

}    fn test_voxel_grid_creation() {

        let grid = VoxelGrid::new(16);

#[test]        assert_eq!(grid.chunk_size(), 16);

fn test_voxel_grid_set_and_get_block() {        assert_eq!(grid.block_positions().count(), 0);

    let mut grid = VoxelGrid::new(16);    }

    let pos = IVec3::new(1, 2, 3);

    let block = VoxelBlock::new(pos, 1);    #[test]

    fn test_voxel_grid_set_and_get_block() {

    grid.set_block(pos, block);        let mut grid = VoxelGrid::new(16);

    assert_eq!(grid.block_positions().count(), 1);        let pos = IVec3::new(1, 2, 3);

        let block = VoxelBlock::new(pos, 1);

    let retrieved = grid.get_block(&pos).unwrap();

    assert_eq!(retrieved.position, pos);        grid.set_block(pos, block);

    assert_eq!(retrieved.material_id, 1);        assert_eq!(grid.block_positions().count(), 1);

}

        let retrieved = grid.get_block(&pos).unwrap();

#[test]        assert_eq!(retrieved.position, pos);

fn test_voxel_grid_remove_block() {        assert_eq!(retrieved.material_id, 1);

    let mut grid = VoxelGrid::new(16);    }

    let pos = IVec3::new(1, 2, 3);

    let block = VoxelBlock::new(pos, 1);    #[test]

    fn test_voxel_grid_remove_block() {

    grid.set_block(pos, block);        let mut grid = VoxelGrid::new(16);

    assert!(grid.get_block(&pos).is_some());        let pos = IVec3::new(1, 2, 3);

        let block = VoxelBlock::new(pos, 1);

    let removed = grid.remove_block(&pos);

    assert!(removed.is_some());        grid.set_block(pos, block);

    assert!(grid.get_block(&pos).is_none());        assert!(grid.get_block(&pos).is_some());

}

        let removed = grid.remove_block(&pos);

#[test]        assert!(removed.is_some());

fn test_voxel_grid_chunk_position_calculation() {        assert!(grid.get_block(&pos).is_none());

    // Test chunk position calculation    }

    assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(0, 0, 0), 16), IVec3::new(0, 0, 0));

    assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(15, 15, 15), 16), IVec3::new(0, 0, 0));    #[test]

    assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(16, 16, 16), 16), IVec3::new(1, 1, 1));    fn test_voxel_grid_chunk_position_calculation() {

    assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(-1, -1, -1), 16), IVec3::new(-1, -1, -1));        // Test chunk position calculation

}        assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(0, 0, 0), 16), IVec3::new(0, 0, 0));

        assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(15, 15, 15), 16), IVec3::new(0, 0, 0));

#[test]        assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(16, 16, 16), 16), IVec3::new(1, 1, 1));

fn test_voxel_grid_get_height() {        assert_eq!(VoxelGrid::get_chunk_pos(IVec3::new(-1, -1, -1), 16), IVec3::new(-1, -1, -1));

    let mut grid = VoxelGrid::new(16);    }



    // Add blocks at different heights for same x,z    #[test]

    grid.set_block(IVec3::new(0, 0, 0), VoxelBlock::new(IVec3::new(0, 0, 0), 0));    fn test_voxel_grid_get_height() {

    grid.set_block(IVec3::new(0, 1, 0), VoxelBlock::new(IVec3::new(0, 1, 0), 0));        let mut grid = VoxelGrid::new(16);

    grid.set_block(IVec3::new(0, 3, 0), VoxelBlock::new(IVec3::new(0, 3, 0), 0));

        // Add blocks at different heights for same x,z

    assert_eq!(grid.get_height(0, 0), Some(3));        grid.set_block(IVec3::new(0, 0, 0), VoxelBlock::new(IVec3::new(0, 0, 0), 0));

    assert_eq!(grid.get_height(1, 0), None); // No blocks at this x,z        grid.set_block(IVec3::new(0, 1, 0), VoxelBlock::new(IVec3::new(0, 1, 0), 0));

}        grid.set_block(IVec3::new(0, 3, 0), VoxelBlock::new(IVec3::new(0, 3, 0), 0));



#[test]        assert_eq!(grid.get_height(0, 0), Some(3));

fn test_voxel_grid_neighbor_heights() {        assert_eq!(grid.get_height(1, 0), None); // No blocks at this x,z

    let mut grid = VoxelGrid::new(16);    }



    // Create a simple height pattern    #[test]

    grid.set_block(IVec3::new(0, 0, 0), VoxelBlock::new(IVec3::new(0, 0, 0), 0)); // Center    fn test_voxel_grid_neighbor_heights() {

    grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0)); // East        let mut grid = VoxelGrid::new(16);

    grid.set_block(IVec3::new(0, 0, 1), VoxelBlock::new(IVec3::new(0, 0, 1), 0)); // North

    grid.set_block(IVec3::new(0, 1, 0), VoxelBlock::new(IVec3::new(0, 1, 0), 0)); // Above center        // Create a simple height pattern

        grid.set_block(IVec3::new(0, 0, 0), VoxelBlock::new(IVec3::new(0, 0, 0), 0)); // Center

    let neighbors = grid.get_neighbor_heights(IVec3::new(0, 1, 0));        grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0)); // East

    assert_eq!(neighbors, [Some(0), None, Some(0), None]); // [North, South, East, West]        grid.set_block(IVec3::new(0, 0, 1), VoxelBlock::new(IVec3::new(0, 0, 1), 0)); // North

}        grid.set_block(IVec3::new(0, 1, 0), VoxelBlock::new(IVec3::new(0, 1, 0), 0)); // Above center



// Face culling tests        let neighbors = grid.get_neighbor_heights(IVec3::new(0, 1, 0));

#[test]        assert_eq!(neighbors, [Some(0), None, Some(0), None]); // [North, South, East, West]

fn test_face_culling_no_neighbor() {    }

    let grid = VoxelGrid::new(16);

    let pos = IVec3::new(0, 0, 0);    // Face culling tests

    #[test]

    // No neighbors, all faces should be visible    fn test_face_culling_no_neighbor() {

    assert!(grid.should_render_face(pos, FaceDirection::PosX));        let grid = VoxelGrid::new(16);

    assert!(grid.should_render_face(pos, FaceDirection::NegX));        let pos = IVec3::new(0, 0, 0);

    assert!(grid.should_render_face(pos, FaceDirection::PosY));

    assert!(grid.should_render_face(pos, FaceDirection::NegY));        // No neighbors, all faces should be visible

    assert!(grid.should_render_face(pos, FaceDirection::PosZ));        assert!(grid.should_render_face(pos, FaceDirection::PosX));

    assert!(grid.should_render_face(pos, FaceDirection::NegZ));        assert!(grid.should_render_face(pos, FaceDirection::NegX));

}        assert!(grid.should_render_face(pos, FaceDirection::PosY));

        assert!(grid.should_render_face(pos, FaceDirection::NegY));

#[test]        assert!(grid.should_render_face(pos, FaceDirection::PosZ));

fn test_face_culling_with_neighbor() {        assert!(grid.should_render_face(pos, FaceDirection::NegZ));

    let mut grid = VoxelGrid::new(16);    }

    let pos = IVec3::new(0, 0, 0);

    #[test]

    // Add neighbor to the east (+X)    fn test_face_culling_with_neighbor() {

    grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0));        let mut grid = VoxelGrid::new(16);

        let pos = IVec3::new(0, 0, 0);

    // East face should not be visible, others should be

    assert!(!grid.should_render_face(pos, FaceDirection::PosX));        // Add neighbor to the east (+X)

    assert!(grid.should_render_face(pos, FaceDirection::NegX));        grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0));

    assert!(grid.should_render_face(pos, FaceDirection::PosY));

    assert!(grid.should_render_face(pos, FaceDirection::NegY));        // East face should not be visible, others should be

    assert!(grid.should_render_face(pos, FaceDirection::PosZ));        assert!(!grid.should_render_face(pos, FaceDirection::PosX));

    assert!(grid.should_render_face(pos, FaceDirection::NegZ));        assert!(grid.should_render_face(pos, FaceDirection::NegX));

}        assert!(grid.should_render_face(pos, FaceDirection::PosY));

        assert!(grid.should_render_face(pos, FaceDirection::NegY));

#[test]        assert!(grid.should_render_face(pos, FaceDirection::PosZ));

fn test_get_visible_faces() {        assert!(grid.should_render_face(pos, FaceDirection::NegZ));

    let mut grid = VoxelGrid::new(16);    }

    let pos = IVec3::new(0, 0, 0);

    #[test]

    // No neighbors - all faces visible    fn test_get_visible_faces() {

    let visible = grid.get_visible_faces(pos);        let mut grid = VoxelGrid::new(16);

    assert_eq!(visible.len(), 6);        let pos = IVec3::new(0, 0, 0);



    // Add neighbors on east and north        // No neighbors - all faces visible

    grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0)); // East        let visible = grid.get_visible_faces(pos);

    grid.set_block(IVec3::new(0, 0, 1), VoxelBlock::new(IVec3::new(0, 0, 1), 0)); // North        assert_eq!(visible.len(), 6);



    let visible = grid.get_visible_faces(pos);        // Add neighbors on east and north

    assert_eq!(visible.len(), 4);        grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0)); // East

    assert!(!visible.contains(&FaceDirection::PosX));        grid.set_block(IVec3::new(0, 0, 1), VoxelBlock::new(IVec3::new(0, 0, 1), 0)); // North

    assert!(!visible.contains(&FaceDirection::PosZ));

    assert!(visible.contains(&FaceDirection::NegX));        let visible = grid.get_visible_faces(pos);

    assert!(visible.contains(&FaceDirection::NegY));        assert_eq!(visible.len(), 4);

    assert!(visible.contains(&FaceDirection::NegZ));        assert!(!visible.contains(&FaceDirection::PosX));

    assert!(visible.contains(&FaceDirection::PosY));        assert!(!visible.contains(&FaceDirection::PosZ));

}        assert!(visible.contains(&FaceDirection::NegX));

        assert!(visible.contains(&FaceDirection::NegY));

// Mesh generation tests        assert!(visible.contains(&FaceDirection::NegZ));

#[test]        assert!(visible.contains(&FaceDirection::PosY));

fn test_cube_mesh_generation() {    }

    let mesh = MeshGenerator::cube_mesh();

    // Mesh generation tests

    // Cube should have 24 vertices (4 per face) and 36 indices (6 per face)    #[test]

    assert_eq!(mesh.vertices.len(), 24);    fn test_cube_mesh_generation() {

    assert_eq!(mesh.normals.len(), 24);        let mesh = MeshGenerator::cube_mesh();

    assert_eq!(mesh.indices.len(), 36);

        // Cube should have 24 vertices (4 per face) and 36 indices (6 per face)

    // Check that vertices are properly positioned for a unit cube        assert_eq!(mesh.vertices.len(), 24);

    // Front face (+Z) vertices should have z = 0.5        assert_eq!(mesh.normals.len(), 24);

    for i in 16..20 { // +Z face vertices        assert_eq!(mesh.indices.len(), 36);

        assert_eq!(mesh.vertices[i][2], 0.5);

    }        // Check that vertices are properly positioned for a unit cube

        // Front face (+Z) vertices should have z = 0.5

    // Back face (-Z) vertices should have z = -0.5        for i in 16..20 { // +Z face vertices

    for i in 20..24 { // -Z face vertices            assert_eq!(mesh.vertices[i][2], 0.5);

        assert_eq!(mesh.vertices[i][2], -0.5);        }

    }

}        // Back face (-Z) vertices should have z = -0.5

        for i in 20..24 { // -Z face vertices

#[test]            assert_eq!(mesh.vertices[i][2], -0.5);

fn test_smoothed_mesh_generation() {        }

    let pos = IVec3::new(0, 1, 0);    }

    let neighbor_heights = [Some(0), None, Some(0), None]; // Lower neighbors to north and east

    #[test]

    let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);    fn test_smoothed_mesh_generation() {

        let pos = IVec3::new(0, 1, 0);

    // Should still have same number of vertices and indices as cube        let neighbor_heights = [Some(0), None, Some(0), None]; // Lower neighbors to north and east

    assert_eq!(mesh.vertices.len(), 24);

    assert_eq!(mesh.normals.len(), 24);        let mesh = MeshGenerator::smoothed_mesh(pos, neighbor_heights);

    assert_eq!(mesh.indices.len(), 36);

        // Should still have same number of vertices and indices as cube

    // Check that top face vertices have been deformed        assert_eq!(mesh.vertices.len(), 24);

    // Top face vertices are at indices 8-11        assert_eq!(mesh.normals.len(), 24);

    // North edge (z = 0.5, y = 0.5) should be deformed down        assert_eq!(mesh.indices.len(), 36);

    let north_top_vertices: Vec<usize> = (8..12).filter(|&i| mesh.vertices[i][2] > 0.49 && mesh.vertices[i][1] > 0.49).collect();

    assert!(!north_top_vertices.is_empty());        // Check that top face vertices have been deformed

        // Top face vertices are at indices 8-11

    // Check that the north top vertices are lower than 0.5        // North edge (z = 0.5, y = 0.5) should be deformed down

    for &idx in &north_top_vertices {        let north_top_vertices: Vec<usize> = (8..12).filter(|&i| mesh.vertices[i][2] > 0.49 && mesh.vertices[i][1] > 0.49).collect();

        assert!(mesh.vertices[idx][1] < 0.5, "North top vertex should be deformed down");        assert!(!north_top_vertices.is_empty());

    }

}        // Check that the north top vertices are lower than 0.5

        for &idx in &north_top_vertices {

// VoxelChunk tests            assert!(mesh.vertices[idx][1] < 0.5, "North top vertex should be deformed down");

#[test]        }

fn test_voxel_chunk_creation() {    }

    let mut grid = VoxelGrid::new(16);

    // VoxelChunk tests

    // Add a single block    #[test]

    let pos = IVec3::new(0, 0, 0);    fn test_voxel_chunk_creation() {

    let mut block = VoxelBlock::new(pos, 0);        let mut grid = VoxelGrid::new(16);

    block.mesh_data = MeshGenerator::cube_mesh();

    grid.set_block(pos, block);        // Add a single block

        let pos = IVec3::new(0, 0, 0);

    let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));        let mut block = VoxelBlock::new(pos, 0);

        block.mesh_data = MeshGenerator::cube_mesh();

    assert_eq!(chunk.chunk_pos, IVec3::new(0, 0, 0));        grid.set_block(pos, block);

    assert!(!chunk.is_empty());

    assert_eq!(chunk.vertices.len(), 24); // One cube        let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

    assert_eq!(chunk.indices.len(), 36);

}        assert_eq!(chunk.chunk_pos, IVec3::new(0, 0, 0));

        assert!(!chunk.is_empty());

#[test]        assert_eq!(chunk.vertices.len(), 24); // One cube

fn test_voxel_chunk_empty() {        assert_eq!(chunk.indices.len(), 36);

    let grid = VoxelGrid::new(16);    }

    let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

    #[test]

    assert!(chunk.is_empty());    fn test_voxel_chunk_empty() {

    assert_eq!(chunk.vertices.len(), 0);        let grid = VoxelGrid::new(16);

    assert_eq!(chunk.indices.len(), 0);        let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

}

        assert!(chunk.is_empty());

#[test]        assert_eq!(chunk.vertices.len(), 0);

fn test_voxel_chunk_face_culling() {        assert_eq!(chunk.indices.len(), 0);

    let mut grid = VoxelGrid::new(16);    }



    // Create two adjacent blocks    #[test]

    let mut block1 = VoxelBlock::new(IVec3::new(0, 0, 0), 0);    fn test_voxel_chunk_face_culling() {

    block1.mesh_data = MeshGenerator::cube_mesh();        let mut grid = VoxelGrid::new(16);

    let mut block2 = VoxelBlock::new(IVec3::new(1, 0, 0), 0); // Adjacent to east

    block2.mesh_data = MeshGenerator::cube_mesh();        // Create two adjacent blocks

        let block1 = VoxelBlock::new(IVec3::new(0, 0, 0), 0);

    grid.set_block(IVec3::new(0, 0, 0), block1);        let block2 = VoxelBlock::new(IVec3::new(1, 0, 0), 0); // Adjacent to east

    grid.set_block(IVec3::new(1, 0, 0), block2);

        grid.set_block(IVec3::new(0, 0, 0), block1);

    let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));        grid.set_block(IVec3::new(1, 0, 0), block2);



    // Should have geometry but fewer triangles due to face culling        let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

    assert!(!chunk.is_empty());

    assert!(chunk.vertices.len() > 0);        // Should have geometry but fewer triangles due to face culling

    assert!(chunk.indices.len() > 0);        assert!(!chunk.is_empty());

        assert!(chunk.vertices.len() > 0);

    // With face culling, should have fewer than 72 indices (2 cubes * 36 indices each)        assert!(chunk.indices.len() > 0);

    assert!(chunk.indices.len() < 72);

}        // With face culling, should have fewer than 72 indices (2 cubes * 36 indices each)

        assert!(chunk.indices.len() < 72);

#[test]    }

fn test_voxel_chunk_memory_size() {

    let mut grid = VoxelGrid::new(16);    #[test]

    let mut block = VoxelBlock::new(IVec3::new(0, 0, 0), 0);    fn test_voxel_chunk_memory_size() {

    block.mesh_data = MeshGenerator::cube_mesh();        let mut grid = VoxelGrid::new(16);

    grid.set_block(IVec3::new(0, 0, 0), block);        let block = VoxelBlock::new(IVec3::new(0, 0, 0), 0);

        grid.set_block(IVec3::new(0, 0, 0), block);

    let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

        let chunk = VoxelChunk::from_grid(&grid, IVec3::new(0, 0, 0));

    // Memory size should be vertices * sizeof([f32; 3])

    let expected_size = chunk.vertices.len() * std::mem::size_of::<[f32; 3]>();        // Memory size should be vertices * sizeof([f32; 3])

    assert_eq!(chunk.memory_size(), expected_size);        let expected_size = chunk.vertices.len() * std::mem::size_of::<[f32; 3]>();

}        assert_eq!(chunk.memory_size(), expected_size);

    }

// Material and Resource Registry tests

#[test]    // Material and Resource Registry tests

fn test_material_registry() {    #[test]

    let mut registry = MaterialRegistry::new();    fn test_material_registry() {

        let mut registry = MaterialRegistry::new();

    // Should have default materials

    assert!(registry.get(0).is_some()); // Grass        // Should have default materials

    assert!(registry.get(1).is_some()); // Dirt        assert!(registry.get(0).is_some()); // Grass

    assert!(registry.get(2).is_some()); // Stone        assert!(registry.get(1).is_some()); // Dirt

        assert!(registry.get(2).is_some()); // Stone

    // Test registration

    let id = registry.register(MaterialType::Lambertian {        // Test registration

        albedo: glam::Vec3::new(1.0, 0.0, 0.0),        let id = registry.register(MaterialType::Lambertian {

    });            albedo: glam::Vec3::new(1.0, 0.0, 0.0),

    assert_eq!(id, 3);        });

    assert!(registry.get(3).is_some());        assert_eq!(id, 3);

}        assert!(registry.get(3).is_some());

    }

#[test]

fn test_resource_registry() {    #[test]

    let mut registry = ResourceRegistry::new();    fn test_resource_registry() {

        let mut registry = ResourceRegistry::new();

    // Should have default resources

    assert!(registry.get(0).is_some()); // Stone        // Should have default resources

    assert!(registry.get(1).is_some()); // Iron ore        assert!(registry.get(0).is_some()); // Stone

        assert!(registry.get(1).is_some()); // Iron ore

    // Test registration

    let id = registry.register(ResourceData {        // Test registration

        resource_type: "gold".to_string(),        let id = registry.register(ResourceData {

        quantity: 5,            resource_type: "gold".to_string(),

    });            quantity: 5,

    assert_eq!(id, 2);        });

    assert!(registry.get(2).is_some());        assert_eq!(id, 2);

    assert_eq!(registry.get(2).unwrap().resource_type, "gold");        assert!(registry.get(2).is_some());

    assert_eq!(registry.get(2).unwrap().quantity, 5);        assert_eq!(registry.get(2).unwrap().resource_type, "gold");

}        assert_eq!(registry.get(2).unwrap().quantity, 5);
    }