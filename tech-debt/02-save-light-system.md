# TD-02: Fix Broken Save → LightSystem Round-Trip

**Priority:** High — loads silently succeed but produce broken runtime state.

## Problem

`src/main.rs:577-601` contains a "CRITICAL ISSUE" comment block acknowledging a known
structural break:

- The save format persists `VoxelChunk` ECS components (rendered mesh data).
- On load, `LightSystem` is reconstructed from an empty `VoxelGrid` because
  `VoxelChunk` contains vertices/indices, not raw block data.
- The resulting `LightSystem` has no block data to propagate through. Light
  propagation and grid-dependent queries are silently wrong after any load.

This is not a TODO — it is a bug that fires on every Continue or Load operation.

## Acceptance Criteria

1. After a save/load cycle, `LightSystem` holds accurate block data.
2. Alternatively, if the terrain overhaul (paletted chunks) is imminent and the fix
   would be immediately replaced, the code must at least fail visibly rather than
   silently: log an error, disable light propagation, and clearly mark the loaded
   state as degraded rather than pretending it works.
3. The "CRITICAL ISSUE" comment block is removed and replaced with either the fix or
   an explicit `// KNOWN LIMITATION: ...` with a link to this work item.

## Suggested Approaches

**Option A — Fix now:** Extend the save format to include `VoxelGrid` block data
alongside the `VoxelChunk` mesh data. Reconstruct `VoxelGrid` on load before
building `LightSystem`. The block data is small (material IDs per block) relative
to mesh data.

**Option B — Defer with integrity:** If the terrain overhaul will replace the save
format anyway (TD-05 / terrain plan), add a `VoxelGrid::is_empty()` guard in
`load_scene` that skips `LightSystem` construction and logs a clear warning. Remove
the misleading "initializing LightSystem" log that implies success.

Option A is preferred if the terrain overhaul is more than 2–3 weeks away.

## Files

- `src/main.rs` — `load_scene` method (~L525)
- `src/save.rs` — save format
- `moho_core/src/voxel/light_system.rs` — `LightSystem` construction
