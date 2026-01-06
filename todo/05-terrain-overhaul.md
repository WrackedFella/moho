# Phase 5: Terrain Overhaul & Research

**Goal:** Research high-fidelity voxel terrain architectures (Sub-voxels / Micro-grids).

## Research: Architecture
- **Problem:** Uniform high-density grids consume too much memory.
- **Investigation:** Compare:
    1. **Sparse Voxel Octrees (SVO):** hierarchically storing empty space.
    2. **Paletted / RLE Chunks:** Run-Length Encoding uniform chunks, expanding to arrays only when modified.
    3. **Micro-Grid Deletion:** Standard map is large blocks (1m). "Mining" replaces a 1m block with a 8x8x8 micro-grid entity. Visual distinction between "World Terrain" and "Deformable Terrain".

## Prototype: Micro-Grid Deletion
- **Concept:**
    - World is stored as broad integers (Stone, Dirt).
    - When a player targets a block, it converts to a `MicroChunk` container.
    - `MicroChunk` contains sub-voxels that can be individually removed.
- **Challenge:** Meshing.
    - Needs a special mesher that handles the transition between the uniform world and the high-res micro-chunk.

## Implementation Steps
1. Create `MicroVoxel` struct (smaller data type).
2. Create `MicroChunk` component.
3. Update Raycast to hit sub-voxels.
4. Update Mesher to stitch seams (or accept visual seams for prototype).
