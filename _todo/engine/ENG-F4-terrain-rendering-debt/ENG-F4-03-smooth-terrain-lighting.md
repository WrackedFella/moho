# Smooth-terrain per-vertex lighting is a placeholder

**Status:** not started
**Feature:** ENG-F4

## Summary

Marching-cubes terrain defaults to `sky_exposed = 1`, `block_light = 0` — no
real per-vertex lighting. The shipped fragment-shader blend
(`max(block_light_rgb, vec3(sky_exposed))`) is also a simplification of the
originally intended equation.

## Deliverables

- Trilinear sampling from the 8 corner voxels for real per-vertex light on
  smooth terrain.
- Revisit the blend equation if the simplified version doesn't look right in
  practice.
