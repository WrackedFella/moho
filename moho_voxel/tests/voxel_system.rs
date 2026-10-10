// Clean, focused unit tests for voxel utilities and grid behavior.
use glam::IVec3;
use moho_voxel::VoxelGrid;

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
