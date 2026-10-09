# Save file format

**Source:** `moho_core/src/persist.rs` (envelope), `src/save.rs` (world and chunk files),
`moho_game/src/scene_persistence.rs` (scene payload), `moho_voxel/src/grid/paletted.rs`
(chunk payload). Rationale: [ADR-0006](../../_todo/adr/0006-save-format-contract.md).

Every persisted file is one envelope around a [postcard](https://docs.rs/postcard) payload.
Read and write only through `persist::encode` / `persist::decode`.

## Envelope

Little-endian, 19-byte header followed by the payload.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `MOHO` |
| 4 | 1 | `FileKind`: 1 World, 2 Scene, 3 Chunk |
| 5 | 2 | format version, per kind (`FileKind::current_version`); 0 is invalid |
| 7 | 8 | payload length |
| 15 | 4 | CRC-32 of the payload |
| 19 | n | postcard payload; must be consumed exactly |

`decode` checks in this order: magic and kind byte (`NotASaveFile`), version
(`UnsupportedFormat` for 0 or newer than the build), expected kind (`WrongKind`), then
length, CRC and trailing bytes (`Corrupt`). The version lives only in the envelope; payload
structs carry no version field. Older versions are not read unless a migration is added
for that kind.

## Files

| Kind | Path | Payload |
|---|---|---|
| World | caller-chosen path passed to `write_scene_with_metadata` | `WorldFile`: `WorldSpec`, the Scene file as embedded bytes, block records |
| Scene | embedded in the World payload, or standalone via `scene_persistence::save_to_file` | `SceneDesc`: spheres, cubes, voxel chunk meshes, camera, lights |
| Chunk | `saves/<world>/chunks/<cx>_<cy>_<cz>.bin` | `ChunkSnapshot`: palette, 4096 indices, resources |

Invariants:

- Chunk resources are sorted by index before encoding, so equal chunks encode to equal bytes.
- A chunk decodes only with 4096 indices, a non-empty palette and every index inside it.
- Loading a scene decodes fully before touching `SceneEntities`; a failed load leaves the
  grid or entities unchanged.
- World and chunk files are written to a temporary file, synced, then renamed over the target.
- A missing chunk file is not an error: the chunk is regenerated from terrain.
