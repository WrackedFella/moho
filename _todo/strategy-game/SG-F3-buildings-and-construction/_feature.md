# SG-F3 — Player places buildings from a build menu

**Status:** parked

## Summary

The player opens a context-sensitive build menu and places simple buildings
into the world. Follows [SG-F1](../SG-F1-core-interaction-loop/_feature.md); not decomposed into cards until picked up.

## Scope sketch

- RTS-style action menu (bottom corner): contents follow the current
  selection; with nothing selected it shows build/actions. A generic UI
  system, not hard-coded to buildings. First-person presentation comes later.
- Buildings start as simple geometric shapes.
- Extend the `spawn` console command to spawn a pawn — the same path later
  serves buildings that produce units.

## Notes

- If buildings are made of crafted voxel blocks (material ID 100 or above),
  they take the blocky mesh path. It emits all six faces of every block,
  with no hidden-face culling and no greedy merging, despite its doc
  comments. Measure structure mesh cost before deciding whether culling or
  greedy merging is needed. Issue #26 holds the algorithm notes, but its
  terrain premise is out of date.
