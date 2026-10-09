# UI screens use egui's current entry points

**Feature:** [ENG-F2](_feature.md)
**Issue:** [#95](https://github.com/WrackedFella/moho/issues/95)
**Status:** Ready
**Gate class:** glue
**Labels:** line:engine

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

## Tech spec

**Order: lands before [#101 ENG-F2-08, graphics stack](https://github.com/WrackedFella/moho/issues/101).** The replacement entry points (`Context::run_ui`, `Panel`, `show_inside`) already exist in egui 0.34.3 with the same signatures as 0.36.2, and 0.36 removes the deprecated ones. Migrating on 0.34.3 is therefore migrating to the API that 0.36 keeps. #101 can't compile until this has landed.

**Design.**
- With `#![allow(deprecated)]` removed, `moho_ui` reports 31 deprecated uses in 9 files (counted on a copy of `dev`, 2026-10-07): `Context::run` (10, 9 of them in tests), `CentralPanel::show` (6), `Panel::show` (6), `TopBottomPanel` (6, as a type).
- Root: `UiAdapter`'s frame (`adapter/mod.rs`) calls `self.context.run_ui(raw_input, |ui| …)`. Screens and overlays receive `&mut egui::Ui` instead of `&egui::Context`, and use `ui.ctx()` where they need the context (windows, areas, input state, style).
- Panels: `TopBottomPanel::top/bottom(id)` → `Panel::top/bottom(id)`, and `.show(ctx, …)` / `CentralPanel::default().show(ctx, …)` → `.show_inside(ui, …)`. Panels inside one screen keep their current order (top, bottom, then central), because the remaining space depends on it.
- Screen and overlay entry points change signature from `&Context` to `&mut Ui` (about 27 functions take `&egui::Context` today). This is the architecture change: every screen gets its root `Ui` from the adapter.
- Tests that drive a frame with `ctx.run(RawInput::default(), |ctx| …)` use `ctx.run_ui(…, |ui| …)`.
- Delete `#![allow(deprecated)]` and its comment from `moho_ui/src/lib.rs`.

**Out of scope.**
- The egui version bump (#101), and wrapping egui behind an engine API (ENG-F20 decision: no).
- Redesigning screens or changing layout or styling, and splitting `moho_ui` into an engine UI shell (ENG-F18).
- Non-deprecated `Window`/`Area` `show(ctx)` calls. Reach them through `ui.ctx()`; don't restructure them.

**Test map.**
| Criterion | Proof | Gate class |
|---|---|---|
| No deprecated egui entry point | `just check` with `#![allow(deprecated)]` removed (rustc deprecation warnings fail `-D warnings`) | glue |
| `moho_ui` builds without `allow(deprecated)` | the `lib.rs` diff | glue |
| Existing UI tests pass | existing `moho_ui` unit tests and `moho_ui/tests/*` | glue |
| Screens behave as before | Verification (manual) | glue |

**Gate class:** glue.

**Risks.**
- Verification needs a human to click through every screen and overlay. Layout regressions (panel order, central-panel space) won't show up in tests.
- The signature change ripples through every screen. Run GitNexus `impact` on the adapter's frame function and the screen trait or entry points first.

## Verification

- Start menu, new-world, settings (all tabs), console, modals and every HUD
  overlay open, render and take input as before.

## Notes

Lands before [ENG-F2-08](ENG-F2-08-graphics-stack-current.md): egui 0.36
removes the deprecated entry points, and their replacements already exist in
0.34.3 with the same signatures (checked 2026-10-07).
