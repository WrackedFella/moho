# TD-05: Trim moho_core — Remove Legacy Code, Extract Domain Crates

**Priority:** Medium — directly gates the "reusable crates" goal and keeps adding
surface area to the most-depended-on crate.

## Problem

`moho_core` is the heaviest dependency in the workspace (11,466 lines) and its
public API mixes three very different concerns:

1. **Generic engine primitives** — `EventBus`, `Event`, `GameClock`, `input`
2. **Voxel-specific game logic** — `voxel::*` (chunks, grids, light, mesh gen,
   marching cubes, modification API)
3. **Ray-tracing leftovers** — `schlick`, `refract`, `reflect`,
   `random_in_unit_sphere`, `random_in_unit_disk`, `Hittable`, `Scatterable`,
   `HittableRecord`, `Ray`, legacy `camera` module, `MaterialType` (Lambertian/Metal/
   Dielectric) from the original ray-tracer heritage

A crate named `moho_core` that exports marching-cubes and Snell's law alongside the
event bus is not usable as a "generic engine primitive" by other projects. The voxel
system and the ray-tracer residue are game/demo-specific.

## Acceptance Criteria

### Phase A — Ray-tracer removal (do first, cheap)

- Delete or move to a `moho_core::legacy` feature-gated module:
  `schlick`, `refract`, `reflect`, `random_in_unit_sphere`, `random_in_unit_disk`,
  `Hittable`, `Scatterable`, `ScatterRecord`, `HittableRecord`, `Ray`,
  `moho_core::camera` (legacy ray-tracing camera).
- Audit all call sites. `scene_builders.rs` uses `MaterialType` for legacy sphere
  scenes; those scenes can be deleted or kept behind a `cfg(test)` / feature gate.
- `moho_core::materials::MaterialType` — if used only by dead sphere/ray scenes,
  remove. If used for material identity on the grid (currently it is for
  `MaterialRegistry`), replace with a simpler struct or merge with `MaterialGpu`.

### Phase B — Voxel extraction (coordinate with terrain overhaul)

- Create a new `moho_voxel` crate.
- Move `moho_core::voxel::*`, `moho_core::scene_builders`, and
  `moho_core::actors::Cube/Sphere` (voxel-mesh specific) there.
- `moho_voxel` depends on `moho_core` (for `EventBus`, etc.) but not vice versa.
- Update all `moho_core::voxel` import paths in the binary and renderer.

This is a larger change and should be timed alongside or after the terrain overhaul
(which rewrites the storage layer anyway), to avoid doing the extraction twice.

### Phase B.5 — Prune stub event categories

While trimming `moho_core`, prune the event category enums that are entirely
placeholder. Currently `NetworkEvent` has no variants and every arm of its enum is
effectively dead. Several `PhysicsEvent` and `WorldEvent` variants have no subscribers
and no publishers anywhere in the codebase.

- Remove or collapse empty/unused event variants. Do not pre-define categories for
  systems that don't exist yet (multiplayer, advanced physics) — add them when needed.
- A 10-category event taxonomy is fine; a 10-category taxonomy where 3–4 categories
  are pure stubs bloats the public API and makes `--all-features` grep results noisy.

This can be a one-pass cleanup done at the same time as Phase A.

### Phase C — Controller/clock extraction (optional, lower priority)

- Consider `moho_gameplay` or `moho_controller` for `PlayerController`,
  `ControllerInput`, `GameClock`, and `SimulationController`.
- These are highly game-specific (FPS + isometric + day/night) and not generic
  engine primitives.

## Files

- `moho_core/src/lib.rs` — primary export surface
- `moho_core/src/camera.rs` — delete
- `moho_core/src/materials.rs` — audit usage, possibly delete or simplify
- `moho_core/src/scene_builders.rs` — audit or move
- `moho_core/src/voxel/` — move to `moho_voxel` in Phase B
- `moho_core/Cargo.toml` — dependencies may shrink significantly after Phase A
