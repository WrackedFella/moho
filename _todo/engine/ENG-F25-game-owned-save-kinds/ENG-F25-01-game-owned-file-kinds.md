# Game lines define their own save file kinds and versions

**Feature:** [ENG-F25](_feature.md)
**Status:** Draft
**Gate class:** domain
**Labels:** line:engine, M2

## Summary

`moho_core::persist` holds a closed `FileKind { World, Scene, Chunk }` enum along with
each kind's version, and all three kinds belong to the strategy line. Per
[ADR-0016](https://github.com/WrackedFella/moho/blob/dev/_todo/adr/0016-game-lines-own-their-save-kinds.md),
a kind becomes data that the line defines, and the strategy kinds move to the crates
that write them. The on-disk layout doesn't change, so existing saves still load.

## Acceptance criteria

```gherkin
Scenario: A game line defines a file kind the engine has never heard of
  Given a file kind defined outside moho_core with tag 200 and version 3
  When a value is encoded with it and decoded with the same kind
  Then the decoded value equals the one encoded

Scenario: Strategy saves written before this change still load
  Given world, scene and chunk bytes encoded by today's dev build
  When they are decoded after this change
  Then each decodes to the value that was written

Scenario Outline: The envelope rejects mismatches using the caller's kind
  Given a file whose header has <header>
  When it is decoded as a kind with tag 1 and version 2
  Then decoding returns <error>
  Examples:
    | header              | error                                   |
    | tag 1, version 3    | UnsupportedFormat { tag: 1, version: 3 } |
    | tag 1, version 0    | UnsupportedFormat { tag: 1, version: 0 } |
    | tag 7, version 1    | WrongKind { expected: 1, found: 7 }      |
    | tag 7, version 0    | UnsupportedFormat { tag: 7, version: 0 } |

Scenario: A change to a strategy payload's shape fails a test
  Given a pinned-bytes test for each of World, Scene and Chunk
  When a field is added to that payload type
  Then that kind's pinned-bytes test fails
```

- [ ] `moho_core` names no game file kind: `rg 'World|Scene|Chunk' moho_core/src/persist.rs` has no hits outside tests.
- [ ] The strategy kinds have distinct tags (test).

## Tech spec

**Design.**
- `moho_core::persist`:
  - `FileKind` becomes `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct FileKind { pub tag: u8, pub version: u16 }`.
    `version` is the newest version this build writes and reads. `current_version()` and
    `from_byte()` are removed. A `const`-constructible struct with public fields, because a
    kind is plain data that the defining crate declares as a `const`. A trait would add a
    level of indirection with only two implementing lines.
  - `PersistError::UnsupportedFormat { tag: u8, version: u16 }` and
    `PersistError::WrongKind { expected: u8, found: u8 }`. The other variants don't change.
  - `encode` writes `kind.tag` and `kind.version`. The header is unchanged: 19 bytes,
    little-endian.
  - `decode(expected, bytes)` checks in this order: length and magic, else `NotASaveFile`;
    version 0, else `UnsupportedFormat { tag: found, version: 0 }`; tag equals
    `expected.tag`, else `WrongKind`; version ≤ `expected.version`, else
    `UnsupportedFormat`; then length, CRC and postcard as today. The version-0 check runs
    first so that pre-envelope saves (`MOHO` + `u32` 1 or 2) still return
    `UnsupportedFormat`.
- Strategy kinds, declared where they are written, with today's tags and versions:
  - `src/save.rs`: `pub const WORLD_FILE: FileKind = FileKind { tag: 1, version: 1 };`
  - `moho_game::scene_persistence`: `pub const SCENE_FILE: FileKind = FileKind { tag: 2, version: 1 };`
  - `moho_voxel::grid::paletted`: `pub const CHUNK_FILE: FileKind = FileKind { tag: 3, version: 1 };`
  - Call sites switch from `FileKind::X` to these constants. The binary sees all three, so
    the distinct-tags test lives there.
- Pinned-bytes tests: one per strategy kind. Each encodes a small fixed value of the real
  payload type (`WorldFile`, `SceneDesc`, `ChunkSnapshot`) and compares it with a literal
  byte array. The test writer produces the literals from current `dev` before any
  production change, which is what proves the "still load" scenario. A failure message
  says to bump the kind's version along with the new bytes.

**Out of scope.**
- SG-F4 mining marks and the World version bump ([#310](https://github.com/WrackedFella/moho/issues/310)).
- ENG-F19's root list ([#231](https://github.com/WrackedFella/moho/issues/231)).
- Any FPS save kind, since no FPS code persists anything yet.
- A wider tag, or any other change to the header layout.
- Moving `src/save.rs` or chunk-file I/O into a crate.
- Changing the corrupt-chunk regenerate policy in `ChunkStreamer::load_chunk`.

**Footprint.** One part. The `PersistError` field changes reach every test that matches on them.
- `moho_core/src/persist.rs`: the struct, errors, decode order and tests.
- `moho_voxel/src/grid/paletted.rs`: `CHUNK_FILE`, call sites, pinned test.
- `moho_game/src/scene_persistence.rs`: `SCENE_FILE`, call sites, pinned test, the `WrongKind` test.
- `src/save.rs`: `WORLD_FILE`, call sites, pinned test, distinct-tags test, the error-shape tests.

**Test map.**
| Scenario / row | Test |
|---|---|
| Kind defined outside `moho_core` round-trips | `moho_core::persist::tests::caller_defined_kind_round_trips` (tag 200, version 3) |
| Saves from today's dev still load | `moho::save::tests::world_payload_bytes_are_pinned`, `moho_game::scene_persistence::tests::scene_payload_bytes_are_pinned`, `moho_voxel::grid::paletted::tests::chunk_payload_bytes_are_pinned`, each decoding the pinned literal back to the fixture as well |
| tag 1, version 3 → Unsupported | `moho_core::persist::tests::version_above_expected_returns_unsupported_format` |
| tag 1, version 0 → Unsupported | `moho_core::persist::tests::version_zero_returns_unsupported_format` |
| tag 7, version 1 → WrongKind | `moho_core::persist::tests::other_tag_returns_wrong_kind` (replaces `unknown_kind_byte_returns_not_a_save_file`) |
| tag 7, version 0 → Unsupported | `moho_core::persist::tests::version_zero_is_reported_before_wrong_kind` |
| Shape change fails a test | the three pinned-bytes tests above |
| Pre-envelope saves still refused | `moho::save::tests::pre_migration_world_save_returns_unsupported_format` (fields renamed to `tag`) |
| Distinct strategy tags | `moho::save::tests::strategy_file_tags_are_distinct` |
| Envelope layout unchanged | `moho_core::persist::tests::envelope_bytes_are_stable` (same literal, local kind `{ tag: 1, version: 1 }`) |

**Gate class:** domain (`moho_core`, `moho_voxel`, `moho_game`).

**Risks.**
- Behaviour change: an unknown tag now returns `WrongKind` instead of `NotASaveFile`. No
  caller distinguishes the two (`ChunkStreamer` logs any error, and the load menu shows
  the message).
- The tag literals are now spread across three crates. The distinct-tags test guards
  against collisions.
- Blast radius: GitNexus resolved no callers of `FileKind`, which doesn't mean there are
  none. A text search finds them in `src/save.rs`, `moho_game/src/scene_persistence.rs`
  and `moho_voxel/src/grid/paletted.rs`.
