//! Sky-exposure pass.
//!
//! Computes the per-voxel `sky_exposed` bit for a chunk by:
//!   1. Rebuilding the chunk's `column_max_y` from its block map (using
//!      `MaterialRegistry::opacity_cost` to filter to opaque-only).
//!   2. For each (x, z) column inside the chunk, finding the world-space
//!      highest-opaque-Y by walking column_max_y across all loaded chunks at
//!      the same (chunk_x, chunk_z) and choosing the highest (chunk_y * 16 +
//!      column_max_y).
//!   3. Setting `sky_exposed[idx] = world_y > column_height` for each voxel.
//!
//! Direct sun/moon shadow is CSM's job — `sky_exposed` is direction-independent
//! and gates only the sky-hemisphere ambient term in the renderer.

use super::grid::VoxelGrid;
use super::light_storage::{self, CHUNK_SIZE, CHUNK_USIZE};
use glam::IVec3;

/// Bring the lighting state for `chunk_pos` up-to-date with respect to
/// sky-exposure, if needed. No-op when the chunk's `sky_dirty` is false (or
/// when the grid does not support lighting).
///
/// Rebuilds the chunk's `column_max_y` from its block map then writes the
/// `sky_exposed` bitmask. Cross-chunk column heights from chunks above are
/// taken from their existing `column_max_y` snapshot — those chunks must have
/// been brought up-to-date first via their own `sky_dirty` flow if they
/// changed.
pub fn recompute_sky_exposure(grid: &mut VoxelGrid, chunk_pos: IVec3) {
    if !grid.supports_lighting() {
        return;
    }
    if !grid
        .chunk_light(chunk_pos)
        .map(|cl| cl.sky_dirty)
        .unwrap_or(true)
    {
        return;
    }

    // Step 1: rebuild this chunk's column_max_y from authoritative block data.
    let new_columns = build_column_max_y(grid, chunk_pos);
    {
        let cl = grid.chunk_light_mut(chunk_pos);
        for (x, column) in new_columns.iter().enumerate() {
            for (z, &max_y) in column.iter().enumerate() {
                cl.set_column_max_y(x as i32, z as i32, max_y);
            }
        }
    }

    // Step 2: gather column heights from chunks at the same (cx, cz) — including
    // this chunk and any allocated chunks above. Below-chunks contribute
    // nothing to *this* chunk's sky exposure (sky comes from above).
    let stacked = collect_stacked_column_max(grid, chunk_pos);

    // Step 3: write sky_exposed bits.
    let cl = grid.chunk_light_mut(chunk_pos);
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let col_h_world = world_column_height(&stacked, lx, lz);
            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_pos.y * CHUNK_SIZE + ly;
                let exposed = match col_h_world {
                    Some(h) => world_y > h,
                    None => true,
                };
                let idx = light_storage::local_index(lx, ly, lz);
                cl.set_sky_exposed(idx, exposed);
            }
        }
    }
    cl.sky_dirty = false;
}

/// Scan blocks in `chunk_pos`'s footprint and compute per-(x,z) chunk-local
/// max-Y of opaque blocks. Returns `i8::MIN` for fully-air columns.
fn build_column_max_y(grid: &VoxelGrid, chunk_pos: IVec3) -> [[i8; CHUNK_USIZE]; CHUNK_USIZE] {
    let mut col = [[i8::MIN; CHUNK_USIZE]; CHUNK_USIZE];
    let blocks = grid.chunk_block_data(chunk_pos);
    let chunk_min = chunk_pos * CHUNK_SIZE;
    for b in &blocks {
        let opacity = grid.material_registry.opacity_cost(b.material_id);
        if opacity < 15 {
            continue; // translucent / transparent — does not block sky
        }
        let lx = (b.position.x - chunk_min.x) as usize;
        let ly = (b.position.y - chunk_min.y) as i8;
        let lz = (b.position.z - chunk_min.z) as usize;
        if ly > col[lx][lz] {
            col[lx][lz] = ly;
        }
    }
    col
}

/// Per-(x, z) world-Y of the highest opaque voxel across every allocated chunk
/// at the same `(chunk_pos.x, chunk_pos.z)` and any `chunk_y >= chunk_pos.y`.
/// Lower-Y chunks are excluded — they cannot occlude sky from above.
fn collect_stacked_column_max(
    grid: &VoxelGrid,
    chunk_pos: IVec3,
) -> Vec<(IVec3, [[i8; CHUNK_USIZE]; CHUNK_USIZE])> {
    grid.lit_chunk_positions()
        .filter(|cp| cp.x == chunk_pos.x && cp.z == chunk_pos.z && cp.y >= chunk_pos.y)
        .filter_map(|cp| grid.chunk_light(cp).map(|cl| (cp, *cl.column_max_y_grid())))
        .collect()
}

fn world_column_height(
    stacked: &[(IVec3, [[i8; CHUNK_USIZE]; CHUNK_USIZE])],
    lx: i32,
    lz: i32,
) -> Option<i32> {
    let mut highest: Option<i32> = None;
    for (cp, cols) in stacked {
        let local_max = cols[lx as usize][lz as usize];
        if local_max == i8::MIN {
            continue;
        }
        let world_y = cp.y * CHUNK_SIZE + local_max as i32;
        highest = Some(match highest {
            Some(h) => h.max(world_y),
            None => world_y,
        });
    }
    highest
}

/// Convenience: ensure sky-exposure is computed for the chunk containing `pos`.
pub fn ensure_chunk_sky_ready(grid: &mut VoxelGrid, chunk_pos: IVec3) {
    recompute_sky_exposure(grid, chunk_pos);
}

/// Mark all loaded chunks below `chunk_pos` (in the same xz-column-of-chunks)
/// as `sky_dirty`. Called after any block change in `chunk_pos` that could
/// shift the world column height seen by lower chunks.
pub fn dirty_chunks_below(grid: &mut VoxelGrid, chunk_pos: IVec3) {
    if !grid.supports_lighting() {
        return;
    }
    let lower: Vec<IVec3> = grid
        .lit_chunk_positions()
        .filter(|cp| cp.x == chunk_pos.x && cp.z == chunk_pos.z && cp.y < chunk_pos.y)
        .collect();
    for cp in lower {
        let cl = grid.chunk_light_mut(cp);
        cl.sky_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::VoxelGrid;
    use glam::IVec3;

    fn empty_grid() -> VoxelGrid {
        VoxelGrid::new(16)
    }

    #[test]
    fn open_air_chunk_is_fully_sky_exposed() {
        let mut grid = empty_grid();
        let cp = IVec3::ZERO;
        // Allocate the chunk-light entry by setting any value.
        grid.chunk_light_mut(cp);
        recompute_sky_exposure(&mut grid, cp);
        let cl = grid.chunk_light(cp).unwrap();
        for idx in 0..4096 {
            assert!(
                cl.sky_exposed_at(idx),
                "voxel {idx} should be sky-exposed in empty chunk"
            );
        }
    }

    #[test]
    fn voxel_below_opaque_block_is_not_exposed() {
        let mut grid = empty_grid();
        // Place an opaque block at world (5, 5, 5). Material 0 (grass) defaults
        // to opaque (opacity_cost = 15) per `MaterialLighting::NON_EMISSIVE_OPAQUE`.
        grid.place_block(IVec3::new(5, 5, 5), 0, None);
        let cp = IVec3::ZERO;
        recompute_sky_exposure(&mut grid, cp);
        let cl = grid.chunk_light(cp).unwrap();
        // (5, 4, 5): below the block, same column → not exposed.
        let below = light_storage::local_index(5, 4, 5);
        assert!(!cl.sky_exposed_at(below));
        // (5, 6, 5): above the block, same column → exposed.
        let above = light_storage::local_index(5, 6, 5);
        assert!(cl.sky_exposed_at(above));
        // (4, 4, 5): adjacent column at same y → exposed.
        let adj = light_storage::local_index(4, 4, 5);
        assert!(cl.sky_exposed_at(adj));
    }

    #[test]
    fn block_in_chunk_above_occludes_chunks_below() {
        let mut grid = empty_grid();
        // Place opaque block at world (3, 20, 3) → chunk (0, 1, 0), local y=4.
        grid.place_block(IVec3::new(3, 20, 3), 0, None);
        // First, bring the chunk-above's heightmap up to date.
        recompute_sky_exposure(&mut grid, IVec3::new(0, 1, 0));
        // Now compute sky exposure for the chunk below it.
        recompute_sky_exposure(&mut grid, IVec3::ZERO);
        let cl = grid.chunk_light(IVec3::ZERO).unwrap();
        // Voxel (3, 15, 3) in chunk (0,0,0) — directly below the block at (3,20,3).
        // Should NOT be sky-exposed.
        let below = light_storage::local_index(3, 15, 3);
        assert!(!cl.sky_exposed_at(below));
        // Voxel (5, 15, 5) in chunk (0,0,0) — adjacent column, no occluder.
        let adj = light_storage::local_index(5, 15, 5);
        assert!(cl.sky_exposed_at(adj));
    }
}
