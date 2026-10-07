# UI screens use egui's current entry points

**Feature:** [ENG-F2](_feature.md)
**Issue:** #95

## Summary

`Context::run`/`Panel::show`/`CentralPanel::show` are deprecated in favor of
`run_ui`/`show_inside` (`&mut Ui`-driven instead of `&Context`-driven).
Currently suppressed via `#![allow(deprecated)]` in `moho_ui`. Touches how
every screen/overlay obtains its root `Ui` — a real architecture change, not
a rename.

## Deliverables

- Every `moho_ui` screen/overlay migrated off the deprecated entry points.
- `#![allow(deprecated)]` removed from `moho_ui/src/lib.rs`.

## Acceptance criteria

- [ ] No `moho_ui` screen or overlay calls a deprecated egui entry point.
- [ ] `moho_ui` builds under `just check` with no `allow(deprecated)`.
- [ ] Existing UI tests pass.

## Verification

- Start menu, new-world, settings (all tabs), console, modals and every HUD
  overlay open, render and take input as before.

## Notes

Lands after [ENG-F2-08](ENG-F2-08-graphics-stack-current.md), so it targets
the egui release it will live on.
