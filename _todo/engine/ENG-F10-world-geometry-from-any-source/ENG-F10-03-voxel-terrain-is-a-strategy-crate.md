# Voxel terrain builds as a strategy-line crate

**Feature:** [ENG-F10](_feature.md)
**Issue:** [#140](https://github.com/WrackedFella/moho/issues/140)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

The voxel terrain engine leaves `moho_core` for a new strategy-line,
platform-free crate, `moho_voxel`
([ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md)). The move is
mechanical: no behaviour changes, and no test is dropped.

## Deliverables

- `moho_voxel` is a workspace member, in the strategy and domain rows of
  `scripts/layering.txt`, and depends on engine crates only.
- It holds everything in `moho_core::voxel` (grid, meshing, LOD, lighting, chunk
  store, streaming, jobs, modification) and the voxel integration tests.
- `moho_core` has no `voxel` module; `moho_game` and the binary import from
  `moho_voxel`.

## Acceptance criteria

- [ ] `moho_core::voxel` no longer exists, and `moho_voxel` exports what it exported.
- [ ] `moho_core`'s `chunk_remesh`, `lighting`, `mesh_alignment` and `voxel_system`
      tests run in `moho_voxel`. The workspace test count equals `dev`'s, and the PR
      quotes both counts.
- [ ] `just layering` passes with `moho_voxel` in the strategy and domain rows, and
      no engine crate depends on it.
- [ ] `just check` passes.

## Tech spec

**Design.**
- `git mv moho_core/src/voxel` into `moho_voxel/src/`; `voxel/mod.rs` becomes
  `lib.rs` with the same `mod`/`pub use` lines, so `moho_core::voxel::X` becomes
  `moho_voxel::X` (and `moho_voxel::streaming`). Keep moved files byte-identical
  apart from paths so git records renames and the diff stays reviewable; that
  also keeps `just mutants`' diff to the edited lines.
- Inside the moved code, `crate::voxel::` becomes `crate::`. `crate::events` and
  `crate::materials` become `moho_core::events` and `moho_core::materials`;
  ENG-F10-04 moves those types.
- `moho_voxel/Cargo.toml`: `moho_core`, plus only the dependencies the moved code
  uses (from `[workspace.dependencies]`), and `proptest` as a dev-dependency if the
  moved tests use it. Lints `workspace = true`.
- Shared files this card must change: root `Cargo.toml` (member and the binary's
  path dependency) and `scripts/layering.txt` (strategy and domain rows), as
  ADR-0012 does for new crates. `moho_game/Cargo.toml` adds `moho_voxel`.
- Doc examples and comment refs follow the new paths (`just check` runs both).
- GitNexus: this is a module move, not a symbol rename. Run `impact` on the
  public items of `moho_core::voxel` before the move and `detect_changes` before
  commit; refresh the index after.

**Out of scope.**
- Moving `MaterialType`, `WorldEvent` or `BlockChangeReason` (ENG-F10-04).
- Any refactor, rename or API change inside the voxel code.
- `moho_core` dependencies left unused by the move: list them in the PR;
  [#96 ENG-F2-03](https://github.com/WrackedFella/moho/issues/96) owns removing
  unused declarations.
- ENG-F1-05's material rethink; [#105 ENG-F2-10](https://github.com/WrackedFella/moho/issues/105)'s noise replacement.

**Test map.**
| Criterion | Proof | Gate class |
|---|---|---|
| Module gone, exports kept | the workspace builds against `moho_voxel::` paths (`just check`) | glue |
| Tests moved, none dropped | `cargo nextest list --workspace` count on `dev` and on the branch, quoted in the PR | glue |
| Layering | `just layering` | glue |

**Gate class:** glue.

**Branch.** `feature/ENG-F10-voxel-extraction` off `dev`, the integration branch
for this card and ENG-F10-04; the card branch PRs into it.

**Risks.**
- Needs ENG-F10-01 merged first: until then `moho_renderer` names `VoxelChunk`,
  and the move would make the engine depend on the strategy line.
- Every open branch that edits `moho_core/src/voxel/` conflicts. The roadmap lands
  the ENG-F22 voxel cards first ([ENG-F22-02](https://github.com/WrackedFella/moho/issues/169), then
  [ENG-F22-06](https://github.com/WrackedFella/moho/issues/173) and [ENG-F22-07](https://github.com/WrackedFella/moho/issues/174)); start only when no open
  PR edits `moho_core/src/voxel/`, announce the move to the Strategy lane before it
  starts, and hold other `voxel/` edits until it merges. Strategy cards don't gate
  it: one that starts later targets `moho_voxel`.

## Verification

None beyond the tests; no behaviour changes.
