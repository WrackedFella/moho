//! Light propagation algorithms for voxel lighting.
//!
//! This module implements Minecraft-style light spreading with:
//! - Initial flood-fill for world generation
//! - Incremental addition when light sources are placed
//! - Two-phase removal when light sources are removed
//! - Chunk boundary handling
//!
//! Light levels range from 0 (dark) to 15 (full brightness).
//! Light decreases by 1 per block traveled.

use crate::voxel::grid::{BlockPos, VoxelGrid};
use glam::IVec3;
use std::collections::VecDeque;

/// Light channel type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightChannel {
    /// Sky light from sun/moon (penetrates downward)
    Sky,
    /// Block light from torches and emissives
    Block,
}

/// Light propagation node for BFS
#[derive(Debug, Clone)]
struct LightNode {
    position: BlockPos,
    light_level: u8,
}

/// Light propagation system
pub struct LightPropagator {
    /// Work queue for BFS
    queue: VecDeque<LightNode>,
    /// Chunk size for boundary detection
    chunk_size: i32,
}

impl LightPropagator {
    /// Create a new light propagator
    pub fn new(chunk_size: i32) -> Self {
        Self {
            queue: VecDeque::with_capacity(1024),
            chunk_size,
        }
    }

    /// Perform initial flood-fill from all light sources.
    ///
    /// This is used for:
    /// - Initial world generation
    /// - Large regenerations after terrain modification
    /// - Sky light calculation
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to propagate light through
    /// * `channel` - Which light channel to propagate (sky or block)
    ///
    /// # Performance
    /// This is an expensive operation (O(N) where N = lit blocks).
    /// Should be run async for large areas.
    pub fn flood_fill(&mut self, grid: &mut VoxelGrid, channel: LightChannel) {
        self.queue.clear();

        // Find all light sources and enqueue them
        match channel {
            LightChannel::Sky => {
                // Sky light: start from top layer of blocks
                self.enqueue_sky_sources(grid);
            }
            LightChannel::Block => {
                // Block light: start from emissive blocks
                self.enqueue_block_sources(grid);
            }
        }

        // BFS propagation from all sources
        self.propagate_light(grid, channel);
    }

    /// Add light incrementally from a single source position.
    ///
    /// This is much faster than flood_fill for local changes like placing a torch.
    /// Only propagates where the new light would be brighter than existing light.
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to propagate light through
    /// * `source_pos` - Position of the new light source
    /// * `light_level` - Initial light level at source (typically 15 for torch)
    /// * `channel` - Which light channel to propagate (sky or block)
    ///
    /// # Returns
    /// List of chunk coordinates that were affected (for dirty marking)
    ///
    /// # Performance
    /// Typical torch placement: <2ms for ~500-1000 affected blocks
    pub fn add_light(
        &mut self,
        grid: &mut VoxelGrid,
        source_pos: BlockPos,
        light_level: u8,
        channel: LightChannel,
    ) -> Vec<IVec3> {
        self.queue.clear();
        let mut affected_chunks = std::collections::HashSet::new();

        // Start from the source position
        self.queue.push_back(LightNode {
            position: source_pos,
            light_level,
        });

        // Track which chunks are affected
        affected_chunks.insert(self.get_chunk_coord(&source_pos));

        // BFS propagation (same as flood_fill but starting from single point)
        while let Some(node) = self.queue.pop_front() {
            // Set light level at current position
            if let Some(block) = grid.get_block_mut(&node.position) {
                let current_light = match channel {
                    LightChannel::Sky => block.sky_light,
                    LightChannel::Block => block.block_light,
                };

                // Only update if new light is brighter
                if node.light_level <= current_light {
                    continue;
                }

                match channel {
                    LightChannel::Sky => block.set_sky_light(node.light_level),
                    LightChannel::Block => block.set_block_light(node.light_level),
                }

                // Track this chunk as affected (we actually modified a block here)
                affected_chunks.insert(self.get_chunk_coord(&node.position));
            }

            // Calculate light level for neighbors (decay by 1)
            let neighbor_light = node.light_level.saturating_sub(1);
            if neighbor_light == 0 {
                continue; // No more light to propagate
            }

            // Propagate to 6 neighbors (±X, ±Y, ±Z)
            let neighbors = [
                node.position + IVec3::new(1, 0, 0),
                node.position + IVec3::new(-1, 0, 0),
                node.position + IVec3::new(0, 1, 0),
                node.position + IVec3::new(0, -1, 0),
                node.position + IVec3::new(0, 0, 1),
                node.position + IVec3::new(0, 0, -1),
            ];

            for neighbor_pos in neighbors {
                // Check if neighbor can receive light
                if self.can_receive_light(grid, &neighbor_pos, channel) {
                    let current_neighbor_light = grid
                        .get_block(&neighbor_pos)
                        .map(|b| match channel {
                            LightChannel::Sky => b.sky_light,
                            LightChannel::Block => b.block_light,
                        })
                        .unwrap_or(0);

                    // Only enqueue if new light would be brighter
                    if neighbor_light > current_neighbor_light {
                        self.queue.push_back(LightNode {
                            position: neighbor_pos,
                            light_level: neighbor_light,
                        });
                    }
                }
            }
        }

        affected_chunks.into_iter().collect()
    }

    /// Remove light from a source position using two-phase BFS.
    ///
    /// This is more complex than addition because removing a light source requires:
    /// 1. Finding all blocks that were lit by the removed source
    /// 2. Clearing those light values
    /// 3. Re-flooding from any adjacent light sources to restore correct lighting
    ///
    /// # Arguments
    /// * `grid` - The voxel grid to remove light from
    /// * `source_pos` - Position of the removed light source
    /// * `channel` - Which light channel to modify (sky or block)
    ///
    /// # Returns
    /// List of chunk coordinates that were affected (for dirty marking)
    ///
    /// # Performance
    /// Typical torch removal: <5ms for ~500-1000 affected blocks
    pub fn remove_light(
        &mut self,
        grid: &mut VoxelGrid,
        source_pos: BlockPos,
        channel: LightChannel,
    ) -> Vec<IVec3> {
        self.queue.clear();
        let mut affected_chunks = std::collections::HashSet::new();
        let mut removal_queue: VecDeque<BlockPos> = VecDeque::with_capacity(512);
        let mut border_lights: Vec<LightNode> = Vec::with_capacity(128);

        // Track which blocks have been visited to avoid re-processing
        let mut visited: std::collections::HashSet<BlockPos> = std::collections::HashSet::new();

        // Phase 1: BFS to find all blocks that need to be cleared
        // We clear any block that has weaker light than its neighbors would provide

        removal_queue.push_back(source_pos);
        visited.insert(source_pos);

        while let Some(pos) = removal_queue.pop_front() {
            affected_chunks.insert(self.get_chunk_coord(&pos));

            let current_light = grid
                .get_block(&pos)
                .map(|b| match channel {
                    LightChannel::Sky => b.sky_light,
                    LightChannel::Block => b.block_light,
                })
                .unwrap_or(0);

            // Check all 6 neighbors
            let neighbors = [
                pos + IVec3::new(1, 0, 0),
                pos + IVec3::new(-1, 0, 0),
                pos + IVec3::new(0, 1, 0),
                pos + IVec3::new(0, -1, 0),
                pos + IVec3::new(0, 0, 1),
                pos + IVec3::new(0, 0, -1),
            ];

            for neighbor_pos in neighbors {
                if visited.contains(&neighbor_pos) {
                    continue;
                }

                if let Some(neighbor_block) = grid.get_block(&neighbor_pos) {
                    let neighbor_light = match channel {
                        LightChannel::Sky => neighbor_block.sky_light,
                        LightChannel::Block => neighbor_block.block_light,
                    };

                    if neighbor_light == 0 {
                        continue; // Already dark
                    }

                    // If neighbor has less light than current, it was lit by this source
                    // If neighbor has equal or more light, it's a border light from another source
                    if neighbor_light < current_light {
                        // This neighbor was lit by the removed source chain
                        visited.insert(neighbor_pos);
                        removal_queue.push_back(neighbor_pos);
                    } else if neighbor_light >= current_light {
                        // This is a potential border light from another source
                        // Only add if it could actually provide light (not already added)
                        if !border_lights.iter().any(|n| n.position == neighbor_pos) {
                            border_lights.push(LightNode {
                                position: neighbor_pos,
                                light_level: neighbor_light,
                            });
                        }
                    }
                }
            }
        }

        // Phase 2: Clear light values for all visited blocks
        for pos in &visited {
            if let Some(block) = grid.get_block_mut(pos) {
                match channel {
                    LightChannel::Sky => block.set_sky_light(0),
                    LightChannel::Block => block.set_block_light(0),
                }
            }
        }

        // Phase 3: Re-flood from border lights
        // These are blocks adjacent to the cleared area that still have light
        for light_node in border_lights {
            // Re-propagate light from this border node
            self.queue.push_back(light_node);
        }

        // Use the standard propagation logic to re-light the area
        while let Some(node) = self.queue.pop_front() {
            // Calculate light level for neighbors (decay by 1)
            let neighbor_light = node.light_level.saturating_sub(1);
            if neighbor_light == 0 {
                continue;
            }

            let neighbors = [
                node.position + IVec3::new(1, 0, 0),
                node.position + IVec3::new(-1, 0, 0),
                node.position + IVec3::new(0, 1, 0),
                node.position + IVec3::new(0, -1, 0),
                node.position + IVec3::new(0, 0, 1),
                node.position + IVec3::new(0, 0, -1),
            ];

            for neighbor_pos in neighbors {
                affected_chunks.insert(self.get_chunk_coord(&neighbor_pos));

                if self.can_receive_light(grid, &neighbor_pos, channel) {
                    let current_neighbor_light = grid
                        .get_block(&neighbor_pos)
                        .map(|b| match channel {
                            LightChannel::Sky => b.sky_light,
                            LightChannel::Block => b.block_light,
                        })
                        .unwrap_or(0);

                    // Only update if new light would be brighter
                    if neighbor_light > current_neighbor_light {
                        if let Some(block) = grid.get_block_mut(&neighbor_pos) {
                            match channel {
                                LightChannel::Sky => block.set_sky_light(neighbor_light),
                                LightChannel::Block => block.set_block_light(neighbor_light),
                            }
                        }

                        self.queue.push_back(LightNode {
                            position: neighbor_pos,
                            light_level: neighbor_light,
                        });
                    }
                }
            }
        }

        affected_chunks.into_iter().collect()
    }

    /// Enqueue sky light sources (top layer of blocks exposed to sky)
    fn enqueue_sky_sources(&mut self, grid: &VoxelGrid) {
        // Get all block positions and find the highest Y for each X,Z column
        let mut columns: std::collections::HashMap<(i32, i32), i32> =
            std::collections::HashMap::new();

        for pos in grid.block_positions() {
            let column = (pos.x, pos.z);
            let entry = columns.entry(column).or_insert(pos.y);
            *entry = (*entry).max(pos.y);
        }

        // Enqueue top blocks with full sky light
        for ((x, z), max_y) in columns {
            let pos = IVec3::new(x, max_y, z);
            if grid.get_block(&pos).is_some() {
                self.queue.push_back(LightNode {
                    position: pos,
                    light_level: 15, // Full sky light at top
                });
            }
        }
    }

    /// Enqueue block light sources (emissive blocks like torches)
    fn enqueue_block_sources(&mut self, grid: &VoxelGrid) {
        for pos in grid.block_positions() {
            if let Some(block) = grid.get_block(pos) {
                let emission = block.emission_level();
                if emission > 0 {
                    self.queue.push_back(LightNode {
                        position: *pos,
                        light_level: emission,
                    });
                }
            }
        }
    }

    /// Propagate light through the grid using BFS
    fn propagate_light(&mut self, grid: &mut VoxelGrid, channel: LightChannel) {
        while let Some(node) = self.queue.pop_front() {
            // Set light level at current position
            if let Some(block) = grid.get_block_mut(&node.position) {
                let current_light = match channel {
                    LightChannel::Sky => block.sky_light,
                    LightChannel::Block => block.block_light,
                };

                // Only update if new light is brighter
                if node.light_level <= current_light {
                    continue;
                }

                match channel {
                    LightChannel::Sky => block.set_sky_light(node.light_level),
                    LightChannel::Block => block.set_block_light(node.light_level),
                }
            }

            // Calculate light level for neighbors (decay by 1)
            let neighbor_light = node.light_level.saturating_sub(1);
            if neighbor_light == 0 {
                continue; // No more light to propagate
            }

            // Propagate to 6 neighbors (±X, ±Y, ±Z)
            let neighbors = [
                node.position + IVec3::new(1, 0, 0),
                node.position + IVec3::new(-1, 0, 0),
                node.position + IVec3::new(0, 1, 0),
                node.position + IVec3::new(0, -1, 0),
                node.position + IVec3::new(0, 0, 1),
                node.position + IVec3::new(0, 0, -1),
            ];

            for neighbor_pos in neighbors {
                // Check if neighbor can receive light
                if self.can_receive_light(grid, &neighbor_pos, channel) {
                    let current_neighbor_light = grid
                        .get_block(&neighbor_pos)
                        .map(|b| match channel {
                            LightChannel::Sky => b.sky_light,
                            LightChannel::Block => b.block_light,
                        })
                        .unwrap_or(0);

                    // Only enqueue if new light would be brighter
                    if neighbor_light > current_neighbor_light {
                        self.queue.push_back(LightNode {
                            position: neighbor_pos,
                            light_level: neighbor_light,
                        });
                    }
                }
            }
        }
    }

    /// Check if a position can receive light
    fn can_receive_light(&self, grid: &VoxelGrid, pos: &BlockPos, channel: LightChannel) -> bool {
        // Check if there's a block at this position
        let has_block = grid.get_block(pos).is_some();

        match channel {
            LightChannel::Sky => {
                // Sky light propagates to blocks only (not infinite air)
                has_block
            }
            LightChannel::Block => {
                // Block light also only affects existing blocks
                has_block
            }
        }
    }

    /// Get the chunk coordinate for a block position
    #[inline]
    pub fn get_chunk_coord(&self, pos: &BlockPos) -> IVec3 {
        IVec3::new(
            pos.x.div_euclid(self.chunk_size),
            pos.y.div_euclid(self.chunk_size),
            pos.z.div_euclid(self.chunk_size),
        )
    }

    /// Check if a block position is at a chunk boundary
    #[inline]
    #[allow(dead_code)]
    fn is_at_chunk_boundary(&self, pos: &BlockPos) -> bool {
        pos.x % self.chunk_size == 0
            || pos.x % self.chunk_size == self.chunk_size - 1
            || pos.y % self.chunk_size == 0
            || pos.y % self.chunk_size == self.chunk_size - 1
            || pos.z % self.chunk_size == 0
            || pos.z % self.chunk_size == self.chunk_size - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::grid::VoxelBlock;

    #[test]
    fn test_light_propagation_basic() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a simple 3x3x3 cube of blocks
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    let pos = IVec3::new(x, y, z);
                    let block = VoxelBlock::new(pos, 0);
                    grid.set_block(pos, block);
                }
            }
        }

        // Run sky light flood fill
        propagator.flood_fill(&mut grid, LightChannel::Sky);

        // Check that top layer has full light
        for x in 0..3 {
            for z in 0..3 {
                let top_pos = IVec3::new(x, 2, z);
                if let Some(block) = grid.get_block(&top_pos) {
                    assert_eq!(block.sky_light, 15, "Top block should have full sky light");
                }
            }
        }

        // Check that lower layers have reduced light
        for x in 0..3 {
            for z in 0..3 {
                let mid_pos = IVec3::new(x, 1, z);
                if let Some(block) = grid.get_block(&mid_pos) {
                    assert!(
                        block.sky_light > 0 && block.sky_light <= 15,
                        "Mid block should have some sky light"
                    );
                }
            }
        }
    }

    #[test]
    fn test_chunk_boundary_detection() {
        let propagator = LightPropagator::new(16);

        // Test corner of chunk
        assert!(propagator.is_at_chunk_boundary(&IVec3::new(0, 0, 0)));
        assert!(propagator.is_at_chunk_boundary(&IVec3::new(15, 15, 15)));

        // Test middle of chunk
        assert!(!propagator.is_at_chunk_boundary(&IVec3::new(8, 8, 8)));
    }

    #[test]
    fn test_incremental_light_addition() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a 5x5x5 cube of blocks
        for x in 0..5 {
            for y in 0..5 {
                for z in 0..5 {
                    let pos = IVec3::new(x, y, z);
                    let mut block = VoxelBlock::new(pos, 0);
                    // Start with no light
                    block.set_block_light(0);
                    grid.set_block(pos, block);
                }
            }
        }

        // Add a light source in the center at level 15
        let light_pos = IVec3::new(2, 2, 2);
        let affected_chunks = propagator.add_light(&mut grid, light_pos, 15, LightChannel::Block);

        // Check that the source has full light
        if let Some(block) = grid.get_block(&light_pos) {
            assert_eq!(block.block_light, 15, "Source should have full light");
        }

        // Check neighbors have decayed light
        let neighbor = IVec3::new(3, 2, 2);
        if let Some(block) = grid.get_block(&neighbor) {
            assert_eq!(block.block_light, 14, "Neighbor should have light - 1");
        }

        // Check that affected chunks were tracked
        assert!(!affected_chunks.is_empty(), "Should track affected chunks");
    }

    #[test]
    fn test_incremental_light_stops_at_brighter() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a line of 5 blocks
        for x in 0..5 {
            let pos = IVec3::new(x, 0, 0);
            let mut block = VoxelBlock::new(pos, 0);
            // Pre-fill with existing light
            block.set_block_light(10);
            grid.set_block(pos, block);
        }

        // Try to add a weaker light source at one end
        let weak_light_pos = IVec3::new(0, 0, 0);
        propagator.add_light(&mut grid, weak_light_pos, 8, LightChannel::Block);

        // Check that existing brighter light was not overwritten
        let far_pos = IVec3::new(4, 0, 0);
        if let Some(block) = grid.get_block(&far_pos) {
            assert_eq!(
                block.block_light, 10,
                "Existing brighter light should not be reduced"
            );
        }
    }

    #[test]
    fn test_light_removal_basic() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a 5x5x5 cube of blocks
        for x in 0..5 {
            for y in 0..5 {
                for z in 0..5 {
                    let pos = IVec3::new(x, y, z);
                    let mut block = VoxelBlock::new(pos, 0);
                    block.set_block_light(0);
                    grid.set_block(pos, block);
                }
            }
        }

        // Add a light source in the center
        let light_pos = IVec3::new(2, 2, 2);
        propagator.add_light(&mut grid, light_pos, 15, LightChannel::Block);

        // Verify light was added
        if let Some(block) = grid.get_block(&light_pos) {
            assert_eq!(block.block_light, 15, "Source should have full light");
        }

        // Verify neighbors have light
        let neighbor = IVec3::new(3, 2, 2);
        if let Some(block) = grid.get_block(&neighbor) {
            assert!(block.block_light > 0, "Neighbor should have some light");
        }

        // Now remove the light source
        let affected_chunks = propagator.remove_light(&mut grid, light_pos, LightChannel::Block);

        // Check that the source is now dark
        if let Some(block) = grid.get_block(&light_pos) {
            assert_eq!(block.block_light, 0, "Source should be dark after removal");
        }

        // Check that neighbors are also dark
        if let Some(block) = grid.get_block(&neighbor) {
            assert_eq!(
                block.block_light, 0,
                "Neighbor should be dark after removal"
            );
        }

        // Check that affected chunks were tracked
        assert!(!affected_chunks.is_empty(), "Should track affected chunks");
    }

    #[test]
    fn test_light_removal_with_multiple_sources() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a line of 11 blocks
        for x in 0..11 {
            let pos = IVec3::new(x, 0, 0);
            let mut block = VoxelBlock::new(pos, 0);
            block.set_block_light(0);
            grid.set_block(pos, block);
        }

        // Add two light sources at opposite ends
        let light1_pos = IVec3::new(0, 0, 0);
        let light2_pos = IVec3::new(10, 0, 0);
        propagator.add_light(&mut grid, light1_pos, 15, LightChannel::Block);
        propagator.add_light(&mut grid, light2_pos, 15, LightChannel::Block);

        // Middle block should have light from both sources
        let middle_pos = IVec3::new(5, 0, 0);
        if let Some(block) = grid.get_block(&middle_pos) {
            assert!(
                block.block_light > 0,
                "Middle should have light from both sources"
            );
        }

        // Remove the first light source
        propagator.remove_light(&mut grid, light1_pos, LightChannel::Block);

        // First light position should now have light from the second source only
        // Distance from light2 is 10, so light level = 15 - 10 = 5
        if let Some(block) = grid.get_block(&light1_pos) {
            assert_eq!(
                block.block_light, 5,
                "First light position should now have light from second source (15 - 10 distance)"
            );
        }

        // Second light should still be bright
        if let Some(block) = grid.get_block(&light2_pos) {
            assert_eq!(block.block_light, 15, "Second light should still be bright");
        }

        // Middle block should still have some light from second source
        if let Some(block) = grid.get_block(&middle_pos) {
            assert!(
                block.block_light > 0,
                "Middle should still have light from second source"
            );
            // Should have light level 10 (15 - 5 distance from light2)
            assert_eq!(
                block.block_light, 10,
                "Middle should have correct light level from second source"
            );
        }

        // Block very close to first light should only have light from second source
        // Distance from light2 to position (1,0,0) is 9, so light = 15 - 9 = 6
        let near_first = IVec3::new(1, 0, 0);
        if let Some(block) = grid.get_block(&near_first) {
            assert_eq!(
                block.block_light, 6,
                "Block near first light should have light from second source"
            );
        }
    }

    #[test]
    fn test_light_removal_corner_case() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a T-shape: one light at center, blocks extending in 3 directions
        let center = IVec3::new(5, 5, 5);
        let mut center_block = VoxelBlock::new(center, 0);
        center_block.set_block_light(0);
        grid.set_block(center, center_block);

        // Extend in +X, +Y, +Z directions
        for i in 1..5 {
            for &dir in &[
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ] {
                let pos = center + dir;
                let mut block = VoxelBlock::new(pos, 0);
                block.set_block_light(0);
                grid.set_block(pos, block);
            }
        }

        // Add light at center
        propagator.add_light(&mut grid, center, 15, LightChannel::Block);

        // Check that all arms have light
        for i in 1..5 {
            for &dir in &[
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ] {
                let pos = center + dir;
                if let Some(block) = grid.get_block(&pos) {
                    assert!(block.block_light > 0, "Arm block should have light");
                }
            }
        }

        // Remove the light
        propagator.remove_light(&mut grid, center, LightChannel::Block);

        // Check that all blocks are now dark
        if let Some(block) = grid.get_block(&center) {
            assert_eq!(block.block_light, 0, "Center should be dark");
        }

        for i in 1..5 {
            for &dir in &[
                IVec3::new(i, 0, 0),
                IVec3::new(0, i, 0),
                IVec3::new(0, 0, i),
            ] {
                let pos = center + dir;
                if let Some(block) = grid.get_block(&pos) {
                    assert_eq!(block.block_light, 0, "Arm block should be dark");
                }
            }
        }
    }

    #[test]
    fn test_cross_chunk_propagation() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Place a light source at the edge of chunk (0,0,0)
        // at position (15, 8, 8) - this is the last X block in the chunk
        let light_pos = IVec3::new(15, 8, 8);

        // Create blocks in two adjacent chunks
        // Chunk (0,0,0): blocks at x=14, x=15
        // Chunk (1,0,0): blocks at x=16, x=17, x=18
        for x in 14..=18 {
            let pos = IVec3::new(x, 8, 8);
            let mut block = VoxelBlock::new(pos, 0);
            block.set_block_light(0); // Start with no light
            grid.set_block(pos, block);
        }

        // Propagate light from the source (don't pre-set the source block light)
        let affected_chunks = propagator.add_light(&mut grid, light_pos, 15, LightChannel::Block);

        // Verify that both chunks are marked as affected
        let chunk_0 = IVec3::new(0, 0, 0);
        let chunk_1 = IVec3::new(1, 0, 0);
        assert!(
            affected_chunks.contains(&chunk_0),
            "Chunk 0 should be affected (contains light source)"
        );
        assert!(
            affected_chunks.contains(&chunk_1),
            "Chunk 1 should be affected (light propagates into it)"
        );

        // Verify light levels decay correctly across the boundary
        // x=15: 15 (source)
        // x=16: 14 (in chunk 1)
        // x=17: 13 (in chunk 1)
        // x=18: 12 (in chunk 1)
        let expected_levels = [(15, 15), (16, 14), (17, 13), (18, 12)];

        for (x, expected_light) in expected_levels {
            let pos = IVec3::new(x, 8, 8);
            if let Some(block) = grid.get_block(&pos) {
                assert_eq!(
                    block.block_light, expected_light,
                    "Block at x={} should have light level {}",
                    x, expected_light
                );
            }
        }
    }

    #[test]
    fn test_cross_chunk_removal() {
        let mut grid = VoxelGrid::new(16);
        let mut propagator = LightPropagator::new(16);

        // Create a line of blocks spanning two chunks
        // Chunk (0,0,0): x=12..=15
        // Chunk (1,0,0): x=16..=20
        for x in 12..=20 {
            let pos = IVec3::new(x, 8, 8);
            let mut block = VoxelBlock::new(pos, 0);
            block.set_block_light(0);
            grid.set_block(pos, block);
        }

        // Place light at x=15 (edge of chunk 0)
        let light_pos = IVec3::new(15, 8, 8);
        propagator.add_light(&mut grid, light_pos, 15, LightChannel::Block);

        // Verify light propagated into both chunks
        assert!(grid.get_block(&IVec3::new(14, 8, 8)).unwrap().block_light > 0);
        assert!(grid.get_block(&IVec3::new(16, 8, 8)).unwrap().block_light > 0);

        // Remove the light
        let affected_chunks = propagator.remove_light(&mut grid, light_pos, LightChannel::Block);

        // Verify both chunks are marked as affected
        let chunk_0 = IVec3::new(0, 0, 0);
        let chunk_1 = IVec3::new(1, 0, 0);
        assert!(
            affected_chunks.contains(&chunk_0),
            "Chunk 0 should be affected by light removal"
        );
        assert!(
            affected_chunks.contains(&chunk_1),
            "Chunk 1 should be affected by light removal"
        );

        // Verify all blocks are now dark
        for x in 12..=20 {
            let pos = IVec3::new(x, 8, 8);
            if let Some(block) = grid.get_block(&pos) {
                assert_eq!(
                    block.block_light, 0,
                    "Block at x={} should be dark after light removal",
                    x
                );
            }
        }
    }
}
