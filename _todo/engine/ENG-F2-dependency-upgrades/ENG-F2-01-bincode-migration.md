# Saves use the ADR-0006 envelope and encoding

**Status:** not started (decided by [ADR-0006](../../adr/0006-save-format-contract.md); must land before [SG-F3](../../strategy-game/SG-F3-buildings-and-construction/_feature.md))
**Feature:** [ENG-F2](_feature.md)

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
