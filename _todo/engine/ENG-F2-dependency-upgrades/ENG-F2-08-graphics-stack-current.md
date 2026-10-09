# The graphics stack is on current wgpu and egui

**Feature:** [ENG-F2](_feature.md)
**Issue:** [#101](https://github.com/WrackedFella/moho/issues/101)
**Status:** Ready
**Gate class:** glue
**Labels:** line:engine

## Summary

ENG-F13 found `wgpu`/`naga` one major behind (29 vs 30) and egui two minors
behind (0.34 vs 0.36). The watchlist says wgpu 30 waits on `egui-wgpu`, but
`egui-wgpu` 0.36.2 has shipped since. This is the M1-close upgrade under the
upgrade policy.

## Deliverables

- `egui`, `egui-winit`, `egui-wgpu` on the latest release; `wgpu` and `naga`
  on the version that `egui-wgpu` supports (30 if available).
- The watchlist entry is updated or removed.

## Acceptance criteria

- [ ] `just check` and `just deny` pass.
- [ ] Shader validation tests pass on the new `naga`.

## Tech spec

**Versions it lands on (checked on crates.io, 2026-10-07).** `egui`, `egui-winit` and `egui-wgpu` 0.36.2. `egui-wgpu` 0.36.2 requires `wgpu ^30.0`, so the watchlist's "wgpu 30 waits on egui-wgpu" is cleared. `wgpu` 30.0.1 and `naga` 30.0.1.

**Depends on** [#95 ENG-F2-02, egui entry points](https://github.com/WrackedFella/moho/issues/95). egui 0.36 **removes** `Context::run`, `TopBottomPanel` and the `&Context` panel `show` methods that 0.34 only deprecated, so this upgrade doesn't compile until #95 has landed.

**Design.**
- Manifests: `[workspace.dependencies]` sets `wgpu = "30.0.1"` and `naga = "30.0.1"`. Move `egui`, `egui-winit` and `egui-wgpu` into `[workspace.dependencies]` at 0.36.2 (CLAUDE.md: shared versions go there), keeping the `egui-winit` features from [#96](https://github.com/WrackedFella/moho/issues/96).
- Breaks found by compiling a copy of `dev` on 2026-10-07; the list is complete for `moho_renderer` and for the first error pass of `moho_ui`:
  - `moho_renderer::device`: `RequestAdapterOptions` gains `apply_limit_buckets: false`; `SurfaceConfiguration` gains `color_space: wgpu::SurfaceColorSpace::Auto` (wgpu's previous behaviour).
  - `moho_renderer::render_ops::frame_ops`: `frame.present()` → `queue.present(frame)`.
  - `moho_renderer::{pipeline, shadow}`: `VertexState::buffers` takes `Option<VertexBufferLayout>`, so wrap each layout in `Some` (3 sites).
  - `moho_ui::adapter`: `SurfaceConfiguration` gains `color_space`. In `gpu_ops::update_textures`, `TexturesDelta::set` now yields a `SmallVec<[ImageDelta; 1]>` per texture id, so iterate it and call `update_texture` for each delta, in order.
  - `src/app/renderer_setup` wasn't reachable in the probe (it stopped at `moho_ui`). The implementer fixes whatever `just check` reports there in the same way.
- Feature watchlist: replace the wgpu entry with the result (on 30; nothing blocked).
- ENG-F20 baseline: the PR description lists every source file the upgrade touched (expected: the 6 files above plus `renderer_setup`), and the card's Notes record that list.

**Out of scope.**
- winit 0.31 (ADR-0008: follow egui's integration crates; egui-winit 0.36 is still on winit 0.30.13).
- Taking advantage of new wgpu features (color spaces, limit buckets), and refactoring to contain wgpu ([ENG-F20](https://github.com/WrackedFella/moho/issues/104)).
- egui API migrations beyond what 0.36 forces. #95 owns the deprecation work.

**Test map.**
| Criterion | Proof | Gate class |
|---|---|---|
| `just check` and `just deny` pass | the two commands | glue |
| Shader validation passes on naga 30 | existing `moho_renderer/tests/shaders_validation.rs` (passed on naga 30.0.1 in the probe) | glue |
| UI and rendering unchanged | Verification (manual) | glue |

**Gate class:** glue.

**Risks.**
- Verification needs a person to look at the game (menus, HUD, shadows, SSAO, skybox). An agent can't sign off on that step.
- `SurfaceColorSpace::Auto` reproduces wgpu's old choice. If colours shift on an HDR-capable display, that is the field to inspect.

## Verification

- Menus, settings, console and HUD render and accept input as before.
- Terrain, shadows, SSAO and skybox look as before.

## Notes

Record which files the upgrade touched; [ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md)
uses it as its baseline.
