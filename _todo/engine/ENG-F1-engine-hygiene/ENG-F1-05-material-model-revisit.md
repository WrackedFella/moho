# Material model no longer forces crate placement

**Status:** parked
**Feature:** [ENG-F1](_feature.md)

## Summary

`VoxelGrid` owns a concrete `Vec<MaterialType>`, which pins `MaterialType` in
`moho_core` until ENG-F10 moves both to `moho_voxel` ([ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md)).
The closed-enum model descends from ported tutorial code and is due a rethink. The
rethink no longer decides any crate's placement.

## Deliverables

- Decide the material model (closed enum vs. registry/atlas).
- `VoxelGrid` depends on an abstract material registry, so `MaterialType`'s
  crate placement is a free choice.
