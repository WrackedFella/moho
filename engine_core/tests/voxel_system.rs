// Clean, focused unit tests for voxel utilities and grid behavior.
use engine_core::voxel::*;
use glam::{IVec3, Vec3};

// FaceDirection helpers
#[test]
fn test_face_direction_offsets_and_lists() {
    assert_eq!(FaceDirection::PosX.offset(), IVec3::new(1, 0, 0));
    assert_eq!(FaceDirection::NegX.offset(), IVec3::new(-1, 0, 0));
    assert_eq!(FaceDirection::PosY.offset(), IVec3::new(0, 1, 0));
    assert_eq!(FaceDirection::NegY.offset(), IVec3::new(0, -1, 0));
    assert_eq!(FaceDirection::PosZ.offset(), IVec3::new(0, 0, 1));
    assert_eq!(FaceDirection::NegZ.offset(), IVec3::new(0, 0, -1));

    let all = FaceDirection::all();
    assert_eq!(all.len(), 6);
    for dir in &[
        FaceDirection::PosX,
        FaceDirection::NegX,
        FaceDirection::PosY,
        FaceDirection::NegY,
        FaceDirection::PosZ,
        FaceDirection::NegZ,
    ] {
        assert!(all.contains(dir));
    }

    assert_eq!(FaceDirection::PosX.vertex_range(), (0usize, 4usize));
    assert_eq!(FaceDirection::NegX.vertex_range(), (4, 4));
    assert_eq!(FaceDirection::PosY.vertex_range(), (8, 4));
    assert_eq!(FaceDirection::NegY.vertex_range(), (12, 4));
    assert_eq!(FaceDirection::PosZ.vertex_range(), (16, 4));
    assert_eq!(FaceDirection::NegZ.vertex_range(), (20, 4));

    assert_eq!(FaceDirection::PosX.index_range(), (0usize, 6usize));
    assert_eq!(FaceDirection::NegX.index_range(), (6, 6));
    assert_eq!(FaceDirection::PosY.index_range(), (12, 6));
    assert_eq!(FaceDirection::NegY.index_range(), (18, 6));
    assert_eq!(FaceDirection::PosZ.index_range(), (24, 6));
    assert_eq!(FaceDirection::NegZ.index_range(), (30, 6));
}

#[test]
fn test_voxel_block_basics() {
    let pos = IVec3::new(1, 2, 3);
    let block = VoxelBlock::new(pos, 1);
    assert_eq!(block.position, pos);
    assert_eq!(block.material_id, 1);
    assert!(block.resource_id.is_none());
    // By default the mesh for a new block is empty (we assign meshes later)
    assert!(block.mesh_data.vertices.is_empty());

    let b2 = VoxelBlock::new(IVec3::new(1, 2, 3), 0);
    let world_pos = b2.world_position();
    // Implementation returns the integer grid position as Vec3; assert accordingly
    assert_eq!(world_pos, Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn test_voxel_grid_basic_ops() {
    let mut grid = VoxelGrid::new(64);
    assert_eq!(grid.chunk_size(), 64);
    assert_eq!(grid.block_positions().count(), 0);

    let pos = IVec3::new(1, 2, 3);
    let block = VoxelBlock::new(pos, 1);
    grid.set_block(pos, block);
    assert_eq!(grid.block_positions().count(), 1);

    let retrieved = grid.get_block(&pos).expect("block present");
    assert_eq!(retrieved.position, pos);
    assert_eq!(retrieved.material_id, 1);

    let removed = grid.remove_block(&pos);
    assert!(removed.is_some());
    assert!(grid.get_block(&pos).is_none());

    // Chunk position calculation
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(0, 0, 0), 64),
        IVec3::new(0, 0, 0)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(63, 63, 63), 64),
        IVec3::new(0, 0, 0)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(64, 64, 64), 64),
        IVec3::new(1, 1, 1)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(-1, -1, -1), 64),
        IVec3::new(-1, -1, -1)
    );
}

#[test]
fn test_voxel_grid_heights_and_neighbors() {
    let mut grid = VoxelGrid::new(16);
    grid.set_block(IVec3::new(0, 0, 0), VoxelBlock::new(IVec3::new(0, 0, 0), 0));
    grid.set_block(IVec3::new(0, 1, 0), VoxelBlock::new(IVec3::new(0, 1, 0), 0));
    grid.set_block(IVec3::new(0, 3, 0), VoxelBlock::new(IVec3::new(0, 3, 0), 0));
    assert_eq!(grid.get_height(0, 0), Some(3));
    assert_eq!(grid.get_height(1, 0), None);

    // Neighbor heights for position (0,1,0): [North(+Z), South(-Z), East(+X), West(-X)]
    // Setup East and North neighbors at y=0
    grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0));
    grid.set_block(IVec3::new(0, 0, 1), VoxelBlock::new(IVec3::new(0, 0, 1), 0));
    let neighbors = grid.get_neighbor_heights(IVec3::new(0, 1, 0));
    assert_eq!(neighbors, [Some(0), None, Some(0), None]);
}

#[test]
fn test_face_culling_and_visible_faces() {
    let mut grid = VoxelGrid::new(16);
    let pos = IVec3::new(0, 0, 0);

    // No neighbors -> all faces visible
    for dir in FaceDirection::all().iter() {
        assert!(grid.should_render_face(pos, *dir));
    }

    // Add neighbor to the east (+X)
    grid.set_block(IVec3::new(1, 0, 0), VoxelBlock::new(IVec3::new(1, 0, 0), 0));
    assert!(!grid.should_render_face(pos, FaceDirection::PosX));
    // Other faces remain visible
    assert!(grid.should_render_face(pos, FaceDirection::NegX));
    assert!(grid.should_render_face(pos, FaceDirection::PosY));

    // get_visible_faces should reflect the culling
    let visible = grid.get_visible_faces(pos);
    assert!(!visible.contains(&FaceDirection::PosX));
    assert!(visible.contains(&FaceDirection::PosY));
    assert!(visible.contains(&FaceDirection::NegX));
}
