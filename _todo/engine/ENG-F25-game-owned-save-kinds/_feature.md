# ENG-F25 — A game line adds save files without changing the engine

**Status:** Draft
**Labels:** feature, line:engine, M2

## Summary

The save envelope (`moho_core::persist`, [ADR-0006](https://github.com/WrackedFella/moho/blob/dev/_todo/adr/0006-save-format-contract.md)) is built, but it names the
strategy line's file kinds and holds their versions. So any strategy save change edits an
engine crate, and the FPS can't add its own saves without doing the same. Following
[ADR-0016](https://github.com/WrackedFella/moho/blob/dev/_todo/adr/0016-game-lines-own-their-save-kinds.md), each line defines its own kinds and versions,
and the engine keeps only the framing. Consumers: SG-F4's mining marks ([#310](https://github.com/WrackedFella/moho/issues/310)),
which bump the World version, and the FPS line's coming save files (Justin, 2026-10-14).

## Exit criteria

- No engine crate names a game file kind. `moho_core::persist` takes a kind as data
  (tag and current version).
- The strategy line's World, Scene and Chunk kinds live in the crates that write them,
  with unchanged tags and versions, so saves written before the change still load (test).
- Each strategy kind has a pinned-bytes test, so a change to the payload shape fails a test.
- A test defines a kind outside `moho_core` and round-trips it.

## Scope

- In: `moho_core::persist` API; strategy kind definitions; per-kind pinned-bytes tests.
- Out: any change to what is saved (SG-F4 marks, ENG-F19 root list); on-disk layout
  changes; an FPS save file (it has no consumer code yet); compression, async I/O.

## Decisions

| Question | Decision | Why |
|---|---|---|
| `u8` tag or a 4-byte tag | Keep the `u8` tag, unique within a line | No layout change, so existing saves keep loading. Lines never read each other's files, so cross-line collisions can't happen. |
| ENG-F19's root list in the header | No, in the world-save payload, first field | Justin, 2026-10-14; comment on [#231](https://github.com/WrackedFella/moho/issues/231) |

## Items

| Item |
|---|
| [ENG-F25-01 game-owned file kinds](ENG-F25-01-game-owned-file-kinds.md) |
