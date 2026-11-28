# 0001 — Renderer sees game types only through `moho_render_api`

**Status:** Accepted

## Context

`moho_renderer` once imported game types directly (actors, materials), so the
engine could not be reused without the game. Engine crates must stay usable
by a second game (the FPS line) and splittable into their own repo
([ENG-F5](https://github.com/WrackedFella/moho/issues/232)).

## Decision

- `moho_render_api` is the contract between engine and game: the
  `Renderable` and `RenderMaterial` traits plus plain GPU-layout data
  (`InstanceGpu`, `MaterialGpu`, `MaterialKey`, `LightDesc`, `CameraDesc`).
  It depends on neither `wgpu` nor `legion`.
- Game types implement those traits; the renderer never names a game type.
- Engine crates (`moho_renderer`, `moho_audio`, `moho_core`) have no
  production dependency on `moho_game`. A `moho_game` dev-dependency for
  integration tests is allowed.
- Frames are driven through `begin_frame` / `enqueue_draw` / `submit_frame`.

## Consequences

- `cargo tree -p moho_renderer -e normal,build` must show no `moho_game`
  edge; a new edge is a boundary violation, not a shortcut.
- New renderable game content means implementing a trait, not editing the
  renderer.
- `VoxelChunk` is the deliberate exception (see [ADR-0002](0002-voxel-and-materials-stay-in-core.md)).
