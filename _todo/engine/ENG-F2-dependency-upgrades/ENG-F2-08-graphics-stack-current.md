# The graphics stack is on current wgpu and egui

**Feature:** [ENG-F2](_feature.md)
**Issue:** #101

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

## Verification

- Menus, settings, console and HUD render and accept input as before.
- Terrain, shadows, SSAO and skybox look as before.

## Notes

Record which files the upgrade touched; [ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md)
uses it as its baseline.
