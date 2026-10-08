# Saves use the ADR-0006 envelope and encoding

**Note:** Decided by [ADR-0006](../../adr/0006-save-format-contract.md); must land before [SG-F3](../../strategy-game/SG-F3-buildings-and-construction/_feature.md)
**Feature:** [ENG-F2](_feature.md)
**Issue:** [#94](https://github.com/WrackedFella/moho/issues/94)
**Status:** unknown
**Gate class:** domain
**Labels:** line:engine

## Summary

bincode is unmaintained, and its 3.0.0 release contains nothing but a
`compile_error!`, so the original "bincode 2 → 3" plan is impossible.
Following [ADR-0006](../../adr/0006-save-format-contract.md), all persisted
files move to one versioned envelope (magic, kind, u16 version, length,
CRC32) around a `postcard` payload derived with `serde`. Old saves break
once; there is no migration shim before v1.0.

## Deliverables

- One envelope implementation used by the world save, the scene file and
  chunk files. Bad magic, an unknown version or a checksum mismatch returns a
  typed error.
- Domain types derive `Serialize`/`Deserialize`. No `bincode::{Encode,
  Decode}` derives remain, including in `moho_render_api`.
- `bincode` is gone from the workspace, along with its `deny.toml` ignore.
- An `insta` snapshot test pins the envelope bytes for a small fixture.
  Round-trip tests cover each file kind, and a load of a pre-migration save
  returns the typed "unsupported format" error.

## Acceptance criteria

```gherkin
Scenario Outline: Each persisted file kind round-trips
  Given a <kind> written by the game
  When it is loaded
  Then the loaded value equals the one written
  Examples:
    | kind       |
    | world save |
    | scene file |
    | chunk file |

Scenario Outline: A damaged or foreign file is rejected with a typed error
  Given a <kind> whose <defect>
  When it is loaded
  Then loading returns the <error> error
  And no partial state is applied
  Examples:
    | kind       | defect                      | error              |
    | world save | magic bytes are wrong        | not a save file    |
    | world save | format version is unknown    | unsupported format |
    | chunk file | checksum does not match      | corrupt            |

Scenario: A pre-migration save is refused cleanly
  Given a world save written before this change
  When it is loaded
  Then loading returns the unsupported-format error
```

- [ ] A snapshot test pins the envelope bytes for a small fixture.
- [ ] No workspace manifest declares `bincode`; its `deny.toml` ignore is gone.

## Tech spec

**Design.**
- New module `moho_core::persist`, the engine half of [ADR-0006](https://github.com/WrackedFella/moho/blob/dev/_todo/adr/0006-save-format-contract.md):
  - `enum FileKind { World = 1, Scene = 2, Chunk = 3 }` (`u8` on disk). `FileKind::current_version(self) -> u16` returns 1 for every kind for now.
  - Header, little-endian, 19 bytes: `b"MOHO"` | kind `u8` | version `u16` | payload length `u64` | CRC32 of the payload `u32`, then the payload. The magic stays `MOHO` so that a pre-migration world save (old layout: `MOHO` + `u32` version 1 or 2) reads as version 0 and is reported as unsupported, not as a foreign file.
  - `pub fn encode<T: Serialize>(kind: FileKind, value: &T) -> Result<Vec<u8>, PersistError>` and `pub fn decode<T: DeserializeOwned>(expected: FileKind, bytes: &[u8]) -> Result<T, PersistError>`. Both use `postcard` 1.x for the payload.
  - `decode` checks in this order: length ≥ header, else `NotASaveFile`; magic, else `NotASaveFile`; known kind byte, else `NotASaveFile`; version is 1..=current, else `UnsupportedFormat { kind, version }`; kind is `expected`, else `WrongKind { expected, found }`; declared length matches the bytes present, else `Corrupt`; CRC matches, else `Corrupt`; postcard decode succeeds with no trailing bytes, else `Corrupt`.
  - `#[non_exhaustive] pub enum PersistError` (`thiserror`): `NotASaveFile`, `UnsupportedFormat { kind: FileKind, version: u16 }`, `WrongKind { expected: FileKind, found: FileKind }`, `Corrupt`, `Encode(String)`, `Io(#[from] std::io::Error)`. postcard's error type is wrapped as a message, not exposed.
- Dependencies: `postcard = { version = "1", default-features = false, features = ["alloc"] }` and `crc32fast = "1"` are added to `moho_core` through `[workspace.dependencies]`. Both are MIT/Apache-2.0. `crc32fast` is already in the lockfile (pulled in by png/flate2); postcard adds `cobs` and `embedded-io`. `serde` is added to `moho_render_api` and to the root crate. `bincode` leaves every manifest, and its `deny.toml` ignore goes too.
- Chunk (`moho_core::voxel::grid::paletted`): `ChunkSnapshot` derives serde. `to_bytes` returns `persist::encode(FileKind::Chunk, ..)` with `resources` **sorted by local index**, because HashMap order would make the bytes nondeterministic. `from_bytes -> Result<Self, PersistError>`, and a wrong index count is `Corrupt`. `VoxelGrid::deserialize_chunk_into -> Result<(), PersistError>` leaves the grid untouched on error.
- Scene (`moho_game::scene_persistence`): the `*Desc` types derive serde. `SceneDesc` loses its `version` field because the envelope carries it. `SceneDescLegacy` and the v1/v2 fallback are deleted (no shim, per ADR-0006). `encode_to_bytes` returns a `Scene` envelope, and `save_to_file` writes those bytes, so the scene file and the scene section of a world save are the same format. `load_from_bytes`/`load_from_file` return `Result<_, PersistError>` and decode fully before touching `entities`.
- World save (`src/save.rs`): the payload is `WorldFile { spec: WorldSpec, scene: Vec<u8>, blocks: Vec<BlockRecord> }`, where `scene` holds an enveloped Scene. `write_scene_with_metadata` keeps its tmp+rename. `read_scene_and_metadata -> Result<SavePayload, PersistError>`. `write_chunk_file` is unchanged (it writes the enveloped bytes as given). `read_chunk_file -> Result<Option<Vec<u8>>, PersistError>`: `Ok(None)` only for `NotFound`.
- Streaming (`ChunkStreamer::load_chunk`): a chunk file that fails to decode is logged at `error` with the position and the error, then regenerated from seed. That is today's behaviour, now announced at `error` instead of `warn`; see Risks.
- Domain derives: `bincode::{Encode, Decode}` come off `WorldSpec`, `GameClock` state, the biome types, `BlockRecord`, `LightDesc` and `CameraDesc`. Their serde derives stay or are added.

**Out of scope.**
- Migration or reading of old saves; the version N−1 reader starts at v1.0 (ADR-0006).
- Changing what is saved: the [SG-F2-03 mining-persistence bug](https://github.com/WrackedFella/moho/blob/dev/_todo/strategy-game/SG-F2-known-bugs/SG-F2-03-mining-not-persisted.md), a save policy, and new persisted types (SG-F3).
- Moving `src/save.rs` or chunk files into an engine crate, compression, async I/O, and prefs (stays INI).
- Fixing or adding `log` call sites beyond the ones this change touches ([#98](https://github.com/WrackedFella/moho/issues/98)).

**Test map.** Domain gate: the tests below are written first and reviewed by a human before implementation.
| Scenario / row | Test |
|---|---|
| World save round-trips | `moho::save::tests::world_save_round_trips` (replaces `write_and_read_envelope_roundtrip`, `empty_blocks_roundtrip`) |
| Scene file round-trips | `moho_game::scene_persistence::tests::scene_file_round_trips` (plus the existing `moho_game/tests/save_load.rs`, ported) |
| Chunk file round-trips | `moho_core::voxel::grid::paletted::tests::chunk_round_trips_through_bytes` |
| World save, wrong magic → not a save file | `moho_core::persist::tests::wrong_magic_returns_not_a_save_file` and `moho::save::tests::foreign_file_returns_not_a_save_file` |
| World save, unknown version → unsupported format | `moho_core::persist::tests::unknown_version_returns_unsupported_format` (versions 0 and current+1) |
| Chunk file, checksum mismatch → corrupt | `moho_core::persist::tests::flipped_payload_byte_returns_corrupt`; `moho_core::voxel::grid::tests::corrupt_chunk_leaves_grid_unchanged` |
| No partial state applied | `moho_game::scene_persistence::tests::failed_load_leaves_entities_untouched`; the grid test above |
| Pre-migration save refused | `moho::save::tests::pre_migration_world_save_returns_unsupported_format` (old v1 and v2 headers built byte by byte) |
| Envelope bytes pinned | `moho_core::persist::tests::envelope_bytes_are_stable`: asserts a literal byte array for a small fixture |
| Edge: kind mismatch | `moho_core::persist::tests::wrong_kind_returns_wrong_kind` |
| Edge: truncated header or payload | `moho_core::persist::tests::truncated_input_is_rejected` (proptest: every strict prefix of a valid file is an `Err`, never a panic) |
| Edge: deterministic chunk bytes | `moho_core::voxel::grid::paletted::tests::bytes_ignore_resource_insertion_order` |
| `bincode` gone | `cargo tree --workspace -i bincode` reports no match; `deny.toml` diff |

**Gate class:** domain (`moho_core`, `moho_game` persistence rules).

**Risks.**
- Snapshot without `insta`: the card asked for an `insta` snapshot, but `insta` isn't in the tree and has no ENG-F13 verdict. A literal byte-array assertion pins the same bytes with no new dev-dependency. Revisit if a second format test wants snapshot files.
- `crc32fast` and `postcard` are new direct dependencies with no ENG-F13 verdict row. postcard is decided by ADR-0006; crc32fast is already built, has no dependencies, and is cheaper than owning a table-driven CRC. Accepting this spec accepts both.
- Corrupt chunk policy: regenerating from seed throws away the player's edits in that chunk, now with an `error` line. Refusing to load the chunk instead needs a product call (a hole, a retry loop, a message). This card keeps today's behaviour.
- Old saves and chunk folders stop loading once this lands. Expected per ADR-0006; the PR description says so.
- Blast radius: root `save`, `scene_loader`, `autosave`, `world_generator`, `generation_processor`, `chunk_streamer`; `moho_game` persistence and three derive sites; `moho_core` grid; `moho_render_api`. Run GitNexus `impact` on `encode_to_bytes`, `load_from_bytes`, `PalettedChunk::{to_bytes, from_bytes}` and `VoxelGrid::deserialize_chunk_into` first.
