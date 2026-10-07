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

## Tech spec

**Design.**
- Root `[workspace.dependencies]`: `winit = { version = "0.30", default-features = false, features = ["rwh_06", "x11", "wayland", "wayland-dlopen"] }`.
- `moho_ui`: `egui-winit = { version = "0.34.3", default-features = false, features = ["clipboard", "wayland", "x11"] }`. The version moves in [#101 ENG-F2-08, graphics stack](https://github.com/WrackedFella/moho/issues/101), not here.
- Delete the 15 declarations listed under Deliverables. Each was re-checked unused on `dev` on 2026-10-07; in `moho_ui`, `ini` appears only in comments. `gilrs` leaves as [ADR-0008](https://github.com/WrackedFella/moho/blob/dev/_todo/adr/0008-keep-winit-for-windowing-and-input.md) says; ENG-F12 adds it back in the crate that consumes it.
- `deny.toml`: delete the RUSTSEC-2026-0192 entry.
- Lockfile: `cargo update -p chacha20 --precise 0.10.2`. Don't run a blanket `cargo update`, so the lockfile diff stays reviewable.
- Delete the stale RAUI comment block in `moho_ui/Cargo.toml`.
- Checked on 2026-10-07 against a copy of `dev`: with both manifests changed, `cargo check --workspace --all-targets` passes and `cargo tree --workspace --target all -e all -i ttf-parser` finds no match.

**Out of scope.**
- Declarations that are still used: `bincode` ([#94](https://github.com/WrackedFella/moho/issues/94)), `log`/`env_logger` in the root ([#98](https://github.com/WrackedFella/moho/issues/98)), `crossbeam-channel` ([#99](https://github.com/WrackedFella/moho/issues/99)), `pollster` in `moho_renderer` ([#100](https://github.com/WrackedFella/moho/issues/100)), `noise` in `moho_game` ([#105](https://github.com/WrackedFella/moho/issues/105)).
- `phf` (ENG-F12), the `ini` 2.0 bump, and `moho_physics`'s `edition = "2021"`.
- Any other lockfile update, and any egui or wgpu version change.

**Test map.** The change touches manifests only, so there is no behavior to unit-test. Each criterion is a command the PR runs and quotes:
| Criterion | Proof | Gate class |
|---|---|---|
| Declarations absent; `just check` passes | `just check` | glue |
| `ttf-parser` gone; ignore removed | `cargo tree --workspace --target all -e all -i ttf-parser` reports no match; `deny.toml` diff | glue |
| No yanked crate | `just deny` output has no `yanked` line | glue |

**Gate class:** glue.

**Risks.**
- Verification needs a person at a desktop with X11 and Wayland sessions (title bar, clipboard). An agent can't sign off on that step.
- [#94](https://github.com/WrackedFella/moho/issues/94), [#98](https://github.com/WrackedFella/moho/issues/98), [#99](https://github.com/WrackedFella/moho/issues/99) and [#100](https://github.com/WrackedFella/moho/issues/100) edit the same manifests. Land this card first; the later cards rebase cleanly.

## Verification

- The window opens and resizes on X11 and Wayland; on Wayland it has no
  client-side title bar (accepted cost, ADR-0008).
- Clipboard copy and paste work in the console.
