# 0024 — Models are glTF data the game places node by node

**Status:** Accepted on merge of #651; amended by #654

## Context

ENG-F39 ([#635](https://github.com/WrackedFella/moho/issues/635)) brings models, sockets and
skeletal animation. [ADR-0010](0010-world-geometry-is-a-mesh-contract.md) keeps the engine
agnostic of where geometry comes from; [ADR-0025](0025-world-meshes-are-placed-per-frame.md)
places world meshes per frame at final transforms the game composes.

## Decision

1. Model import lives in `moho_level` with level import: one glTF reader, one buffer rule set,
   one `LevelError` naming the file.
2. A `Model` is immutable plain data: meshes in node space, a node tree with names, local and
   rest transforms, and materials. It holds no GPU handles and no per-instance state.
3. The game owns mesh ids and material indices: it uploads each model mesh once as movable and
   places them each frame from `Model::placements`. The engine keeps no model instances.
4. Posing is an input to placement, never a mutation of the model: sockets (#653) and skins and
   clips (#654) produce transforms from the same `Model`, so a pose the sim computes (#655) is
   plain data.
5. Normals transform by the cofactor of the model matrix, so any affine placement lights
   correctly.
6. Committed models are CC0 or made for the repo, with source and licence recorded
   ([ADR-0007](0007-third-party-licence-policy.md)); other art is for local checks only.

## Consequences

- Each game repeats the upload-and-place glue until a third copy argues for a helper.
- A glTF mesh shared by several nodes is converted once per node.
- `ModelMaterial` reserves a `MaterialKey` variant.
- A mirrored root transform draws inside-out (back-face culling), so games place with
  positive-determinant roots.
