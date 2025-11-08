//! Chunk coordinate calculations and block retrieval.

use super::super::{BlockPos, VoxelBlock};
use glam::IVec3;
use std::collections::HashMap;

/// Get chunk coordinate from block position
///
/// Uses `div_euclid` for correct negative coordinate handling.
pub fn get_chunk_pos(pos: BlockPos, chunk_size: i32) -> IVec3 {
    IVec3::new(
        pos.x.div_euclid(chunk_size),
        pos.y.div_euclid(chunk_size),
        pos.z.div_euclid(chunk_size),
    )
}

/// Get all blocks in a chunk
///
/// Returns references to all blocks within the specified chunk boundaries.
pub fn get_chunk_blocks<'a>(
    blocks: &'a HashMap<BlockPos, VoxelBlock>,
    chunk_pos: IVec3,
    chunk_size: i32,
) -> Vec<&'a VoxelBlock> {
    let min = chunk_pos * chunk_size;
    let max = min + IVec3::splat(chunk_size);

    blocks
        .values()
        .filter(|b| {
            b.position.x >= min.x
                && b.position.x < max.x
                && b.position.y >= min.y
                && b.position.y < max.y
                && b.position.z >= min.z
                && b.position.z < max.z
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::VoxelBlock;

    #[test]
    fn test_chunk_pos_positive() {
        assert_eq!(
            get_chunk_pos(BlockPos::new(0, 0, 0), 16),
            IVec3::new(0, 0, 0)
        );
        assert_eq!(
            get_chunk_pos(BlockPos::new(15, 15, 15), 16),
            IVec3::new(0, 0, 0)
        );
        assert_eq!(
            get_chunk_pos(BlockPos::new(16, 16, 16), 16),
            IVec3::new(1, 1, 1)
        );
    }

    #[test]
    fn test_chunk_pos_negative() {
        // div_euclid handles negative coordinates correctly
        assert_eq!(
            get_chunk_pos(BlockPos::new(-1, -1, -1), 16),
            IVec3::new(-1, -1, -1)
        );
        assert_eq!(
            get_chunk_pos(BlockPos::new(-16, -16, -16), 16),
            IVec3::new(-1, -1, -1)
        );
    }

    #[test]
    fn test_get_chunk_blocks_empty() {
        let blocks = HashMap::new();
        let chunk_blocks = get_chunk_blocks(&blocks, IVec3::new(0, 0, 0), 16);
        assert_eq!(chunk_blocks.len(), 0);
    }

    #[test]
    fn test_get_chunk_blocks_single_chunk() {
        let mut blocks = HashMap::new();

        // Add blocks in chunk (0, 0, 0)
        blocks.insert(
            BlockPos::new(0, 0, 0),
            VoxelBlock::new(BlockPos::new(0, 0, 0), 0),
        );
        blocks.insert(
            BlockPos::new(5, 5, 5),
            VoxelBlock::new(BlockPos::new(5, 5, 5), 0),
        );

        let chunk_blocks = get_chunk_blocks(&blocks, IVec3::new(0, 0, 0), 16);
        assert_eq!(chunk_blocks.len(), 2);
    }

    #[test]
    fn test_get_chunk_blocks_multiple_chunks() {
        let mut blocks = HashMap::new();

        // Add blocks in chunk (0, 0, 0)
        blocks.insert(
            BlockPos::new(0, 0, 0),
            VoxelBlock::new(BlockPos::new(0, 0, 0), 0),
        );
        blocks.insert(
            BlockPos::new(5, 5, 5),
            VoxelBlock::new(BlockPos::new(5, 5, 5), 0),
        );

        // Add block in different chunk
        blocks.insert(
            BlockPos::new(20, 20, 20),
            VoxelBlock::new(BlockPos::new(20, 20, 20), 0),
        );

        let chunk_blocks_0 = get_chunk_blocks(&blocks, IVec3::new(0, 0, 0), 16);
        assert_eq!(chunk_blocks_0.len(), 2);

        let chunk_blocks_1 = get_chunk_blocks(&blocks, IVec3::new(1, 1, 1), 16);
        assert_eq!(chunk_blocks_1.len(), 1);
    }
}
