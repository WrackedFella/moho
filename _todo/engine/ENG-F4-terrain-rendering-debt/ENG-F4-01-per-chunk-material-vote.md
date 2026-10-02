# Per-chunk material majority vote is wrong

**Status:** not started
**Feature:** ENG-F4

## Summary

`VoxelChunk::from_grid_hybrid` picks the most common `material_id` in a chunk
and renders the whole chunk with it — a chunk with grass/dirt/stone renders as
whichever was most numerous.

## Deliverables

- Per-vertex material index in the mesh + texture array in the shader, or
  split a chunk's mesh into one draw call per material used.

## Notes

Natural timing: paletted chunks already give each voxel a palette index, so
per-vertex material is now cheap to add.
