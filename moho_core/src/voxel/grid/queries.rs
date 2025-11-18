//! Query methods for VoxelGrid - height and neighbor lookups.

use super::super::{BlockPos, VoxelBlock};
use std::collections::HashMap;

/// Get height at a position (highest Y with a block)
///
/// Scans all blocks at the given (x, z) coordinates and returns the highest Y value.
pub fn get_height(blocks: &HashMap<BlockPos, VoxelBlock>, x: i32, z: i32) -> Option<i32> {
    let blocks_at_xz: Vec<i32> = blocks
        .keys()
        .filter(|pos| pos.x == x && pos.z == z)
        .map(|pos| pos.y)
        .collect();
    blocks_at_xz.into_iter().max()
}

/// Get neighbor heights for smoothing algorithm
///
/// Returns heights in cardinal directions: [North, South, East, West]
/// Used for terrain smoothing to create ramps.
pub fn get_neighbor_heights(
    blocks: &HashMap<BlockPos, VoxelBlock>,
    pos: BlockPos,
) -> [Option<i32>; 4] {
    [
        get_height(blocks, pos.x, pos.z + 1), // North (+Z)
        get_height(blocks, pos.x, pos.z - 1), // South (-Z)
        get_height(blocks, pos.x + 1, pos.z), // East (+X)
        get_height(blocks, pos.x - 1, pos.z), // West (-X)
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::VoxelBlock;

    #[test]
    fn test_get_height_empty() {
        let blocks = HashMap::new();
        assert_eq!(get_height(&blocks, 0, 0), None);
    }

    #[test]
    fn test_get_height_single_block() {
        let mut blocks = HashMap::new();
        let pos = BlockPos::new(0, 5, 0);
        blocks.insert(pos, VoxelBlock::new(pos, 0));

        assert_eq!(get_height(&blocks, 0, 0), Some(5));
    }

    #[test]
    fn test_get_height_multiple_blocks() {
        let mut blocks = HashMap::new();

        // Add blocks at different heights
        blocks.insert(
            BlockPos::new(0, 1, 0),
            VoxelBlock::new(BlockPos::new(0, 1, 0), 0),
        );
        blocks.insert(
            BlockPos::new(0, 3, 0),
            VoxelBlock::new(BlockPos::new(0, 3, 0), 0),
        );
        blocks.insert(
            BlockPos::new(0, 2, 0),
            VoxelBlock::new(BlockPos::new(0, 2, 0), 0),
        );

        // Should return highest Y
        assert_eq!(get_height(&blocks, 0, 0), Some(3));
    }

    #[test]
    fn test_get_neighbor_heights() {
        let mut blocks = HashMap::new();
        let center = BlockPos::new(10, 5, 10);

        // Set heights for neighbors
        blocks.insert(
            BlockPos::new(10, 3, 11),
            VoxelBlock::new(BlockPos::new(10, 3, 11), 0),
        ); // North
        blocks.insert(
            BlockPos::new(10, 4, 9),
            VoxelBlock::new(BlockPos::new(10, 4, 9), 0),
        ); // South
        blocks.insert(
            BlockPos::new(11, 5, 10),
            VoxelBlock::new(BlockPos::new(11, 5, 10), 0),
        ); // East
        blocks.insert(
            BlockPos::new(9, 6, 10),
            VoxelBlock::new(BlockPos::new(9, 6, 10), 0),
        ); // West

        let heights = get_neighbor_heights(&blocks, center);
        assert_eq!(heights, [Some(3), Some(4), Some(5), Some(6)]);
    }

    #[test]
    fn test_get_neighbor_heights_missing_neighbors() {
        let blocks = HashMap::new();
        let center = BlockPos::new(10, 5, 10);

        let heights = get_neighbor_heights(&blocks, center);
        assert_eq!(heights, [None, None, None, None]);
    }
}
