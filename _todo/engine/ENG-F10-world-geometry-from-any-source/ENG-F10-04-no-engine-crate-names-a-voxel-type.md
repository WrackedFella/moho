# No engine crate names a voxel type, and the layering check covers tests

**Feature:** [ENG-F10](_feature.md)

## Summary

`MaterialType` and the block-level world events move to `moho_voxel`,
completing [ADR-0010](../../adr/0010-world-geometry-is-a-mesh-contract.md)'s
split. The layering check also reads dev-dependencies, so an engine crate's tests
can't reach a game crate either. Closes ENG-F10's remaining exit criteria.

## Deliverables

- `MaterialType` (with its `RenderMaterial` impl) lives in `moho_voxel`.
- `WorldEvent` and `BlockChangeReason` live in `moho_voxel`; `moho_core::events`
  keeps the bus and engine-wide events.
- The layering check includes dev-dependencies.
- `CLAUDE.md`'s architecture table and domain-logic paths name `moho_voxel`.

## Acceptance criteria

- [ ] No engine-line crate's source or tests name `VoxelChunk`, `ChunkStore`,
      `MaterialType`, `WorldEvent` or `BlockChangeReason`, and none depends on
      `moho_voxel` or `moho_game` as a normal, build or dev dependency.
- [ ] Adding a game crate as an engine crate's dev-dependency fails `just layering`
      (shown once in the PR, then reverted).
- [ ] A save written by `dev` before this change loads, and the existing save tests
      pass.
- [ ] `just check` passes.

## Tech spec

**Design.**
- `moho_core::materials` → `moho_voxel::materials`; `moho_core::events::types::world`
  → `moho_voxel::events`, re-exported at the crate root. `EventBus` is generic over
  `Event`, so the bus needs no change. Update imports in
  `moho_voxel`, `moho_game` and the binary.
- `scripts/check-layering.sh`: per-crate edges use `-e normal,build,dev`; the
  workspace listing stays `-e normal`. The rules don't change, so no new fixture.
- Saves store `moho_game`'s `MaterialDesc` mirror, not `MaterialType`, so the
  save format doesn't change; the old-save check confirms it.
- `CLAUDE.md`: architecture table gains `moho_voxel` and drops voxels and
  materials from `moho_core`; the "until ENG-F10 moves it" sentence goes; domain
  paths add `moho_voxel`; the `ChunkStore` reference names `moho_voxel`.

**Out of scope.**
- Renaming `MaterialType` or splitting actor materials from terrain materials
  (ENG-F1-05).
- Line-specific events other than `WorldEvent` and `BlockChangeReason`.
- Any change to `moho_core` beyond removing what moves.

**Test map.**
| Criterion | Proof | Gate class |
|---|---|---|
| No voxel names or deps in the engine | `just layering` with dev edges; `rg -w 'VoxelChunk\|ChunkStore\|MaterialType\|WorldEvent\|BlockChangeReason'` over engine crates, quoted in the PR | glue |
| Dev-dep violation rejected | one-off run quoted in the PR | glue |
| Old save loads | existing `moho_game/tests/save_load.rs`; manual load of a `dev` save (Verification) | glue |
| Gate | `just check` | glue |

**Gate class:** glue.

**Branch.** `feature/ENG-F10-voxel-extraction`, after ENG-F10-03. The feature
PR into `dev` lists `Closes` for ENG-F10-03 and ENG-F10-04.

**Risks.**
- Including dev edges brings dev-dependencies' transitive crates into the
  platform rule for domain crates; today none pulls `winit`, `wgpu` or `egui`.
- Conflicts with [#94 ENG-F2-01, save envelope](https://github.com/WrackedFella/moho/issues/94)
  in `moho_game`'s persistence imports; whichever lands second rebases.

## Verification

ENG-F10's manual check, on the integration branch before its PR into `dev`:
- Mining updates the terrain and the collision.
- Chunks stream in and out; lighting looks as it does on `dev`.
- A save written by `dev` loads.
