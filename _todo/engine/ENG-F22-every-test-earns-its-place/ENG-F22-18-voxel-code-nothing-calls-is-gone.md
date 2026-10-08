# Voxel code nothing calls is gone

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#185](https://github.com/WrackedFella/moho/issues/185)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

`moho_core`'s voxel module carries a block modifier, a chunk state machine, a mesh job queue and a face-culling API that nothing in the game calls. They are deleted with their tests, so the module holds only code the game runs.

## Deliverables

- Deleted, with their tests and re-exports: `BlockModifier` and `ModificationResult`; `ChunkState`, `DirtyFlags`, `GenerationState`, `JobId` (the `state` module); `MeshJobQueue`, `MeshJob`, `CancellationToken`, `create_hybrid_generator` (the `jobs` module); `should_render_face`, `get_visible_faces` and the `FaceDirection` methods only they use; `VoxelGrid::block_data_at` if `BlockModifier` was its only caller.
- `VoxelMutator`, `FaceDirection` itself (used by blocky meshing) and every event type stay.
- Rustdoc that promises these features (for example "face-culled" on `VoxelChunk`) is corrected.

## Acceptance criteria

- [ ] `rg` finds none of the deleted names in the workspace.
- [ ] `just check` passes and the workspace test count drops only by the deleted tests (listed in the PR).
- [ ] Nothing outside `moho_core::voxel` changes except removed imports.

## Tech spec

**Design:** delete the items and the modules that become empty; remove their `pub use` lines from `voxel/mod.rs`. Before deleting each item, confirm with `rg` (method calls included) that no caller outside the deleted set and their tests remains; if one does, keep that item and say so in the PR.

**Out of scope**
- `LightSystem` and its event handlers (deferred until a block emits light).
- Culling faces between adjacent blocky blocks: SG-F3 decides, and would build it fresh.
- Anything in #169 (ENG-F22-02); whichever lands second drops the tests the other already removed.

**Test map:** no new tests; the gate and the test-count diff are the check.

**Gate class:** glue (deletion of unused code; no rule changes).

**Risks:** #140 (ENG-F10-03) moves these files; land before its branch is cut or after it merges. Deleting first makes #140 smaller.
