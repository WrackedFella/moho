# TD-01: Fix Terrain Seed Determinism

**Priority:** High — affects correctness of a user-visible feature (reproducible worlds).

## Problem

`moho_core/src/scene_builders.rs` calls `rand::rng().random()` during terrain generation
(specifically in `determine_resource_id` and related callers). `rng()` creates a
thread-local RNG seeded from entropy, not from `WorldSpec::seed`. The result: the
same seed produces different ore layouts and resource placements on every run. The
`WorldSpec::seed` field is UI-plumbed but largely decorative.

This will only get harder to fix once gameplay systems start reading resource data.

## Acceptance Criteria

1. Terrain generation is a pure function of `(seed, config, position)` — no calls to
   entropy-seeded RNGs anywhere in the generation path.
2. A regression test generates the same world twice with the same seed and asserts
   block-for-block equality of the resulting `VoxelGrid`.
3. A second test confirms different seeds produce different outputs.

## Suggested Approach

- Replace `rng().random()` calls with a positional hash: something like
  `hash3(x, y, z, seed) % 100 < threshold` using a fast mix (e.g. PCG-style or
  a wrapping-multiply chain — no external dependency needed).
- Audit all sites in `scene_builders.rs` that call `rng()` and apply the same fix.
- Add the regression tests under `moho_core` (unit or doc-test).

## Files

- `moho_core/src/scene_builders.rs` — primary fix site
- `moho_core/src/voxel/grid.rs` — check for stray rng calls
- `moho_core/tests/` or `moho_core/src/scene_builders.rs` — regression tests
