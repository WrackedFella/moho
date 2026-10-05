# 0006 — Saves use one versioned envelope around a serde payload

**Status:** Accepted

## Context

Persisted data is encoded with `bincode` 2.0.1 through its own
`Encode`/`Decode` derives. Those derives sit on domain types in `moho_game`,
`moho_core` (paletted chunks) and `moho_render_api`. bincode is unmaintained
(RUSTSEC-2025-0141). Its 3.0.0 release contains nothing but a
`compile_error!`, so "upgrade to bincode 3" ([ENG-F2-01](../engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) as written) isn't
possible.

There are four on-disk layouts with separate headers: the world save (`MOHO`
magic, u32 version), the scene file, chunk files, and the snapshot (magic,
u16 version, length, CRC32). Only the snapshot detects corruption.

No long-lived saves exist yet. Every feature that persists new state ([SG-F3](../strategy-game/SG-F3-buildings-and-construction/_feature.md)
buildings, [SG-F4](../strategy-game/SG-F4-worker-pawns/_feature.md) pawns, inventory) adds types to this format.

## Decision

- **Encoding:** `postcard` 1.x over `serde` derives. postcard is maintained,
  has a written wire specification that is stable across 1.x, and uses the
  `serde` derives the workspace already depends on. Domain types derive
  `Serialize`/`Deserialize`, never an encoder-specific trait.
- **Envelope:** one shared header for every persisted file: magic, format
  kind, u16 version, payload length, CRC32 of the payload. Unknown versions
  and checksum failures return a typed error. They never fall back to
  defaults.
- **Compatibility promise:** none before `v1.0`. Old saves are rejected with
  a clear error, and no migration shims are written. From `v1.0` on, the
  reader loads version N−1 and rewrites it as N on the next save.

## Consequences

- [ENG-F2-01](../engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md) becomes "saves use the ADR-0006 envelope and encoding". Its
  migration lands before the first feature adds a persisted type, and old
  saves break once.
- `bincode` and its advisory ignore go away when the migration lands.
- Snapshot and save code share one envelope implementation instead of
  three hand-rolled headers.
- `moho_render_api` keeps only `serde` derives, so the render contract no
  longer depends on a save encoder.
