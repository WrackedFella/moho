# The physics stack is on current rapier3d

**Feature:** [ENG-F2](_feature.md)
**Issue:** #102

## Summary

ENG-F13 found `rapier3d` four minors behind (0.32 vs 0.36). Its `glamx`
dependency is what pins `glam` at 0.30, and its math stack is what brings in
`paste` ([ENG-F2-04](ENG-F2-04-paste-advisory-cleared.md)). This is the
M1-close upgrade under the upgrade policy.

## Deliverables

- `rapier3d` on the latest release; `glam` on the newest version the stack
  then allows.
- The `glam` watchlist entry and ENG-F2-04's blocker note are updated with
  what the upgrade showed.

## Acceptance criteria

- [ ] `just check` and `just deny` pass.
- [ ] Physics tests pass (character controller, colliders).
- [ ] The workspace builds a single `glam` version.

## Tech spec

**Versions it lands on (checked by resolving and compiling a copy of `dev`, 2026-10-07).** `rapier3d` 0.36.0 brings parry3d 0.31, nalgebra 0.35, simba 0.10 and glamx 0.3.1. **glamx 0.3.1 pins `glam` 0.33**, so the workspace moves to `glam = "0.33"` (0.33.12), not 0.34. One glam version is built. **`paste` leaves the graph**: `cargo tree -i paste` finds no match.

**Folds in [#97 ENG-F2-04, paste advisory](https://github.com/WrackedFella/moho/issues/97).** The upgrade clears `paste`, so this card removes the RUSTSEC-2024-0436 ignore and its PR closes #97.

**Design.**
- `[workspace.dependencies]`: `rapier3d = "0.36"` and `glam = "0.33"`. The `simd-stable` feature no longer exists. Drop it and leave rapier on its defaults: simba's `wide` SIMD is now always on, and `simd8` (8-lane) has no measured need.
- `moho_physics::world::PhysicsWorld`: `PhysicsPipeline::step` and `ColliderSet::remove` now take `&mut SoftBodySet`. Add a private `soft_body_set: SoftBodySet` field (`SoftBodySet::new()`) and pass it. In the probe this was the only physics break, and the 4 `moho_physics` tests passed.
- glam 0.33 deprecates `Mat4::look_at_rh`, `perspective_rh` and `orthographic_rh`, and `-D warnings` turns that into a gate failure. Replace them with `glam::camera::rh::view::look_at_mat4`, `glam::camera::rh::proj::directx::perspective` and `…::orthographic`. The signatures and the [0, 1] depth range are the same. There are 12 sites: `src/app/camera.rs`, `moho_game::controller`, `moho_renderer::shadow`.
- `deny.toml`: delete the RUSTSEC-2024-0436 entry. Feature watchlist: the glam entry becomes "0.34 blocked on glamx (rapier3d 0.36 pins glam 0.33)", and the paste entry is removed.

**Out of scope.**
- glam 0.34. It is blocked upstream and goes back on the watchlist.
- Soft bodies, `parallel`, `simd8` and `enhanced-determinism` (no consumer). New physics features belong to [ENG-F15](https://github.com/WrackedFella/moho/blob/dev/_todo/engine/ENG-F15-physics-queries-and-bodies/_feature.md).
- Removing `moho_physics`'s unused `moho_core` dependency ([#96](https://github.com/WrackedFella/moho/issues/96)).

**Test map.**
| Criterion | Proof | Gate class |
|---|---|---|
| `just check`, `just deny` pass | the two commands | glue |
| Physics tests pass | existing `moho_physics` tests (character controller, colliders) | glue |
| One glam version | `cargo tree --workspace -e normal -d` shows no `glam` duplicate | glue |
| `paste` gone (from #97) | `cargo tree --workspace --target all -e all -i paste` reports no match; ignore removed | glue |
| Camera and shadow matrices unchanged | `moho_game::controller::tests::view_projection_matches_previous_glam_output` (compares against literal matrices captured on glam 0.30) | glue |

**Gate class:** glue.

**Risks.**
- Solver changes across four rapier minors may change how the character feels (step-up, snap-to-ground). Verification needs a human playtest.
- rapier3d has a single owner on crates.io (audit note); no new risk.

## Verification

- The player walks, jumps and collides with terrain as before.
