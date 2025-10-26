# engine_core — Conceptual Documentation

This document contains concise conceptual notes for the `engine_core` crate. Detailed, longer-form design text has been archived; this file highlights the key systems and next steps for contributors.

Essentials
- Voxel data is represented by `VoxelGrid` and split into fixed-size chunks by `grid_to_chunks`.
- Block-level face culling is implemented in `VoxelGrid::should_render_face` and `VoxelGrid::get_visible_faces`.
- `VoxelChunk::extract_visible_faces` supports emitting only the faces that are actually visible for partially-occluded blocks.

Outstanding work
- Greedy meshing (intra-chunk rectangle merging) is currently unimplemented; see `docs/ISSUES/greedy-meshing.md` for a prioritized plan and benchmarks.
- Add unit tests for `extract_visible_faces` to lock current behavior before refactors.

For historical notes and full API examples (InstanceGpu layout, shader attribute mappings, etc.) see the archived README contents that were moved into the repository docs.
