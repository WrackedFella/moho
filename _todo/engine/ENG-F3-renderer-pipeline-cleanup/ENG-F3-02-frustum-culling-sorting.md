# CPU frustum culling + draw sorting

**Status:** not started
**Feature:** [ENG-F3](_feature.md)

## Summary

Every registered `Renderable` draws every frame regardless of visibility. No
draw sorting.

## Deliverables

- Mesh registration carries an AABB.
- Per-`Renderable`, transform AABB and cull against the view frustum.
- Opaque draws sorted front-to-back; grouped by `(mesh_handle, material)`.
- Shadow-frustum culling per light, same module.
