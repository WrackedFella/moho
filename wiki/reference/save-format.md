# Save file format

**Source:** `moho_core/src/persist.rs` (envelope), `flagstone/src/save.rs` (world and chunk files, `WORLD_FILE`),
`flagstone/src/app/autosave.rs` (autosave file and schedule),
`flagstone_game/src/scene_persistence.rs` (scene payload, `SCENE_FILE`),
`flagstone_voxel/src/grid/paletted.rs` (chunk payload, `CHUNK_FILE`). Rationale:
[ADR-0006](../../_todo/adr/0006-save-format-contract.md),
[ADR-0016](../../_todo/adr/0016-game-lines-own-their-save-kinds.md).

Every persisted file is one envelope around a [postcard](https://docs.rs/postcard) payload.
Read and write only through `persist::encode` / `persist::decode`.

## Envelope

Little-endian, 19-byte header followed by the payload.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `MOHO` |
| 4 | 1 | `FileKind::tag`: 1 World, 2 Scene, 3 Chunk |
| 5 | 2 | format version, per kind (`FileKind::version`); 0 is invalid |
| 7 | 8 | payload length |
| 15 | 4 | CRC-32 of the payload |
| 19 | n | postcard payload; must be consumed exactly |

`FileKind` is a `{ tag, version }` pair declared by the crate that owns the payload, not
by `moho_core`: `WORLD_FILE` in `flagstone/src/save.rs`, `SCENE_FILE` in `flagstone_game`, `CHUNK_FILE`
in `flagstone_voxel`. Errors report the raw tag byte.

`decode` checks in this order: magic (`NotASaveFile`), version 0 (`UnsupportedFormat`,
so pre-envelope saves keep that error), tag against the caller's kind (`WrongKind`),
version newer than the caller's kind (`UnsupportedFormat`), then length, CRC and trailing
bytes (`Corrupt`). The version lives only in the envelope; payload
structs carry no version field. Older versions are not read unless a migration is added
for that kind.

## Files

| Kind | Path | Payload |
|---|---|---|
| World | caller-chosen path passed to `write_scene_with_metadata`; autosave writes `<saves_dir>/scene.bin` | `WorldFile`: `WorldSpec`, the Scene file as embedded bytes, block records |
| Scene | embedded in the World payload, or standalone via `scene_persistence::save_to_file` | `SceneDesc`: spheres, cubes, voxel chunk meshes, camera, lights |
| Chunk | `<saves_dir>/<world>/chunks/<cx>_<cy>_<cz>.bin` | `ChunkSnapshot`: palette, 4096 indices, resources |

Each payload's encoding is pinned by a test (`PINNED_WORLD`, `PINNED_SCENE`,
`PINNED_CHUNK`). Changing a payload struct fails it; bump the kind's `version` and update
the pinned bytes.

Invariants:

- Chunk resources are sorted by index before encoding, so equal chunks encode to equal bytes.
- A chunk decodes only with 4096 indices, a non-empty palette and every index inside it.
- Loading a scene decodes fully before touching `SceneEntities`; a failed load leaves the
  grid or entities unchanged.
- World and chunk files are written to a temporary file, synced, then renamed over the target.
- A missing chunk file is not an error: the chunk is regenerated from terrain.
- `saves_dir` is `App::saves_dir` (default `saves`); the chunk functions in `save.rs` take it as an
  argument, and `ChunkStreamer` holds the one it was built with, so world and chunk files share a root.

## Autosave

`app::autosave::auto_save_on_shutdown` writes the World file (scene, `WorldSpec` with the current
time of day, block records) to `<saves_dir>/scene.bin`. It runs on exit, on Exit to main
menu and periodically. With no world loaded (`App::generation.last_spec` is `None`) it writes nothing.

`AutosaveSchedule` decides the periodic runs, counted in simulation ticks:

- It advances once per tick, only in `GameState::Playing`; a paused game, a menu or the
  console does not count toward the interval.
- A world is due when it has been edited since the last save and `DEFAULT_AUTOSAVE_INTERVAL`
  (120 s of play, `App::autosave_interval`) has passed. An idle world is never due.
- `process_world_events` marks it edited on `BlockPlaced`, `BlockRemoved` and
  `BlocksBatchModified`. Chunk streaming events (`ChunkMeshDirty`) do not.
- A successful save resets the interval and clears the edited flag; a failed save resets the
  interval only, so the world stays due and is retried one interval later.
- Generating or loading a world restarts the schedule (`App::restart_autosave`).

`App::frame` runs the save when due ([frame loop](../architecture/frame-loop.md#4-frame)).
