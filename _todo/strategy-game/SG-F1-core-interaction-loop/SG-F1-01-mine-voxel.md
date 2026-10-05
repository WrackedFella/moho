# Player mines a voxel and receives its resource

**Status:** done
**Feature:** [SG-F1](_feature.md)

## Summary

Left-click raycasts from the camera, breaks the targeted voxel, deposits its
resource into the player's inventory. `Pawn::mine(origin, dir, max_distance)`
takes plain glam types so AI-controlled pawns can reuse it later.

## Deliverables

- `moho_game::pawn::{Pawn, Inventory, ResourceYield}` — done.
- `Pawn::mine` — raycast, break, deposit — done.
- Left-click wired through `InputDispatcher`, UI-gated — done.

## Notes

Found and fixed two pre-existing engine bugs along the way: smooth-terrain
mesh offset (+0.5 on every axis vs. the voxel grid), and a raycast/renderer
surface mismatch (69% of visible surface sat in voxels the grid called
empty — new `raycast_surface` fixes it). See `moho_core/tests/mesh_alignment.rs`
and `moho_game/tests/surface_raycast.rs`.
