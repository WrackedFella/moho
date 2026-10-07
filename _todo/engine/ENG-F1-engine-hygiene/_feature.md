# ENG-F1 — Engine Hygiene

**Note:** Low priority, opportunistic

## Summary

Engine-generic cleanup items — none block gameplay. Land opportunistically
when already touching the relevant file; no standalone sweep. Portable to the
engine repo once the split ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md)) happens.

## Exit criteria

- The only `#[allow(unsafe_code)]` site in the workspace is the
  `wgpu-experimental` opt-in ([ENG-F1-06](ENG-F1-06-unsafe-surface.md), [ENG-F3-03](../ENG-F3-renderer-pipeline-cleanup/ENG-F3-03-misc-cleanups.md)).
- Remaining criteria to be set when the feature is next planned.

## Items

| Item |
|---|
| [ENG-F1-01 constructor-size-cleanup](ENG-F1-01-constructor-size-cleanup.md) |
| [ENG-F1-02 error-handling-backlog](ENG-F1-02-error-handling-backlog.md) |
| [ENG-F1-03 god-module-splits](ENG-F1-03-god-module-splits.md) |
| [ENG-F1-04 keybind-test-layering](ENG-F1-04-keybind-test-layering.md) |
| [ENG-F1-05 material-model-revisit](ENG-F1-05-material-model-revisit.md) |
| [ENG-F1-06 unsafe-surface](ENG-F1-06-unsafe-surface.md) |
| [ENG-F1-07 lint-ratchet](ENG-F1-07-lint-ratchet.md) |
| [ENG-F1-08 headless-app-tests](ENG-F1-08-headless-app-tests.md) |
| [ENG-F1-09 malformed-prefs-are-reported](ENG-F1-09-malformed-prefs-are-reported.md) |

## Notes

[ENG-F1-06](ENG-F1-06-unsafe-surface.md) deviated from its spec in two ways that constrain later work:

- `EguiAdapter` is `Send` but cannot be `Sync`: egui-winit's clipboard holds
  an `mpsc::Receiver`. The removed `unsafe impl Sync` was false, not merely
  unproven. Sharing works only through `Mutex` (or another wrapper that needs
  just `Send`). An `RwLock`, a plain `Arc<EguiAdapter>`, or an ECS resource
  slot that requires `Sync` (relevant to [ENG-F7](../ENG-F7-maintained-ecs/_feature.md)) will not compile. Don't
  restore `unsafe impl Sync` to get around this.
- The compile-time assertion beside `EguiAdapter` is `Send` only, not the
  planned `Send + Sync`. `Modal` now has a `Send` supertrait, so modals must
  use thread-safe state (no `Rc`/`RefCell`).
