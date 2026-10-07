# Unused dependencies and default features are trimmed

**Feature:** [ENG-F2](_feature.md)

## Summary

Following [ADR-0008](../../adr/0008-keep-winit-for-windowing-and-input.md),
winit and egui-winit run without default features, which removes
`ab_glyph` → `ttf-parser` (RUSTSEC-2026-0192) at the cost of Wayland
client-side decorations. This was verified to build in 2026-10. The same pass
removes dependencies that are declared but unused.

## Deliverables

- `winit` in `[workspace.dependencies]` uses `default-features = false` with
  `rwh_06`, `x11`, `wayland` and `wayland-dlopen`. `egui-winit` uses
  `default-features = false` with `clipboard`, `wayland` and `x11`.
- The `ttf-parser` ignore is removed from `deny.toml`.
- Unused declared dependencies removed (2026-10 survey; re-verify each):
  - `moho_core`: `bytemuck`, `noise`, `serde` (`winit` and `legion` go in
    [ENG-F8-02](../ENG-F8-foundation-gate/ENG-F8-02-layering-check.md) and [ENG-F7-02](../ENG-F7-maintained-ecs/ENG-F7-02-legion-removed.md))
  - `moho_renderer`: `bincode`
  - `moho_ui`: `once_cell`, `gilrs`, `serde`, `bincode`
  - `moho_physics`: `moho_core`
  - `moho_render_api`: `glam`
  - root: `bytemuck`, `pollster`, `rand`
- `moho_ui`'s stale RAUI manifest comment is removed.
