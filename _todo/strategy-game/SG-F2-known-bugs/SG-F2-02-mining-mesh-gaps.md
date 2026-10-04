# Mining can leave gaps in nearby terrain

**Status:** not started
**Feature:** [SG-F2](_feature.md)

## Summary

Removing a voxel near a chunk boundary sometimes tears a visible gap beyond
the mined block, instead of cleanly closing over the missing material.

## Deliverables

- When a mined voxel is within one cell of a chunk boundary, also dirty the
  affected face-neighbor chunk(s) — mirror the neighbor-remesh pattern chunk
  streaming already uses on load (`update_chunk_streaming`'s `FACE_DIRS`
  re-dirty), which mining's `handle_mine_requested` doesn't currently do.
