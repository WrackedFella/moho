# Consolidate VoxelMesh/VoxelChunk/hybrid mesh result

**Feature:** [ENG-F4](_feature.md)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Three structs each independently carry `vertices`/`normals`/`ambient_occlusion`/
`geometry_type`/`light_level`/`indices`.

## Deliverables

- Pull the shared fields into one `MeshBuffers` struct with a single
  constructor, used by all three.
