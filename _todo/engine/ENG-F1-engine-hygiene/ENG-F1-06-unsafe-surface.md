# UI layer carries no unsafe code

**Status:** done
**Feature:** ENG-F1
**Issue:** #57

## Summary

The workspace denies `unsafe_code`; three of its five scoped allows are in
`moho_ui`: a lifetime `transmute` in the egui render pass and the
`unsafe impl Send`/`Sync` on `EguiAdapter`. Remove all three.

## Deliverables

- egui render pass without the lifetime `transmute`.
- `EguiAdapter` without `unsafe impl Sync`.
- `EguiAdapter` without `unsafe impl Send`.

## Acceptance criteria

- [ ] The egui render pass contains no `unsafe`.
- [ ] `EguiAdapter` has no `unsafe impl Sync`.
- [ ] `EguiAdapter` has no `unsafe impl Send`.
- [ ] `moho_ui` has no `unsafe` and no `#[allow(unsafe_code)]`.
- [ ] `unsafe` outside `moho_ui` is unchanged.
- [ ] `just check` passes with no new lint allows.

## Tech spec

**Design** (`moho_ui` only; both changes trial-compiled clean workspace-wide):

- `adapter::gpu_ops::execute_render_pass`: chain `.forget_lifetime()` onto
  `begin_render_pass` and pass `&mut render_pass` to
  `egui_wgpu::Renderer::render`. Delete the `transmute` and its SAFETY comment.
  wgpu's safe API for exactly this case; the encoder is locked until the pass
  drops at function end, which the current code already satisfies.
- `modal::Modal`: add a `Send` supertrait (`pub trait Modal: Send`). This was
  the only non-`Send` field in `EguiAdapter`; with it, `Send` is
  auto-derived. Delete both `unsafe impl`s and their SAFETY comment.
  Chosen over a justified `unsafe impl Send` because the bound costs nothing
  (the one impl, `KeybindConflictModal`, is already `Send`) and the compiler
  then proves what the comment only asserted.
- Add a compile-time assertion beside `EguiAdapter` that it is `Send`, so a
  future non-`Send` field fails in `moho_ui` rather than at the binary's
  `InputDispatcher::register` call site. Only `Send` is needed: the adapter is
  shared as `Arc<Mutex<_>>`, which is `Send + Sync` iff the adapter is `Send`.
  `Sync` is unattainable (egui-winit's clipboard holds an `mpsc::Receiver`), so
  the removed `unsafe impl Sync` was asserting something false.

**Out of scope:** the renderer's raw frame-callback path (ENG-F3-03); the
`wgpu-experimental` block in `moho_renderer`; the `Arc<Mutex<EguiAdapter>>`
sharing model and `InputDispatcher`'s `Send + Sync` handler bound; any other
`gpu_ops` or adapter refactor.

**Test map** (gate class: **glue**; type-level changes, no runtime behavior):

| Criterion | Proof |
|---|---|
| No `unsafe` in render pass | `just check` (workspace `unsafe_code` deny) + review |
| No `unsafe impl Sync` / `Send` | `just check`; the `Send` assertion compiles |
| No `unsafe` in `moho_ui` | grep `unsafe` in `moho_ui`: expect 0 |
| `unsafe` outside `moho_ui` unchanged | diff touches only `moho_ui` |
| Gate passes, no new allows | `just check`; `Cargo.toml` lint tables untouched |

No new runtime tests: no test drives `execute_render_pass` (needs a GPU), so
render correctness is covered by Verification. `cargo mutants` has nothing to
mutate in a trait bound or a method chain.

**Risks:** `Modal: Send` is a public trait change: any future modal holding
`Rc`/`RefCell` must use thread-safe types instead. Blast radius is one impl
today. Render-pass change is runtime-only to observe; a misuse of the
encoder while the pass is alive would surface as a wgpu validation panic, not
a compile error.

## Verification

- Main menu, settings, console, HUD and a modal render and accept input as
  before.

## Notes

- Out: the raw frame-callback path in the renderer (ENG-F3-03) and the
  `wgpu-experimental` opt-in, which stays.
