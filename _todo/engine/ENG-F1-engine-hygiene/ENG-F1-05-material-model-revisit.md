# Material model no longer forces crate placement

**Status:** parked
**Feature:** [ENG-F1](_feature.md)

## Summary

`VoxelGrid` owns a concrete `Vec<MaterialType>`, which pins `MaterialType` in
`moho_core` (see [ADR-0002](../../adr/0002-voxel-and-materials-stay-in-core.md)).
The closed-enum model descends from ported tutorial code and is due a rethink.

## Deliverables

- Decide the material model (closed enum vs. registry/atlas).
- `VoxelGrid` depends on an abstract material registry, so `MaterialType`'s
  crate placement is a free choice.
