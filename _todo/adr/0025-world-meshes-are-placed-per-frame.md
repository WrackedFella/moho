# 0025 — World meshes are placed per frame

**Status:** Accepted on merge of #648; amends [0010](0010-world-geometry-is-a-mesh-contract.md): a world mesh may also be placed at transforms the game sets each frame

## Context

[ADR-0010](0010-world-geometry-is-a-mesh-contract.md)'s contract draws each mesh once, at the
identity, so moving geometry is rebuilt and re-uploaded every frame. Characters, loose
objects ([#600](https://github.com/WrackedFella/moho/issues/600)), models (ENG-F39,
[#635](https://github.com/WrackedFella/moho/issues/635)) and effects (ENG-F40,
[#636](https://github.com/WrackedFella/moho/issues/636)) all need to move.

## Decision

- A world mesh is uploaded once, as static (drawn at the identity every frame until
  replaced or removed) or movable (drawn only where placed).
- Each frame the game places any live mesh at an affine transform with a material.
  Placements last one frame, including a frame that is skipped.
- The game composes hierarchies (a model's nodes, a socket) and interpolates between ticks;
  the engine receives final world transforms.
- Placements are presentation only and never create or move physics colliders.

## Consequences

- One mesh path serves shadows, SSAO and materials, and all placements of one mesh go to
  the backend as one instanced draw.
- Baked-lighting channels on a movable mesh are not moved lighting, so games fill them
  with neutral values.
- Normals transform by the model matrix. That is exact for rotation, uniform scale and
  axis-aligned faces under non-uniform scale; a general mesh under non-uniform scale needs
  the cofactor normal transform, which ENG-F39 adds with its first such model.
- Skinning, per-camera visibility and per-placement fields are added by the feature that
  needs them.
- Immediate placements, not retained per-instance handles: the game already reads sim state
  every frame to draw ([ADR-0011](0011-simulation-stays-network-ready.md)), and the backend
  re-uploads the instance buffer every frame anyway, so retained instances would add ids,
  hide flags and stale-state bugs for no saved work. "Hide" is not placing it.
