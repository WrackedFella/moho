# TD-13: High-Value Test Coverage

**Priority:** Medium — focus only on tests that prevent real, hard-to-diagnose bugs,
not tests for coverage's sake. Each test below has a specific, justified reason to
exist.

## Principle

Don't test what is already verified by the type system or compiler. Don't add tests
that merely assert a constructor returns a struct. Focus on:
- **Regression prevention** for paths that have already broken or nearly broken.
- **Contract enforcement** for APIs with non-obvious invariants.
- **Integration seams** where two systems hand off data and format assumptions can
  silently diverge.

---

## Test 1: Save/Load Round-Trip (highest value)

**Why:** The save/load path has a known active bug (TD-02) and any format version
bump silently breaks all existing saves. There is currently no automated check that
a written save can be read back. This is the one test that would have caught the
LightSystem break before it shipped.

**What to test:**
- Write a `WorldSpec` + minimal scene (empty `World`, one `LightDesc`) via `save.rs`.
- Read it back with `read_scene_and_metadata`.
- Assert `WorldSpec` fields round-trip exactly (name, seed, size, time_of_day).
- Assert light descriptors round-trip (position, color, intensity, range).
- Assert camera data (position, yaw, pitch) round-trips within float epsilon.

**Where:** `tests/save_load_roundtrip.rs`

---

## Test 2: Terrain Seed Determinism (prerequisite for TD-01)

**Why:** This test is the acceptance criterion for TD-01 and has to be written
regardless. Without it, the seed determinism fix has no guard against regression.

**What to test:**
- Generate the same world twice with `seed = 42`. Assert block-for-block equality
  of the resulting `VoxelGrid` (same positions, same material IDs).
- Generate with `seed = 42` and `seed = 43`. Assert they differ (at least one block
  position or material differs).

**Where:** `moho_core/src/scene_builders.rs` (doc test or `#[cfg(test)]` module)

---

## Test 3: BlockModifier Invariants

**Why:** `BlockModifier` (638 lines, `moho_core/src/voxel/modification.rs`) manages
dirty-flag propagation across chunk boundaries and publishes `WorldEvent`s. These
invariants are easy to break silently — a missed dirty flag means a chunk that
should remesh doesn't. This has happened before (existing comment in the file hints
at boundary edge cases).

**What to test:**
- `set_block` at a chunk boundary position marks *both* adjacent chunks dirty.
- `remove_block` on an empty position returns `ModificationResult::BlockNotFound`
  (not a panic).
- Consecutive `set_block` + `remove_block` at the same position leaves the grid in
  the original state.

**Where:** `moho_core/src/voxel/modification.rs` (`#[cfg(test)]` module)

---

## Test 4: Voxel Mesh Generator Contracts

**Why:** `BlockyMeshGenerator` and `MarchingCubes` are the mesh paths for all
terrain. An off-by-one in index generation produces out-of-bounds GPU reads that
are silent on some hardware and crash on others. 926 lines of marching-cubes table
with no test is a real risk for the terrain overhaul.

**What to test (minimal, not exhaustive):**
- For `BlockyMeshGenerator`: a chunk with one solid block at (0,0,0) produces non-empty
  vertices and indices. All indices are `< vertices.len()` (no out-of-bounds refs).
- For `MarchingCubes`: same check. All indices in range.
- For both: an all-air chunk produces empty output (no degenerate draws).

**Where:** `moho_core/src/voxel/mesh/blocky.rs`, `moho_core/src/voxel/mesh/marching_cubes.rs`

---

## What Is Explicitly Not Worth Testing

- `EguiAdapter` state transitions — these are already exercised by integration
  testing the running app and the state transitions are covered in `moho_types`.
  Mocking egui for unit tests is expensive and fragile.
- Renderer draw calls — GPU state requires a real device. A null-backend test
  is a significant engineering investment for marginal gain right now.
- `PhysicsWorld` beyond the 4 existing smoke tests — rapier3d is well-tested upstream;
  trust it.
- `EventBus` — already at 27 tests. Don't add more unless a specific failure mode
  is identified.

---

## Files

- `tests/save_load_roundtrip.rs` — new integration test
- `moho_core/src/scene_builders.rs` — seed determinism test (also in TD-01)
- `moho_core/src/voxel/modification.rs` — `BlockModifier` tests
- `moho_core/src/voxel/mesh/blocky.rs` — mesh index validity
- `moho_core/src/voxel/mesh/marching_cubes.rs` — mesh index validity
