# 0008 — Keep winit for windowing and input; add gamepads with gilrs when needed

**Status:** Accepted

## Context

Concrete friction with the windowing and input stack, 2026-10:

1. Three keyboard tables keyed by an ad-hoc `u32` code: winit `KeyCode` → code
   (`moho_input`, `0` as the "unmapped" sentinel), INI name → code
   (`moho_core::prefs`), and egui key → code/label (`moho_ui`).
2. Cursor grab tries `Confined` first, then `Locked`, and ignores failure.
   Mouselook needs `Locked` (relative motion). `Confined` is unsupported on
   macOS and `Locked` is the only grab Wayland offers.
3. No gamepad support. `gilrs` is declared as an optional `moho_ui`
   dependency, but no code uses it.
4. winit's default `wayland-csd-adwaita` feature (also enabled through
   `egui-winit`'s defaults) pulls in `ab_glyph` → `ttf-parser`, an
   unmaintained crate with an ignored advisory.
5. Version coupling: winit 0.31 and wgpu 30 both wait on egui's integration
   crates. The duplicate `smithay-client-toolkit`/`calloop` versions come from
   winit 0.30 and egui's clipboard pulling different versions.

Items 1–2 are our own code. Item 3 has an established companion crate.
Item 4 is configuration. Item 5 would apply to any windowing crate that egui
integrates with. SDL3 would solve item 3 by bundling gamepad support, but it
adds a C library to build and ship on three OSes, depends on a 0.x Rust
binding, and has a weaker egui integration. It fixes none of items 1, 2, 4
or 5.

## Decision

- Keep winit 0.30 and follow egui's integration crates for upgrades.
- Gamepads go through `gilrs` once a feature needs them (likely [FPS-F1](../fps-game/FPS-F1-game-design-document/_feature.md)). The
  unused optional dependency is removed until then.
- winit and egui-winit run without default features. Enable `x11`, `wayland`,
  `wayland-dlopen` and `rwh_06` explicitly, plus egui-winit's `clipboard`.
  Wayland windows lose client-side decorations; the game runs
  fullscreen-first.
- One engine-owned key identifier replaces the `u32` binding code. The winit
  mapping lives in the platform adapter, and prefs store key names.
- Revisit SDL3 only if winit blocks a shipped platform target or a named
  input feature that `gilrs` can't provide (for example, rumble or gyro on a
  specific controller).

## Consequences

- The `ttf-parser` advisory ignore is removed (verified: with defaults off,
  the workspace builds and `ttf-parser` leaves the graph).
- Windowed mode on GNOME Wayland has no title bar until winit adds a
  decoration path we opt into.
- Input-model cleanup (item 1) and grab order (item 2) are code fixes
  scheduled with the `moho_input`/`moho_types` merge, not a platform change.
