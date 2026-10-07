# Unused dependencies and default features are trimmed

**Feature:** [ENG-F2](_feature.md)
**Issue:** #96

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
- Unused declared dependencies removed (survey corrected by the
  [ENG-F13 audit](../ENG-F13-dependency-audit/audit.md); re-verify each):
  - `moho_core`: `bytemuck`, `noise` (`moho_core` does use `serde`; keep it)
  - `moho_renderer`: `bincode`, `tempfile` (dev)
  - `moho_ui`: `once_cell`, `gilrs`, `serde`, `bincode`, `ini`
  - `moho_audio`: `env_logger` (dev)
  - `moho_physics`: `moho_core`
  - `moho_render_api`: `glam`
  - root: `bytemuck`, `pollster`, `rand`
- `moho_ui`'s stale RAUI manifest comment is removed.
- The yanked `chacha20` 0.10.1 is updated out of the lockfile; `just deny`
  reports no yanked crates.

## Acceptance criteria

- [ ] Each listed declaration is absent from its manifest, and `just check` passes.
- [ ] `cargo tree --workspace -i ttf-parser` reports no match; the RUSTSEC-2026-0192 ignore is gone.
- [ ] `just deny` passes with no yanked-crate warning.

## Verification

- The window opens and resizes on X11 and Wayland; on Wayland it has no
  client-side title bar (accepted cost, ADR-0008).
- Clipboard copy and paste work in the console.
