# SG-F3 — Player places buildings from a build menu

**Status:** parked

## Summary

The player opens a context-sensitive build menu and places simple buildings
into the world. Follows SG-F1; not decomposed into cards until picked up.

## Scope sketch

- RTS-style action menu (bottom corner): contents follow the current
  selection; with nothing selected it shows build/actions. A generic UI
  system, not hard-coded to buildings. First-person presentation comes later.
- Buildings start as simple geometric shapes.
- Extend the `spawn` console command to spawn a pawn — the same path later
  serves buildings that produce units.
