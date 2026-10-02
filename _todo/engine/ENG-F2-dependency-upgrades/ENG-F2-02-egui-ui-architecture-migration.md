# egui 0.34 UI-architecture migration

**Status:** not started
**Feature:** ENG-F2

## Summary

`Context::run`/`Panel::show`/`CentralPanel::show` are deprecated in favor of
`run_ui`/`show_inside` (`&mut Ui`-driven instead of `&Context`-driven).
Currently suppressed via `#![allow(deprecated)]` in `moho_ui`. Touches how
every screen/overlay obtains its root `Ui` — a real architecture change, not
a rename.

## Deliverables

- Every `moho_ui` screen/overlay migrated off the deprecated entry points.
- `#![allow(deprecated)]` removed from `moho_ui/src/lib.rs`.
