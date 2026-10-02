# LOD1 mesh has holes and looks too blocky

**Status:** not started
**Feature:** ENG-F4

## Summary

Near the LOD0→LOD1 boundary (~4 chunks out), distant chunks show holes at
chunk edges and look blockier than the distance warrants.

## Deliverables

- ±Y skirt quads (only ±X/±Z exist today — a LOD0 chunk above/below a LOD1
  chunk shows a crack).
- Widen the LOD0 radius, or add a LOD 0.5 blocky-no-MC pass in the 4-8 chunk
  band, to soften the quality step.
- Consider max-height-in-cell instead of any-solid-wins for the coarse
  silhouette.

## Notes

Files: `moho_core/src/voxel/mesh/hybrid.rs` (`generate_coarse_mesh`,
`emit_coarse_quad`), `lod_for_chunk` in `event_processor.rs` for threshold
tuning.
