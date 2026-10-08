# ENG-F1 — Engine Hygiene

**Note:** Low priority, opportunistic
**Status:** Draft
**Labels:** feature, line:engine

## Summary

Engine-generic cleanup items — none block gameplay. Land opportunistically
when already touching the relevant file; no standalone sweep. Portable to the
engine repo once the split ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md)) happens.

## Exit criteria

- The only `#[allow(unsafe_code)]` site in the workspace is the
  `wgpu-experimental` opt-in ([ENG-F1-06](https://github.com/WrackedFella/moho/issues/57), [ENG-F3-03](https://github.com/WrackedFella/moho/issues/208)).
- Remaining criteria to be set when the feature is next planned.

## Items

| Item |
|---|
| [ENG-F1-01 constructor-size-cleanup](ENG-F1-01-constructor-size-cleanup.md) |
| [ENG-F1-02 error-handling-backlog](ENG-F1-02-error-handling-backlog.md) |
| [ENG-F1-03 god-module-splits](ENG-F1-03-god-module-splits.md) |
| [ENG-F1-04 keybind-test-layering](ENG-F1-04-keybind-test-layering.md) |
| [ENG-F1-05 material-model-revisit](ENG-F1-05-material-model-revisit.md) |
| [ENG-F1-06 unsafe-surface](https://github.com/WrackedFella/moho/issues/57) |
| [ENG-F1-07 lint-ratchet](ENG-F1-07-lint-ratchet.md) |
| [ENG-F1-08 headless-app-tests](ENG-F1-08-headless-app-tests.md) |
| [ENG-F1-09 malformed-prefs-are-reported](https://github.com/WrackedFella/moho/issues/91) |
| [ENG-F1-10 key-capture-disarms-on-conflict](ENG-F1-10-key-capture-disarms-on-conflict.md) |

## Notes

[ENG-F1-06](https://github.com/WrackedFella/moho/issues/57) deviated from its spec in two ways that constrain later work:

- `EguiAdapter` is `Send` but cannot be `Sync`: egui-winit's clipboard holds
  an `mpsc::Receiver`. The removed `unsafe impl Sync` was false, not merely
  unproven. Sharing works only through `Mutex` (or another wrapper that needs
  just `Send`). An `RwLock`, a plain `Arc<EguiAdapter>`, or an ECS resource
  slot that requires `Sync` (relevant to [ENG-F7](https://github.com/WrackedFella/moho/issues/62)) will not compile. Don't
  restore `unsafe impl Sync` to get around this.
- The compile-time assertion beside `EguiAdapter` is `Send` only, not the
  planned `Send + Sync`. `Modal` now has a `Send` supertrait, so modals must
  use thread-safe state (no `Rc`/`RefCell`).
