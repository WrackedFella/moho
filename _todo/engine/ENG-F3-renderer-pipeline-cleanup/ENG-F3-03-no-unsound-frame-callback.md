# The renderer accepts a frame callback only in a sound form

**Feature:** [ENG-F3](_feature.md)
**Issue:** [#208](https://github.com/WrackedFella/moho/issues/208)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

The renderer has two ways to register the UI frame callback. One is a safe setter that
stores a `*mut dyn FrameCallback` which `finish_frame` later dereferences: nothing makes
the caller keep the pointer valid, so the path is unsound by construction. Nothing calls
it; the `Arc<Mutex<dyn FrameCallback>>` setter covers every caller. Delete the raw path
so M1 gate item 1.6 closes without waiting for ENG-F20-02 (#156), which later replaces
the wgpu-typed callback altogether.

## Deliverables

- `RendererBackend` and `Renderer` have no raw-pointer frame-callback setter, and
  `Renderer` stores no raw callback pointer.
- `finish_frame` takes the optional `Arc<Mutex<dyn FrameCallback>>` directly; the
  `FrameCallbackWrapper` enum is gone.
- `moho_renderer` frame code contains no `unsafe` block.
- The UI still draws through the `Arc` callback.

## Acceptance criteria

Refactor; checklist:

- [ ] No public item in `moho_renderer` accepts or stores a `*mut dyn FrameCallback`.
- [ ] `render_ops::frame_ops` contains no `unsafe` and no `#[allow(unsafe_code)]`.
- [ ] Every `RendererBackend` implementation (renderer and test mocks) compiles without
      a raw setter.
- [ ] `just check` passes.

## Tech spec

**Design**
- `moho_renderer`:
  - `RendererBackend`: remove `set_frame_callback_raw`; remove it from the `Renderer`
    impl and from the three test mocks (`instance_collector` tests, `buffer_manager`
    tests, `tests/scene_render_mock.rs`).
  - `Renderer`: remove the `frame_callback_raw` field and `set_frame_callback_raw_inherent`.
    `submit_frame` passes `self.frame_callback_arc.as_ref()`.
  - `render_ops::frame_ops`: delete `FrameCallbackWrapper` and its two tests;
    `finish_frame` takes `Option<&Arc<Mutex<dyn FrameCallback>>>` and calls it with
    `if let`. The lock-failure warning stays.
- Why `Option` over keeping a one-variant-plus-`None` enum: with the raw variant gone the
  enum is `Option` under another name.
- No caller changes: the binary's renderer setup and `moho_ui` already use only the `Arc`
  setter.

**Out of scope**
- Renaming `set_frame_callback_arc` or its `_inherent` twin, changing `FrameCallback`'s
  signature, or moving it off wgpu types: #156 (ENG-F20-02) replaces the callback.
- The other renderer polish items, now ENG-F3-04 (renderer polish).
- `src/app/renderer_setup.rs` and `moho_ui`: untouched.

**Test map**

| Criterion | Proof |
|---|---|
| No raw setter or field | compile: removing the trait method forces every impl to drop it; `cargo clippy --workspace --all-targets` |
| No `unsafe` in frame code | `rg -n 'unsafe' moho_renderer/src/render_ops` returns nothing (PR states the output) |
| Mocks compile | `just check` (nextest builds `moho_renderer` tests) |
| UI still draws | Verification below |

No new test. The deleted `frame_ops` tests asserted the wrapper's shape and size, not
behavior; the callback itself needs a GPU device, and `moho_renderer` is excluded from
`just mutants` for that reason.

**Gate class:** glue (renderer adapter code; tests and code together).

**Overlap**
- #156 (ENG-F20-02): its deliverable "remove `FrameCallback` and its raw/arc setters,
  wrapper and test stubs" shrinks to the `Arc` setter, the trait and the stubs. Its issue
  gets a comment when this card is filed.
- #138 (ENG-F10-01) rewrites `buffer_manager` tests and `tests/scene_render_mock.rs`;
  this card removes one stub method from each. Whichever lands second rebases.
- #170 (ENG-F22-03) and #178 (ENG-F22-11) touch renderer tests but leave `frame_ops`
  tests to #156; no conflict beyond the `instance_collector` mock line.
- ENG-F11 (#143 to #145): no shared files.

**Risks:** none beyond the UI draw path. Blast radius is `moho_renderer` only: GitNexus
upstream impact is LOW for the raw setter, `FrameCallbackWrapper` (no dependents) and
`finish_frame` (only `submit_frame`). The kept `FrameCallback` trait reaches `EguiAdapter`
and `StubUi` in `moho_ui`, which this card doesn't change.

## Verification

Manual: launch the game. The start menu, settings, console and HUD draw and respond as
before.
