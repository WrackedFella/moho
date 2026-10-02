// Clean, focused unit tests for voxel utilities and grid behavior.
use glam::{IVec3, Vec3};
use moho_core::voxel::{VoxelBlock, VoxelGrid};

#[test]
fn test_voxel_block_basics() {
    let pos = IVec3::new(1, 2, 3);
    let block = VoxelBlock::new(pos, 1);
    assert_eq!(block.position, pos);
    assert_eq!(block.material_id, 1);
    assert!(block.resource_id.is_none());

    let b2 = VoxelBlock::new(IVec3::new(1, 2, 3), 0);
    let world_pos = b2.world_position();
    // Implementation returns the integer grid position as Vec3; assert accordingly
    assert_eq!(world_pos, Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn test_voxel_grid_get_chunk_pos() {
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(0, 0, 0), 16),
        IVec3::new(0, 0, 0)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(15, 15, 15), 16),
        IVec3::new(0, 0, 0)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(16, 16, 16), 16),
        IVec3::new(1, 1, 1)
    );
    assert_eq!(
        VoxelGrid::get_chunk_pos(IVec3::new(-1, -1, -1), 16),
        IVec3::new(-1, -1, -1)
    );
}

#[test]
fn test_voxel_grid_heights_and_neighbors() {
    let mut grid = VoxelGrid::new(16);
    grid.mutator().place(IVec3::new(0, 0, 0), 0, None);
    grid.mutator().place(IVec3::new(0, 1, 0), 0, None);
    grid.mutator().place(IVec3::new(0, 3, 0), 0, None);
    assert_eq!(grid.get_height(0, 0), Some(3));
    assert_eq!(grid.get_height(1, 0), None);

    // Neighbor heights for position (0,1,0): [North(+Z), South(-Z), East(+X), West(-X)]
    // Setup East and North neighbors at y=0
    grid.mutator().place(IVec3::new(1, 0, 0), 0, None);
    grid.mutator().place(IVec3::new(0, 0, 1), 0, None);
    let neighbors = grid.get_neighbor_heights(IVec3::new(0, 1, 0));
    assert_eq!(neighbors, [Some(0), None, Some(0), None]);
}
