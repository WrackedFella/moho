# 0016 — Game lines own their save kinds and versions; the envelope stays content-blind

**Status:** Proposed (extends [0006](0006-save-format-contract.md))

## Context

[ADR-0006](0006-save-format-contract.md)'s envelope (`moho_core::persist`) frames every
persisted file. Its kind list is a closed engine enum, `FileKind { World, Scene, Chunk }`,
and the engine also holds each kind's current version. All three kinds belong to the
strategy line: chunk files are `moho_voxel`'s, the scene is `moho_game`'s, and the world
save is the binary's. Two consequences follow. A strategy save change (such as SG-F4's
mining marks, [#310](https://github.com/WrackedFella/moho/issues/310)) has to edit an engine
crate to bump its version, and the FPS line can't add its own save files without doing the
same. That cuts against [ADR-0010](0010-world-geometry-is-a-mesh-contract.md), under which
the engine doesn't know what a game stores. The FPS will have its own save requirements
(Justin, 2026-10-14).

postcard is positional and not self-describing. Adding, removing or reordering a field, or
reordering enum variants, changes the wire format. If the version isn't bumped, an old file
fails as `Corrupt` instead of `UnsupportedFormat`.

[ENG-F19](https://github.com/WrackedFella/moho/issues/231) has to record the active content
roots in saves. The question was whether that list belongs in the envelope.

## Decision

- **The engine owns the framing only:** magic, kind tag, version, length, CRC32, and the
  checks on them. The on-disk layout (19 bytes, `u8` tag) is unchanged.
- **Game lines own their kinds.** A file kind is plain data that the line defines, made of
  a `u8` tag and the newest version this build writes. The engine names no kinds. A tag is
  unique within its line. Lines never read each other's files, so tags may repeat across
  lines.
- **Any change to a payload's serde shape bumps that kind's version.** Each kind has a test
  that pins the bytes of a small fixture, so a shape change fails a test and the fix
  includes the bump. The ADR-0006 compatibility promise still holds: an old version is
  rejected before v1.0 and read as N−1 from v1.0.
- **Content metadata goes in the payload, never the header.** ENG-F19's active root list is
  the first field of each game's world-save payload, so it can be decoded without decoding
  the rest. Each game decides what a root-list mismatch means.

## Consequences

- `FileKind` stops being an engine enum. `PersistError` reports tags as `u8`, and an
  unknown tag is reported as `WrongKind` instead of `NotASaveFile`. Version 0 is still
  reported as `UnsupportedFormat` first, so pre-envelope saves keep their error.
- The strategy line defines World, Scene and Chunk in the crates that write them, with the
  same tags and versions, so existing saves still load.
- SG-F4's marks bump the World version in `src/save.rs`, not in `moho_core`.
- The FPS line adds save files without touching an engine crate.
- The envelope never needs to learn about content roots, mods or any other game concept.
