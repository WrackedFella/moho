# 0030 — Skinned meshes are skinned once per frame before every pass

**Status:** Accepted on merge of #701; amends [0024](0024-models-are-gltf-data-placed-by-node.md) and [0025](0025-world-meshes-are-placed-per-frame.md): a world mesh may carry a skin and be placed with a joint palette

## Context

[ENG-F39](https://github.com/WrackedFella/moho/issues/635) draws rigged characters.
[ADR-0024](0024-models-are-gltf-data-placed-by-node.md) makes a pose an input to placement
and [ADR-0025](0025-world-meshes-are-placed-per-frame.md) places world meshes per frame with
final transforms. A skinned mesh needs a joint palette per placement, and the shadow cascades
and the main pass all draw it.

## Decision

1. **A skinned placement is a root transform plus a model-space joint palette,** given per
   frame like any placement. The mesh uploads once, with its per-vertex joints and weights.
2. **The renderer skins each skinned placement once per frame** in a compute pass into a
   vertex arena, before any geometry pass. Every geometry pass then draws the arena slice like
   an ordinary mesh, with the mesh's index buffer and the placement's root as the instance
   model.
3. **The skinning shader rewrites only position and normal** and copies every other vertex
   word, so a vertex layout change needs no shader edit.

## Consequences

- No skinned variant of any pipeline, and a new pass gets skinning for free.
- One arena slice of vertex memory per skinned placement per frame (64 bytes a vertex).
- The palette is model space, so sockets on bones (#655) compose from the same pose.
- Four influences per vertex; more influences, dual-quaternion skinning and morph targets
  are outside this decision.
