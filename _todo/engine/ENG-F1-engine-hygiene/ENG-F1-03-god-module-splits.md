# Split oversized modules

**Status:** not started
**Feature:** ENG-F1

## Summary

Three files mix distinct concerns and are worth splitting when next touched
for another reason — not a standalone refactor.

## Deliverables

- `voxel/grid.rs` (875 lines) — storage vs. mesh queries vs. light queries vs.
  serialization.
- `voxel/modification.rs` (716 lines) — mutation state machine vs. grid
  updates.
- `prefs/mod.rs` (653 lines) — parsing vs. validation vs. storage.
- `moho_ui/src/screens/settings/keybind_capture.rs` — verify a prior split
  (commit `c051802`) actually got it under ~400 lines; if not, separate the
  capture state machine from rendering.
