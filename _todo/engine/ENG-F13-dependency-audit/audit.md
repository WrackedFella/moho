# ENG-F13 — Dependency audit

Verdicts per the [rubric](https://github.com/WrackedFella/moho/issues/83), 2026-10-07. **Econ** =
economical verdict, **Lean** = lean-engine verdict. **Call** is the user's
decision (ENG-F13-05); blank until made. Upgrades and removals of unused
declarations are follow-ups, not verdicts. Evidence: appendices A–D.

## Decisions — existing dependencies

| Crate | Econ | Lean | Reason | Lean cost (when verdicts differ) | Follow-up regardless of call | Call |
|---|---|---|---|---|---|---|
| `wgpu` | keep | keep | Green light: 39M dl, released 2026-08 | — | 29→30 once egui allows (watchlist may be stale) | keep (accepted 2026-10-07) |
| `naga` (dev) | keep | keep | Green light; wgpu release train | — | Upgrade with wgpu | keep (accepted 2026-10-07) |
| `egui` | keep | keep | Green light: 25M dl, released 2026-09 | — | 0.34→0.36 | keep (accepted 2026-10-07) |
| `egui-winit` | keep | keep | Green light; `ttf-parser` advisory handled by ENG-F2-03 | — | ENG-F2-03 | keep (accepted 2026-10-07) |
| `egui-wgpu` | keep | keep | Green light; 0.36.2 out, re-check the wgpu-30 block | — | 0.34→0.36 | keep (accepted 2026-10-07) |
| `rapier3d` | keep | keep | Green light; no realistic homebrew for physics | — | 0.32→0.36; may unpin glam and clear `paste` (ENG-F2-04) | keep (accepted 2026-10-07) |
| `rodio` | keep | keep | Green light; MPL-2.0 codecs fine unmodified | — | Own gain/pan math lands with ENG-F16 | keep (accepted 2026-10-07) |
| `glam` | keep | keep | Green light: 156M dl | — | 0.30→0.34 after rapier3d | keep (accepted 2026-10-07) |
| `serde` | keep | keep | Green light | — | Remove from moho_ui only; ENG-F2-03 wrongly lists moho_core | keep (accepted 2026-10-07) |
| `bytemuck` | keep | keep | Green light; GPU layouts | — | Remove from root, moho_core (ENG-F2-03) | keep (accepted 2026-10-07) |
| `rand` | keep | keep | Green light; one `rand::random` call | — | `cargo update -p chacha20` (yanked) | keep (accepted 2026-10-07) |
| `thiserror` | keep | keep | Green light; standards mandate it; v1 stays via winit anyway | — | — | keep (accepted 2026-10-07) |
| `bincode` | replace | replace | Decided: postcard, ADR-0006 | — | ENG-F2-01 | replace (accepted 2026-10-07) |
| `winit` | keep | keep | Decided: ADR-0008 | — | ENG-F2-03 default features | keep (accepted 2026-10-07) |
| `noise` | homebrew | homebrew | Red flags: last release 2024-03, duplicate `rand` 0.8 | 2D Perlin only, ~60–90 lines; tests: range, zero at lattice, seed determinism, continuity, golden values. Terrain changes per seed (OK pre-v1.0) | Remove from moho_core (ENG-F2-03) | homebrew (accepted 2026-10-07) |
| `log` | replace | replace | Red flag: blocks structured spans; `tracing` adopted (row below) | ~165 macro call sites, migrate gradually; `tracing-log` bridges deps | — | replace (accepted 2026-10-07) |
| `env_logger` | replace | replace | Follows `tracing`; 27 transitive pkgs | `tracing-subscriber` fmt + EnvFilter, one init site | Remove moho_audio dev decl | replace (accepted 2026-10-07) |
| `crossbeam-channel` | keep | replace | Green light (1 transitive pkg, 0.4 s) | `std::sync::mpsc` covers unbounded/send/try_recv/try_iter/recv_timeout in 12 files; loses `select!` if the job pool needs it | — || replace → std `mpsc` |
| `ini` | keep | replace | No red flag | `toml` (planned anyway) for prefs: file format changes, prefs reset or migrate, `prefs-format.md` rewritten; needs serde in moho_core | Remove moho_ui decl || **keep** (INI is the format players and tools expect) |
| `phf` | keep | homebrew | Green light (0.1 s) | 2 tables, 69 entries → `match` fns; drops 9 transitive pkgs + `phf_shared` dup; tests: every key round-trips | — || homebrew (`match`) |
| `pollster` | keep | homebrew | Green light (0 deps) | 2 `block_on` calls in renderer device init → ~25-line `block_on` (`std::task::Wake` + park); tests: ready and pending futures complete | 0.4→1.0.1 bump if kept || homebrew (`block_on`) |
| `once_cell` | drop | drop | Unused; std `LazyLock`/`OnceLock` | — | ENG-F2-03 | drop (accepted 2026-10-07) |
| `gilrs` | keep | keep | Decided: ADR-0008; unused until ENG-F12 | — | — | keep (accepted 2026-10-07) |
| `proptest` (dev) | keep | keep | Green light; `rand` 0.9 dup is dev-only | — | — | keep (accepted 2026-10-07) |
| `criterion` (dev) | keep | keep | Green light; no small homebrew for stats | — | — | keep (accepted 2026-10-07) |
| `tempfile` (dev) | keep | keep | Green light; homebrew below third-use bar | — | Remove moho_renderer dev decl | keep (accepted 2026-10-07) |

## Decisions — planned additions

| Need (consumer) | Econ | Lean | Reason | Call |
|---|---|---|---|---|
| Gamepad (ENG-F12) | `gilrs` | `gilrs` | Confirms ADR-0008; homebrew is per-OS FFI | `gilrs` (accepted 2026-10-07) |
| Scene format (ENG-F14) | glTF via `gltf` 1.4 | same | Confirms glTF 2.0; 0 unsafe; note: one owner, last release 2024-05, repo active | glTF via `gltf` 1.4 (accepted 2026-10-07) |
| Image decoding (ENG-F14) | `image` (png) | same | Already in tree; KTX2/Basis deferred, no consumer | `image` (png) (accepted 2026-10-07) |
| Logging (engine-wide) | `tracing` + `tracing-subscriber` | same | Spans and structured fields; already in tree | `tracing` + `tracing-subscriber` (accepted 2026-10-07) |
| Positional audio (ENG-F16) | keep `rodio` | `rodio` + own gain/pan (~150–250 lines) | rodio's spatial model is crude; `kira` is the fallback (duplicates glam/cpal/symphonia) | `rodio` + own gain/pan (2026-10-07) |
| Data/mod format (ENG-F19) | `toml` | same | Override-by-id is a table merge; `ron` if enum-heavy | `toml` (accepted 2026-10-07) |

## Where each call is carried out

| Call | Owner |
|---|---|
| `bincode` → postcard | [ENG-F2-01](https://github.com/WrackedFella/moho/issues/94) |
| Unused declarations, `once_cell`, yanked `chacha20` | [ENG-F2-03](https://github.com/WrackedFella/moho/issues/96) (list corrected) |
| `log`/`env_logger` → `tracing` | [ENG-F2-05](https://github.com/WrackedFella/moho/issues/98) |
| `crossbeam-channel` → std | [ENG-F2-06](https://github.com/WrackedFella/moho/issues/99) |
| `pollster` → own `block_on` | [ENG-F2-07](https://github.com/WrackedFella/moho/issues/100) |
| wgpu/naga/egui upgrade | [ENG-F2-08](../ENG-F2-dependency-upgrades/ENG-F2-08-graphics-stack-current.md) |
| rapier3d/glam upgrade, `paste` | [ENG-F2-09](https://github.com/WrackedFella/moho/issues/102), [ENG-F2-04](https://github.com/WrackedFella/moho/issues/97) |
| `noise` → in-house Perlin | [ENG-F2-10](../ENG-F2-dependency-upgrades/ENG-F2-10-terrain-noise-in-house.md) |
| `phf` → `match` | [ENG-F12](../ENG-F12-input-actions/_feature.md) (key naming is reworked there) |
| Planned additions | The consuming features (ENG-F12, F14, F16, F19) adopt them; ENG-F16 owns the gain/pan layer |
| Upgrade cost of wgpu/egui | [ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md) (proposed) |

## Alternatives considered for the largest stacks

Recorded for reference; no verdict changes.

- **wgpu:** no Rust-native peer with its maturity and reach. `ash` (raw Vulkan) is stable but unsafe-heavy and Vulkan-only; `glow` (OpenGL) is stable but a step back; SDL3 GPU, `bgfx`, `sokol` are mature C libraries with young bindings and a C toolchain; `vulkano`, `blade`, `miniquad` are less proven. Churn is answered by containment ([ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md)) and the upgrade policy in [ENG-F2](../ENG-F2-dependency-upgrades/_feature.md).
- **rapier3d:** the only mature pure-Rust engine. Jolt (AAA-proven C++, young bindings, C++ build step) and PhysX 5 (stalled `physx` bindings) are the non-Rust options; `avian3d` requires Bevy's ECS. `parry3d` alone (collision queries, no dynamics) is a lean option if [ENG-F15](../ENG-F15-physics-queries-and-bodies/_feature.md) needs no rigid-body dynamics. Already contained in `moho_physics`.

## Findings outside the verdicts

- [ENG-F2-03](https://github.com/WrackedFella/moho/issues/96)'s unused list is wrong on `serde` in moho_core (used in 3 files) and misses `ini` (moho_ui), `env_logger` (moho_audio dev), `tempfile` (moho_renderer dev).
- ENG-F2's watchlist says wgpu 30 waits on egui-wgpu; egui-wgpu 0.36.2 has shipped. Re-verify.
- Prefs parsing falls back to defaults on every error, against the "never silently drop" standard; routed to a separate planning session.

## Appendix A. Facts (ENG-F13-01)


Collected 2026-10-07. Sources: `cargo metadata`, `cargo tree` (`--target all`), `cargo deny check advisories`, one clean `cargo build --workspace --all-targets --timings` (26.8 s wall, 453 units, separate target dir), crates.io API, grep of `src/`, `tests/`, `benches/` per declaring crate. Facts only.

| Crate | Resolved | Licence | Transitive pkgs | Own compile s | Max version / last release | Downloads total / 90-day | Owners | Advisories | Duplicates contributed | Decided elsewhere |
|---|---|---|---|---|---|---|---|---|---|---|
| `bincode` | v2.0.1 | MIT | 3 | 0.7 | 3.0.0 / 2025-12-16 | 337,101,587 / 66,797,111 | 1 | RUSTSEC-2025-0141 unmaintained (ignored → ENG-F2-01) | none | Replace with postcard — ADR-0006 / ENG-F2-01 |
| `bytemuck` | v1.25.2 | Zlib OR Apache-2.0 OR MIT | 5 | 0.2 | 1.25.2 / 2026-07-19 | 401,028,392 / 107,703,366 | 2 | none | none |  |
| `criterion` | v0.8.2 | Apache-2.0 OR MIT | 2 | 2.8 | 0.8.2 / 2026-02-04 | 299,469,151 / 62,603,621 | 4 | none | none |  |
| `crossbeam-channel` | v0.5.16 | MIT OR Apache-2.0 | 1 | 0.4 | 0.5.17 / 2026-09-05 | 626,891,752 / 125,968,614 | 4 | none | none |  |
| `egui` | v0.34.3 | MIT OR Apache-2.0 | 54 | 3.6 (+epaint 1.6) | 0.36.2 / 2026-09-08 | 25,414,857 / 5,951,151 | 2 | none | none |  |
| `egui-wgpu` | v0.34.3 | MIT OR Apache-2.0 | 146 | 0.3 | 0.36.2 / 2026-09-08 | 13,097,069 / 4,337,730 | 2 | none | none |  |
| `egui-winit` | v0.34.3 | MIT OR Apache-2.0 | 275 | 0.5 | 0.36.2 / 2026-09-08 | 21,428,323 / 5,231,563 | 2 | RUSTSEC-2026-0192 `ttf-parser` (ignored → ENG-F2-03) | none |  |
| `env_logger` | v0.11.11 | MIT OR Apache-2.0 | 27 | 0.3 | 0.11.11 / 2026-06-25 | 580,094,779 / 83,582,444 | 6 | none | none |  |
| `gilrs` | not resolved (optional, off) | MIT OR Apache-2.0 (crates.io) | — | 0 | 0.11.2 / 2026-05-30 | 8,904,356 / 1,761,718 | 1 | none | none | Gamepad crate — ADR-0008 |
| `glam` | v0.30.10 | MIT OR Apache-2.0 | 4 | 5.7 | 0.34.1 / 2026-10-06 | 155,652,707 / 52,113,481 | 1 | none | none |  |
| `ini` | v1.3.0 | MIT OR LGPL-3.0-or-later | 1 | 0.0 | 2.0.0 / 2026-09-06 | 1,848,505 / 355,224 | 1 | none | none |  |
| `log` | v0.4.33 | MIT OR Apache-2.0 | 1 | 0.1 | 0.4.34 / 2026-08-22 | 1,334,750,079 / 310,249,780 | 5 | none | none |  |
| `naga` | v29.0.4 | MIT OR Apache-2.0 | 33 | 7.9 | 30.0.1 / 2026-08-22 | 43,396,863 / 13,972,273 | 3 | none | bit-vec 0.9 |  |
| `noise` | v0.9.0 | Apache-2.0/MIT | 6 | 1.0 | 0.9.0 / 2024-03-23 | 2,725,436 / 401,161 | 5 | none | brings rand 0.8 / rand_core 0.6 |  |
| `once_cell` | v1.21.4 | MIT OR Apache-2.0 | 0 | 0.1 | 1.21.4 / 2026-03-12 | 1,336,650,906 / 304,177,427 | 2 | none | none |  |
| `phf` | v0.13.1 | MIT | 9 | 0.1 | 0.14.0 / 2026-06-21 | 592,193,024 / 171,175,802 | 3 | none | phf_shared (two consumers) |  |
| `pollster` | v0.4.0 | Apache-2.0/MIT | 0 | 0.1 | 1.0.1 / 2026-07-10 | 36,029,264 / 14,079,832 | 1 | none | none |  |
| `proptest` | v1.11.0 | MIT OR Apache-2.0 | 2 | 2.6 | 1.11.0 / 2026-03-24 | 203,460,453 / 53,699,945 | 3 | none | none |  |
| `rand` | v0.10.2 v0.8.7 v0.9.5 | MIT OR Apache-2.0 | 7 1 | 1.5 | 0.10.3 / 2026-09-20 | 1,850,319,975 / 489,288,813 | 2 | yanked `chacha20` 0.10.1 (warning) | rand 0.8.7 via `noise`; rand 0.9.5 via `proptest` (dev); workspace uses 0.10 |  |
| `rapier3d` | v0.32.0 | Apache-2.0 | 56 | 2.4 (+parry3d 2.6, nalgebra 5.2) | 0.36.0 / 2026-09-25 | 1,765,996 / 447,841 | 1 | RUSTSEC-2024-0436 `paste` (ignored → ENG-F2-04) | bit-vec 0.8, rustc-hash 1.x, downcast-rs |  |
| `rodio` | v0.22.2 | MIT OR Apache-2.0 | 125 | 1.3 | 0.22.2 / 2026-03-05 | 12,388,523 / 3,352,320 | 2 | none | none |  |
| `serde` | v1.0.229 | MIT OR Apache-2.0 | 6 | 0.6 | 1.0.229 / 2026-07-18 | 1,493,245,909 / 345,979,077 | 2 | none | none |  |
| `tempfile` | v3.27.0 | MIT OR Apache-2.0 | 2 | 0.3 | 3.27.0 / 2026-03-11 | 866,799,506 / 200,254,187 | 1 | none | none |  |
| `thiserror` | v1.0.69 v2.0.20 | MIT OR Apache-2.0 | 5 5 | 0.5 | 2.0.21 / 2026-09-23 | 1,578,959,413 / 406,778,067 | 1 | none | thiserror 1.x via winit → calloop |  |
| `wgpu` | v29.0.4 | MIT OR Apache-2.0 | 120 | 1.7 (+wgpu-core 4.1, wgpu-hal 2.7, ash 11.5) | 30.0.1 / 2026-08-22 | 39,055,462 / 11,992,882 | 3 | none | none |  |
| `winit` | v0.30.13 | Apache-2.0 | 168 | 2.9 (+x11rb-protocol 11.6, wayland-protocols 5.3) | 0.30.13 / 2026-09-04 | 56,542,630 / 11,257,519 | 5 | RUSTSEC-2026-0192 `ttf-parser` via egui-winit/ab_glyph (ignored → ENG-F2-03) | calloop, smithay-client-toolkit, objc2, windows-sys ×5 (platform stack) | Keep — ADR-0008 |

#### Call sites (files referencing the crate, per declaring crate)

- `bincode`: moho (normal): 1 — save; moho_core (normal): 1 — voxel/grid/paletted; moho_game (normal): 4 — biome,game_clock,scene_builders,scene_persistence; moho_render_api (normal): 1 — persistence; moho_renderer (normal): **unused**; moho_ui (normal): **unused**
- `bytemuck`: moho (normal): **unused**; moho_core (normal): **unused**; moho_render_api (normal): 2 — instance,material; moho_renderer (normal): 9 — gpu_types,mesh_renderer,renderer,render_ops/camera_ops
- `criterion`: moho (dev): 1 — event_bus_bench
- `crossbeam-channel`: moho (normal): 8 — app/event_setup,app/generation_job,app/initializer,app/input_state; moho_ui (normal): 2 — adapter,adapter/event_routing
- `egui`: moho_ui (normal): 28 — adapter,adapter/gpu_ops,adapter/rendering,input_handling
- `egui-wgpu`: moho_ui (normal): 2 — adapter,adapter/gpu_ops
- `egui-winit`: moho_ui (normal): 1 — adapter
- `env_logger`: moho_audio (dev): **unused**; moho (normal): 1 — app/initializer
- `gilrs`: moho_ui (normal,optional): **unused**
- `glam`: moho (normal): 13 — app/autosave,app/camera,app/chunk_streamer,app/event_loop/event_processor; moho_core (normal): 25 — events,events/bus,events/types,events/types/debug; moho_game (normal): 12 — actors,controller,events,game_clock; moho_physics (normal): 1 — world; moho_render_api (normal): **unused**; moho_renderer (normal): 11 — buffer_manager,instance_collector,lib,lights
- `ini`: moho_core (normal): 1 — prefs; moho_ui (normal): **unused**
- `log`: moho (normal): 16 — app/audio_init,app/autosave,app/chunk_streamer,app/event_loop/event_processor; moho_audio (normal): 2 — audio_cache,audio_system; moho_core (normal): 5 — events/bus,voxel/jobs,voxel/light_jobs,voxel/light_system; moho_game (normal): 2 — scene_builders,scene_persistence; moho_physics (normal): 1 — world; moho_renderer (normal): 9 — buffer_manager,builder,device,renderer; moho_ui (normal): 1 — adapter
- `naga`: moho_renderer (dev): 1 — tests/shaders_validation
- `noise`: moho_core (normal): **unused**; moho_game (normal): 2 — biome,scene_builders
- `once_cell`: moho_ui (normal): **unused**
- `phf`: moho_core (normal): 1 — prefs/key_names; moho_ui (normal): 1 — screens/settings/key_mapping
- `pollster`: moho (normal): **unused**; moho_renderer (normal): 1 — device
- `proptest`: moho_core (dev): 1 — voxel/chunk_store
- `rand`: moho (normal): **unused**; moho_ui (normal): 1 — screens/new_world
- `rapier3d`: moho_physics (normal): 2 — lib,world
- `rodio`: moho_audio (normal): 1 — audio_system
- `serde`: moho_core (normal): 3 — prefs,voxel/grid,voxel/grid/paletted; moho_game (normal): 3 — biome,game_clock,scene_builders; moho_ui (normal): **unused**
- `tempfile`: moho (dev): 2 — app/autosave,app/world_generator; moho_renderer (dev): **unused**
- `thiserror`: moho_audio (normal): 1 — error; moho_renderer (normal): 1 — lib
- `wgpu`: moho (normal): 1 — app/renderer_setup; moho_renderer (normal): 15 — device,lib,mesh_renderer,pipeline; moho_ui (normal): 3 — adapter,adapter/gpu_ops,lib
- `winit`: moho (normal): 7 — app/event_loop/event_processor,app/event_loop/frame_processor,app/event_loop/window_event_handler,app/event_loop/window_manager; moho_input (normal): 1 — lib; moho_renderer (normal): 5 — builder,device,lib,renderer; moho_ui (normal): 4 — adapter,screens/menu,screens/settings,screens/settings/keybind_capture

#### Unused declarations vs ENG-F2-03

- `bincode`: moho_renderer, moho_ui unused (agrees); moho_core uses it
- `bytemuck`: root + moho_core unused (ENG-F2-03 agrees)
- `env_logger`: moho_audio dev unused (not in ENG-F2-03)
- `gilrs`: unused (agrees)
- `glam`: moho_render_api unused (agrees)
- `ini`: moho_ui unused (not in ENG-F2-03)
- `noise`: moho_core unused (agrees)
- `once_cell`: unused (agrees)
- `pollster`: root unused (agrees)
- `rand`: root unused (agrees)
- `serde`: moho_ui unused (agrees); **moho_core uses it — ENG-F2-03 wrong**
- `tempfile`: moho_renderer dev unused (not in ENG-F2-03)

#### Notes

- Lockfile lists 18 `glam` versions (0.14–0.32) as optional conversion targets of `nalgebra`; only 0.30.10 is built.
- `wgpu`/`naga` 30 and `glam` 0.34 exist; upgrades blocked per ENG-F2 watchlist (egui-wgpu, rapier3d).
- `rapier3d` is 4 minors behind (0.32 → 0.36); `ini` has a 2.0.0 release; `pollster` has 1.0.1.
- Top compile units overall: x11rb-protocol 11.6 s, ash 11.5 s, rustix 8.7 s, naga 7.9 s, syn 7.0 s, read-fonts 6.1 s, glam 5.7 s, wayland-protocols 5.3 s, nalgebra 5.2 s.

## Appendix B. Large-crate triage (ENG-F13-02)


Facts cited from 01-facts.md ("F:"). Green light = rubric 1.

| Crate | Green? | Econ | Lean | Reason | Follow-up |
|---|---|---|---|---|---|
| wgpu 29.0.4 | Yes | keep | keep | F: 39M dl, 12M/90d, 3 owners, release 2026-08-22, no advisories, no duplicates; no realistic homebrew | Upgrade 29→30 via ENG-F2 watchlist (blocked on egui-wgpu); not a verdict change |
| naga 29.0.4 (dev) | Yes | keep | keep | F: 43M dl, 14M/90d, 3 owners, same wgpu-team release train; used by one validation test; adds bit-vec 0.9 dup (transitive, wgpu-shared, minor) | Upgrade with wgpu (30.0.1) |
| egui 0.34.3 | Yes | keep | keep | F: 25M dl, 6M/90d, 2 owners, release 2026-09-08, no advisories, 28 call sites | Upgrade 0.34→0.36 via ENG-F2 (egui-wgpu 0.36 will unblock wgpu 30); ENG-F2-02 deprecation migration |
| egui-winit 0.34.3 | Yes | keep | keep | F: 21M dl, 5M/90d; RUSTSEC-2026-0192 ttf-parser is transitive (ab_glyph), already ignored and removed by ENG-F2-03 (default-features off), so not a crate flag | ENG-F2-03 (existing) |
| egui-wgpu 0.34.3 | Yes | keep | keep | F: 13M dl, 4.3M/90d, same emilk release train; 2 files of use. Cause of the wgpu pin, but 0.36.2 exists (2026-09-08); verify its wgpu requirement | ENG-F2: bump egui*, then check wgpu 30 |
| rapier3d 0.32.0 | Yes | keep | keep | F: Apache-2.0, 1.77M dl, 448k/90d, 0.36.0 released 2026-09-25 (active), no homebrew for physics. Owner note below | ENG-F2: upgrade 0.32→0.36 (re-check glam pin and paste); ENG-F2-04 stays |
| rodio 0.22.2 | Yes | keep | keep (own gain/pan math per 04-planned) | F: 12M dl, 3.4M/90d, 2 owners, MIT/Apache; MPL-2.0 symphonia is accepted by ADR-0007 for unmodified use and allowed in deny.toml, so not a flag | none (ENG-F16 homebrew gain layer is in 04-planned) |
| glam 0.30.10 | Yes | keep | keep | F: 156M dl, 52M/90d, release 2026-10-06 (yesterday), no advisories; one owner but ubiquitous | Upgrade 0.30→0.34 blocked on rapier3d (glamx); resolves with rapier 0.36 upgrade if it moves glam |
| serde 1.0.229 | Yes | keep | keep | F: 1.49B dl, 346M/90d, 2 owners, no advisories; also the ADR-0006/postcard/toml base | Remove unused declaration in moho_ui only (ENG-F2-03); NOTE F: ENG-F2-03 wrongly lists moho_core as unused (it uses serde) |
| bytemuck 1.25.2 | Yes | keep | keep | F: 401M dl, 108M/90d, release 2026-07-19, 5 pkgs, 0.2 s compile; used by moho_render_api, moho_renderer (GPU layouts) | ENG-F2-03 (remove from root, moho_core) |
| rand 0.10.2 | Yes (see note) | keep | keep | F: 1.85B dl, 489M/90d, 0.10.3 exists. Only use is `rand::random::<u64>()` in moho_ui new_world (1 call) | `cargo update -p chacha20` (0.10.1 yanked, 0.10.2 is not); then re-run deny. Not a verdict change |
| noise 0.9.0 | **No** | **homebrew** | **homebrew** | F: last release 2024-03-23, 2.7M dl / 401k 90d, brings rand 0.8/rand_core 0.6 duplicate; 2D Perlin only | Card under ENG-F2 (below) |

#### Flag notes (card requires)
- **glam pin:** glam 0.30 is built only because rapier3d 0.32 depends on it via `glamx`; 0.34.1 exists. Upgrade, not a verdict. Lockfile's 18 glam versions are nalgebra optional conversion targets, not built (F notes).
- **paste:** RUSTSEC-2024-0436 comes via rapier3d -> simba/nalgebra/glamx/parry3d. Unmaintained, compile-time proc-macro. rapier3d is 4 minors behind; ENG-F2-04 should be tied to the 0.36 upgrade. Forking rapier/simba rejected (large, unsafe-heavy math stack).
- **wgpu/naga 29->30:** still blocked per watchlist, but egui-wgpu 0.36.2 is out (F), so first step is the egui 0.34->0.36 bump; wgpu 30 follows only if 0.36 targets it. Check before assuming the block persists.
- **rodio MPL-2.0:** ADR-0007 allows MPL-2.0 for unmodified upstream crates; deny.toml lists it. No red flag. Only a symphonia fork would trigger source-publication.
- **rand chacha20:** `chacha20` 0.10.1 is yanked (deny warning). crates.io has 0.10.2 unyanked, so `cargo update -p chacha20` fixes it. rand 0.9.5 is via proptest (dev-only), outside our control; clears when proptest moves.
- **rapier3d owner:** crates.io lists one owner: user `sebcrozet` (Sébastien Crozet, founder of Dimforge, the org behind rapier/parry/nalgebra). Not an org account, so single-owner on paper, but the project is org-run, heavily used by Bevy, and shipped 0.36.0 on 2026-09-25. Judged not a bus-factor flag under "stale single-maintainer"; it is a note only.

#### noise (red flag: stale + duplicate rand 0.8)
Flags: last release 2024-03 (2.5 yrs; 5 owners so not single-maintainer); duplicate rand 0.8/rand_core 0.6 (F). Weight (6 pkgs, 1.0 s) is small. Domain: strategy-line terrain gen (ADR-0010 puts voxels/terrain on the strategy line).
- Econ: **homebrew** (flags apply; no cheaper swap found: alternatives are other small crates with unknown rand/maintenance profiles). Could accept "keep" if user tolerates one dup; verdict per rubric is replace.
- Lean: **homebrew**. Same.
- Used surface: only `Perlin::new(u32 seed)` and `NoiseFn::get([f64; 2])` (2D, never 3D). Call sites: moho_game/src/biome.rs (biome field, line ~159), moho_game/src/scene_builders.rs (octave loop `sample_surface_height` ~177, plus a Perlin built at lines 135 and 306). moho_core declares it unused.
- Homebrew size: ~60-90 lines in moho_game (seeded permutation table via a small hash or splitmix64 shuffle, fade curve, gradient hash, 2D lerp), own `Perlin2` type with `get(x, z) -> f64`. No rand dependency.
- Proving tests: range stays within about [-1, 1] over a grid; zero at integer lattice points (code relies on this: "Perlin = 0 at integer" comment near scene_builders:221); same seed gives identical output, different seeds differ; continuity (small step gives small delta); mean near 0 over large sample; a golden-value snapshot pinning N sample points (the terrain contract).
- Cost/risk: output will not match noise 0.9's table, so terrain for a given seed changes. Existing saves hold chunks so they load; unsaved/ungenerated chunks of an old seed would mismatch at seams. Pre-v1.0 so acceptable (ADR-0006 already breaks old saves); record it in the card. Existing terrain tests that assert specific heights/biomes will need re-baselining (check moho_game tests).
- Fork option not pursued: noise is Apache-2.0/MIT, ~no unsafe, but forking just to drop `rand` costs more than the 80-line homebrew.
- Follow-up: ENG-F2 card "noise replaced with owned 2D Perlin" (also then drop the moho_core declaration via ENG-F2-03).

## Appendix C. Small-crate analysis (ENG-F13-03)


Facts cited from 01-facts.md ("F01"); planned decisions from 04-planned.md ("F04"). Usage verified by grep of the code on 2026-10-07.

#### 1. Summary

| Crate | Used surface | Economical | Lean | Reason (fact) | Replacement |
|---|---|---|---|---|---|
| `once_cell` | none: declared in `moho_ui`, zero uses (F01 unused; ENG-F2-03 removes it) | drop | drop | Unused; 0 transitive pkgs but also 0 value | std `LazyLock`/`OnceLock` if ever needed |
| `crossbeam-channel` | `unbounded()`, `send`, `try_recv`, `try_iter`, `recv_timeout`, Sender/Receiver types; 10 files in `moho` + 2 in `moho_ui`. No `select!`, no `bounded`, no cloned Receivers | keep | replace | Green light: 627M dl, 4 owners, 1 transitive pkg, 0.4 s, no advisory (F01). Lean: surface is the std subset | `std::sync::mpsc` (`channel`, `try_iter`, `recv_timeout` all exist) |
| `ini` | `ini::macro_safe_read` in one file (`moho_core/src/prefs/mod.rs`, 6 call sites) -> section/key/string map; `moho_ui` declares it unused | keep | replace | No red flag: MIT election is ADR-0007-compliant, 2.0.0 released 2026-09-06 so maintained, 1.8M dl, 1 owner (F01). Lean: `toml` is already adopted (F04 #6), so prefs can ride it and drop a crate | `toml` + serde `#[serde(default)]` struct (alt: ~50-line homebrew INI reader) |
| `phf` | 2 static tables: `KEY_NAME_TO_CODE` (57 str->u32 entries, `moho_core/prefs/key_names.rs`) and `CODE_TO_LABEL_MAP` (12 u32->str, `moho_ui/screens/settings/key_mapping.rs`); only `.get()` | keep | homebrew | Green light: 592M dl, 171M/90d, MIT, 0.1 s, no advisory. Lean: 9 transitive pkgs + `phf_shared` duplicate (F01) for 69 entries read one at a time | `match` fns in the same files (std) |
| `log` | ~165 `log::{debug,info,warn,error}!` calls across 7 crates (F01: 36 files); no spans/structured fields | replace | replace | Red flag: blocks a planned feature, spans + structured fields required (F04 #4 adopts `tracing`). `log` itself is green (1.33B dl, 0 advisories) and stays in the tree via wgpu/winit/egui, bridged by `tracing-log` | `tracing` (+ `tracing-subscriber`, `tracing-log`) |
| `env_logger` | one init call, `moho/src/app/initializer.rs:147` (`Builder::from_env(...default_filter_or("info")).try_init()`); `moho_audio` dev decl unused | replace | replace | Follows `log` -> `tracing` (F04 #4). 27 transitive pkgs (F01) and its job is done by `tracing-subscriber` `fmt`+`env-filter` | `tracing-subscriber` (`EnvFilter` keeps `RUST_LOG`) |
| `thiserror` | `#[derive(Error)]` on 3 enums: `AudioError` (7 variants, one `#[from]`), `RendererInitError`, `FrameError` | keep | keep | Green light: 1.58B dl, 407M/90d, MIT/Apache, 0.5 s (F01). Workspace standard mandates it for library errors. Dropping saves nothing: thiserror 1.x stays via winit->calloop, and syn/proc-macro2 are shared | none |
| `pollster` | `pollster::block_on` x2, both in `moho_renderer/src/device.rs` (`request_adapter`, `request_device`); root decl unused (ENG-F2-03) | keep | homebrew | Green light for keep: 36M dl, 0 deps, 0.1 s, no advisory; 1.0.1 exists (F01). Lean: one function, ~25 lines, std-only | own `block_on` via `std::task::Wake` + `thread::park` |
| `proptest` (dev) | 1 test in `moho_core/voxel/chunk_store.rs` (`iteration_is_unique_and_ascending...`) | keep | keep | 203M dl, 3 owners, dev-only. Flag: brings `rand` 0.9.5 duplicate (F01), not shipped; workspace already carries 3 rand versions via `noise` (0.8) | none (shrinking is not worth owning) |
| `criterion` (dev) | `benches/event_bus_bench.rs` only (`bench_function`, `benchmark_group`, `bench_with_input`; `html_reports` feature) | keep | keep | 299M dl, 4 owners, 2.8 s dev-only compile, no advisory (F01). Stats/warmup/outlier handling has no small homebrew | none (could trim `html_reports` if plotters weight matters) |
| `tempfile` (dev) | `tempfile::tempdir()` in 2 test modules (`app/autosave.rs`, `app/world_generator.rs`); `moho_renderer` dev decl unused | keep | keep | 867M dl, 200M/90d, 2 transitive pkgs, 0.3 s, no advisory (F01). std alternative is a ~25-line `TempDir` guard but below the third-use bar and race-safe naming is subtle | none |

Housekeeping (not decided here; flagged for ENG-F2-03 since F01 says they are missing): `ini` unused in `moho_ui`, `env_logger` dev unused in `moho_audio`, `tempfile` dev unused in `moho_renderer`, `once_cell` already listed.
Note: F01/ENG-F2-03 lists `serde` in `moho_core` as unused; F01 shows prefs uses it. The `ini` -> `toml` route below depends on it staying.

#### 2. Rows where economical != lean, or not keep

##### crossbeam-channel: keep / replace (std::sync::mpsc)
- Surface: `unbounded`, `send`, `try_recv`, `try_iter`, `recv_timeout`; 5 event channels (`event_setup.rs`), generation job, input dispatcher, 2 test sites. No `select!`, `bounded`, `Receiver::clone`, or multi-consumer.
- Homebrew size: 0 new lines; ~25 mechanical edits (`unbounded::<T>()` -> `mpsc::channel::<T>()`, import swaps, `Receiver` type paths in `initializer.rs`, `main.rs`, `generation_job.rs`, `input_state.rs`, `moho_ui::UiReceiver`).
- Hazards: `mpsc::Receiver` is `!Sync` (check anything holding it behind `Arc`/needing `Sync` — grep found none); the `EventBus` subscriber closures capture `Sender`, which needs `Sync` (true since Rust 1.72). Loses `select!`: if ENG job pool / multi-source waiting arrives later, this reverses.
- Tests proving it: existing `try_recv` tests in `event_setup`, `input_dispatcher`, `event_routing::recv_timeout` stay green; add compile-time `assert_send_sync::<Sender<UiEvent>>()` / `Receiver<_>: Send`; `try_iter` drains FIFO per channel; `try_recv` after sender drop is `Disconnected` (frame loop must not spin); send after receiver drop returns `Err`.

##### ini: keep / replace (toml)
- Surface: one read function returning `section -> key -> Option<String>`; 6 `if let Ok(map) = ini::macro_safe_read` sites that all fall back to defaults on parse error (also swallows errors; at odds with the "never silently drop" standard).
- Target: `toml` 1.x + serde `Deserialize` on `Prefs` (`#[serde(default)]`, typed fields replace hand `get_f32`/parse plumbing), per F04 #6. Costs: file format changes (`config/prefs.ini` -> `prefs.toml`), a one-time migration or reset, wiki `reference/prefs-format.md` rewrite; `toml` brings ~15 pkgs, but F04 adopts it anyway, so net is -1 crate. Bindings (`Ctrl+W`) remain strings.
- Cheaper alt (homebrew INI reader): ~50 lines (sections, `key=value`, `;`/`#` comments, trim, no escapes), keeps format. Not preferred if toml lands.
- Tests: load defaults when file missing; missing section/key -> default; unknown key ignored; malformed value -> typed error naming key (not silent default); save->load round trip equals original; clamping (`shadow_quality` 0-4, `chunks_per_frame` >= 1); legacy `prefs.ini` migration (if kept).
- Optional low-cost step instead: bump `ini` 1.3 -> 2.0.

##### phf: keep / homebrew (std match)
- Surface: 57 name->code entries (many aliases: `UP`/`ARROWUP`) and 12 code->label; lookups only, one key at a time, no iteration, 0.1 s compile.
- Homebrew size: ~70 lines in place of the macro tables (`fn key_code(name:&str)->Option<u32>{match name{"ARROWUP"|"UP"=>Some(0x100),..}}` and a 12-arm label `match`). Net lines roughly equal; drops 9 pkgs and `phf_shared` duplicate. Compiler flags duplicate arms (phf macro also does).
- Tests: every name resolves to the expected code (table-driven); aliases share a code; lookup is case-insensitive through the existing uppercase path; unknown name -> `None`; reverse labels: each labelled code round-trips (`label.to_uppercase()` parses back to the code) so the two tables cannot drift; every code in the label table is reachable from some name.

##### pollster: keep / homebrew
- Surface: 2 `block_on` calls at device init (native only, futures resolve almost immediately).
- Homebrew size: ~25 lines: a `ThreadWaker(Thread)` implementing `std::task::Wake` (`unpark`), `pin!` the future, loop `poll` -> `Pending => thread::park()`. No unsafe.
- Tests: ready future returns without parking; future that returns `Pending` once and wakes from another thread completes; wake before park does not deadlock (park token semantics); panicking future propagates. Lean value is low: the code is what pollster already is; revisit if wgpu 30 or an async job pool makes a runtime appear (then use it instead). Free alternative: bump to pollster 1.0.1.

##### log: replace with tracing (weighed)
- Used surface: plain-string macros (`log::info!`, `debug!`, `warn!`) in 7 crates, plus the single `env_logger` init; ~165 sites (F04: 165).
- `tracing` (F04 #4): MIT, 909M dl, `tracing` 0.1.44 already in tree via calloop, +22 pkgs total for tracing-subscriber, spans and structured fields, `tracing-log` carries wgpu/winit/egui `log` output so no information is lost. RUSTSEC-2025-0055 requires tracing-subscriber >= 0.3.20.
- Keep-log case: no advisory, 0.1 s, but no spans/timings, so frame-stage observability in the user's standards is unreachable. `log` remains as a transitive dep regardless; "replace" means our call sites move, not that `log` leaves `Cargo.lock`.
- Migration: macro syntax is identical (`log::info!("..")` -> `tracing::info!("..")`); then convert hot spots to structured fields incrementally; ~165 mechanical edits. Tests: init is idempotent (`try_init` result handled, not discarded), `RUST_LOG` filter honoured, dependency `log` records appear via `tracing-log`, a span-wrapped frame stage emits a duration field.

##### env_logger: replace with tracing-subscriber
- Surface: one builder call (default filter `info`, `try_init` result discarded with `let _ =`, which also needs an explicit decision per the standards).
- Replacement is `tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).try_init()`, ~5 lines, drops 27 transitive pkgs (F01). Tests: default level is info when `RUST_LOG` unset; env override works; second init does not panic (tests construct `AppInitializer` repeatedly).

## Appendix D. Planned additions (ENG-F13-04)


Data retrieved 2026-10-07: crates.io API (versions, dates, downloads = all-time / last 90 days, owners), GitHub
`pushed_at`, and throwaway crates under the scratchpad (`cargo tree -e normal`, Linux, unique name+version, counts
include proc-macro/build crates; `cargo deny check advisories` against the current RustSec db). Nothing in the repo was edited.
"In tree" = already in `/home/justin/projects/moho/Cargo.lock`.

#### Summary

| Need | Economical | Lean | Licence OK? | One-line reason |
|---|---|---|---|---|
| 1 Gamepad (ENG-F12) | adopt `gilrs` 0.11 | adopt `gilrs` | Yes (MIT/Apache-2.0) | Confirms ADR-0008: 8.9M dl, no advisories, already in the lock, no realistic homebrew (per-OS FFI). |
| 2 Scene format (ENG-F14) | adopt `gltf` 1.4, glTF 2.0 (`default-features = false`, `utils`, `names`, `extras`) | same (homebrew ~1,000 lines is the fallback, not preferred) | Yes (MIT/Apache-2.0) | Confirms glTF; 10.2M dl, 0 unsafe, 19 pkgs without `import`; reject `easy-gltf` (pulls unmaintained `cgmath`). |
| 3 Image decoding (ENG-F14) | none needed beyond `image` (already in tree, `png`); add `jpeg` feature if needed | same; defer KTX2/Basis | Yes (MIT/Apache-2.0, zune-jpeg also Zlib) | `image` 206M dl is in the tree via arboard; marginal cost is ~2 crates for JPEG; a PNG/JPEG homebrew is not small. |
| 4 Tracing (cross-cutting) | adopt `tracing` + `tracing-subscriber` (replaces `log` + `env_logger`; `tracing-log` bridges deps) | same; profiler bridge deferred (`tracing-tracy` when needed, dev-feature only) | Yes (MIT; tracy-client-sys adds BSD-3-Clause) | 909M dl; `tracing` 0.1.44 already in tree; user standard requires spans/structured fields, which `log` cannot give. |
| 5 Audio (ENG-F16) | keep `rodio` 0.22 | keep `rodio` + ~200-line homebrew gain/pan layer | Yes (MIT/Apache-2.0; symphonia MPL-2.0 unchanged) | rodio already has `SpatialPlayer` (crude 1/d^2); our F16 test spec is on gain values, so own the pure math; `kira` is the escape hatch if buses/reverb/occlusion arrive. |
| 6 Data/mod format (ENG-F19) | adopt `toml` 1.x + serde | same | Yes (MIT/Apache-2.0) | Table-key-per-id maps directly to override-by-id; 245M recent dl; comments; span errors; 15 pkgs; RON is the runner-up if defs turn enum-heavy. |

All chosen candidates are under licences already in `deny.toml`. No ADR-0007 amendment is needed. `cargo deny` licence/advisory checks on every
chosen candidate's tree were clean except the probe crate's own "unlicensed" (artifact of the throwaway crate).

---

#### 1. Gamepad input (ENG-F12)

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`gilrs`** | Apache-2.0/MIT | 0.11.2 / 2026-05-30 | 8.9M / 1.76M | 1 (Arvamer; GitLab gilrs-project) | 15 | Single owner, but large and widely used (Bevy's choice). `gilrs-core` has ~83 `unsafe` (platform FFI). Needs `libudev` dev package on Linux (`libudev-sys` is already in our lock). No advisories. | Axes, buttons, hotplug, force feedback, SDL-style mapping DB. Matches F12 "gamepad to game-supplied action". |
| `gamepad` | MIT/Apache | 0.1.6 / 2023-06 | 15k / 0.5k | n/a | n/a | Stale | No |
| `sdl3` | MIT | 0.20.0 / 2026-09 | 226k / 68k | n/a | n/a | C library on 3 OSes; ADR-0008 already rejected | Only if gyro/rumble gaps appear |
| homebrew | n/a | n/a | n/a | n/a | n/a | evdev+udev, XInput/WGI, IOKit/GameController, mapping DB | ~3,000+ lines of unsafe FFI across 3 OSes; no testable-in-CI surface |

- Economical: **adopt `gilrs`**. Green light (large, active, widely used, no flag; the single-owner note is outweighed).
- Lean: **adopt `gilrs`**. No realistic homebrew; surface used is a small event poll loop.
- ADR-0008 confirmed. Already declared optional in `moho_ui`; ENG-F12 should move it to the crate that owns device input and keep the
  `Event -> engine key/axis id` mapping in our adapter (so SDL3 stays a swap).
- Sources: crates.io/gilrs, `cargo tree`, RustSec (no entries).

#### 2. Scene format (ENG-F14)

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`gltf`** (+`gltf-json`) | MIT/Apache-2.0 | 1.4.1 / 2024-05-10 (repo pushed 2026-05-11) | 10.2M / 2.26M | 1 (alteous) | 19 with `default-features=false, features=["utils"]`; 40 with default `import` (adds `image`, base64 0.13 dup, urlencoding) | Last release 2.4 years old, one owner, though main has 2026 commits (panic fixes). 0 `unsafe`. No advisories. | Nodes with names/TRS, meshes, primitives, accessors, GLB blob, materials; `extras` feature exposes Blender custom properties for markers. |
| `easy-gltf` | MIT | 1.1.5 / 2025-03 | 30k / 1.2k | 1 | 48 | **Unmaintained `cgmath` (RUSTSEC-2026-0196)**, tiny adoption, a second math lib | No |
| `goth-gltf` | MIT | 0.3.2 / 2026-09 | 5.8k / 0.2k | n/a | n/a | Tiny adoption | No |
| `russimp` (Assimp) | non-standard | 3.2.1 | 74k | n/a | n/a | C++ build, licence expression non-standard | No |
| homebrew | n/a | n/a | n/a | n/a | serde_json is already in tree | Spec breadth: sparse/strided accessors, GLB, data URIs, extras | ~1,000-1,300 lines for a static-mesh+nodes+extras subset |

- Economical: **adopt `gltf`**. Green light: large, widely used, no advisory; stale release cadence is not a flag against 10M downloads.
- Lean: **adopt `gltf`** with `import` off. Used surface is parse + accessor reads + node walk, but authoring tools emit sparse/strided/normalised
  accessors and GLB variants; owning them is not "small and testable" in the rubric's sense. Without `import`, our code decodes images
  (see need 3) and reads `.glb` or sidecar `.bin` itself, so there is no `base64 0.13` duplicate.
- If lean were pushed to homebrew: ~1,000 lines (GLB chunk parse, JSON via `serde_json`, accessors incl. strides/normalised, node transform
  walk, extras) proven by fixtures exported from Blender (two meshes + one named marker per the F14 exit test, a strided accessor, a nested
  transform, a missing-buffer error naming the file).
- glTF 2.0 recommendation confirmed. Caveat for the FPS line: collider geometry/markers rely on authoring conventions (named nodes + `extras`),
  a decision for ENG-F14, not the crate.
- Features to enable: `names` (default) and `extras`; `KHR_lights_punctual`/`KHR_texture_transform` only when a consumer needs them.

#### 3. Image decoding (ENG-F14 textures)

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`image`** (`png`, optionally `jpeg`) | MIT/Apache-2.0 | 0.25.10 / 2026-03-10 | 206M / 57M | 4 (image-rs) | png only 18; png+jpeg 20; default features 140 | **Already in tree** (arboard, `png` feature only). Old advisories only affect <0.25. Default features are heavy: keep them off. | Decode to RGBA8 for the renderer's upload path |
| `png` direct | MIT/Apache-2.0 | 0.18.1 / 2026-02-14 | 250M / 82M | image-rs | 12 | Same crate `image` uses | PNG only; fine if JPEG never needed |
| `zune-jpeg` / `zune-png` | MIT/Apache/Zlib | 0.5.15 / 0.5.2 | 125M / 0.2M | 1 | n/a | zune-png has low adoption; zune-jpeg is already `image`'s JPEG backend | Use via `image` |
| `ktx2` (+ Basis transcoder) | Apache-2.0 | 0.5.0 / 2026-04-19 | 5.6M / 1.3M | 1 (cwfitzgerald, wgpu team) | 6 | Container parser only. Transcoding needs `basis-universal` (C++ bindings, last release 2023-11, stale) or pre-compressed BCn/ASTC per platform | Premature: no consumer, no texture-memory pressure yet |
| homebrew | n/a | n/a | n/a | n/a | n/a | DEFLATE+filters (PNG) or Huffman/IDCT (JPEG) | PNG alone ~1,500 lines plus fuzzing; not realistic |

- Economical: **none needed beyond `image`** (already present; add `jpeg` if art needs it). Green light.
- Lean: **adopt `image`, `default-features = false, features = ["png"(, "jpeg")]`**. Marginal weight is ~0 (PNG) or 2 crates (`zune-jpeg`, `zune-core`, plus `moxcms`); no small homebrew exists.
- Defer KTX2/Basis to a card when VRAM or load time is measured. Record `ktx2` as the parser candidate then; transcoding is a separate decision.

#### 4. Structured logging / tracing

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`tracing` + `tracing-subscriber`** (`fmt`, `env-filter`, `tracing-log`) | MIT | 0.1.44 / 2025-12; 0.3.23 / 2026-03 | 909M / 212M; 649M / 155M | 3 each (tokio-rs) | 22 total (`regex-automata`, `matchers`, `nu-ansi-term` are the extras) | **`tracing` 0.1.44 already in tree** (via calloop). RUSTSEC-2025-0055 (ANSI-escape log poisoning) fixed in >=0.3.20; RUSTSEC-2023-0078 historical. | Spans around frame stages, asset loads; structured fields, per the standards. `tracing-log` carries wgpu/winit/egui `log` output |
| `log` + `env_logger` (stay) | MIT/Apache-2.0 | 0.4.34; 0.11.11 | 1.33B; 580M | n/a | 22 | None, but no spans/fields/timings; 165 `log::` call sites today | Does not meet the observability standard |
| `tracing-tracy` (profiler bridge) | MIT/Apache-2.0 (bundled Tracy: BSD-3-Clause) | 0.12.0 / 2026-08-25 | 9.7M / 1.6M | 1 (nagisa) | +19 | Builds C++ Tracy client (`cc`); dev-only feature | Best frame profiler; use gated off in shipped builds |
| `puffin` (+`puffin_egui` 0.30.0) | MIT/Apache-2.0 | 0.20.0 / 2026-03-18 | 6.1M / 1.3M | 2 (emilk, Embark) | 16 | Pure Rust; `puffin_egui` tracks egui versions (verify against egui 0.34 before adopting) | Convenient in-game flame graph |
| homebrew | n/a | n/a | n/a | n/a | n/a | Span/field/filter machinery | Not realistic |

- Economical: **adopt `tracing` + `tracing-subscriber`**, bridge existing `log` calls with `tracing-log` and migrate call sites opportunistically.
- Lean: **same**. Net weight is small because `tracing` is already present, and `env_logger` can be dropped. Profiler bridge deferred;
  prefer `tracing-tracy` behind a dev cargo feature when a frame-time card exists (check its `tracy-client-sys` licence line in `just deny`).
- Gate: add `tracing-subscriber` init where `env_logger` is built today (`src/app/initializer.rs:147`); keep `RUST_LOG` semantics via `EnvFilter`.

#### 5. Audio (ENG-F16)

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`rodio`** (current) | MIT/Apache-2.0 | 0.22.2 / 2026-03-05 (repo pushed today) | 12.4M / 3.35M | 2 (RustAudio maintainers) | 57 | None. symphonia 0.5.5 MPL-2.0, per ADR-0007 | Has `SpatialPlayer` + `Spatial` source: per-ear distance, gain `1/d^2` clamped to 1, no configurable min/max distance or rolloff, no listener orientation (caller supplies ear positions), mutex per player |
| `kira` | MIT/Apache-2.0 | 0.12.5 / 2026-09-26 | 972k / 167k | 1 (tesselode) | 61 | Pulls **glam 0.33** (ours is 0.30; public API uses it), cpal 0.18 and symphonia 0.6.1 (all new duplicates unless rodio leaves). Default `cpal-realtime-dbus` pulls libdbus on Linux (can turn off) | Spatial tracks with listener, distance attenuation, mixer buses, tweens, clocks, effects. Rewrites `moho_audio` (726 lines) |
| `oddio` | MIT/Apache-2.0 | 0.7.4 / 2023-10 | 58k / 5k | 1 | 14 | Stale single-maintainer; cpal 0.17 | No |
| `fyrox-sound` | MIT | 1.0.1 / 2026-03 | 82k / 5.8k | 1 | 163 | Pulls unmaintained `bincode`, `fxhash`, `instant`, `paste` (RustSec); engine-coupled | No |
| `cpal` + homebrew mixer | Apache-2.0 | 0.18.2 / 2026-08 | 23M / 7.6M | n/a | 10 | We would re-implement decode (symphonia), mixing, resampling | ~1,500+ lines with realtime-thread care |

- Economical: **keep `rodio`**. Green light; the F16 need is covered by its spatial types plus our own attenuation.
- Lean: **keep `rodio`, own the gain/pan maths** (~150-250 lines): listener transform to left/right ear positions, per-emitter
  `min_distance`/`max_distance`/rolloff curve, equal-power pan; applied through rodio's `ChannelVolume`/per-channel volumes.
  Proof: pure functions tested on gain values (silence beyond max, 1.0 at min, monotone falloff, hard-left/right pan at 90 degrees,
  listener rotation moves a source from left to right), matching F16's exit criterion; existing non-positional tests untouched.
- Revisit `kira` when a card needs buses/ducking, reverb or occlusion filters (F16 puts these out of scope). Its glam 0.33 would need
  adapter conversions at the engine boundary.

#### 6. Data / mod file format (ENG-F19)

| Candidate | Licence | Version / date | Downloads (all / 90d) | Owners | Pkgs | Red flags | Fit |
|---|---|---|---|---|---|---|---|
| **`toml`** (+serde) | MIT/Apache-2.0 | 1.1.6 / 2026-09-10 | 979M / 246M | 2 (epage + toml-rs) | 15 | None; stable 1.x. Weak at deeply nested enums, no schema tooling | `[weapons.rifle_ak]` makes the id the table key; later roots override by key; comments; line/col in errors; modder-friendly |
| `ron` (+serde) | MIT/Apache-2.0 | 0.12.2 / 2026-06-22 | 118M / 21M | 3 (kvark, torkleyy, juntyr) | 15 | 0.x API; unfamiliar to modders; no schema tooling | Native enums/Option/tuples; best for enum-heavy defs |
| `serde_json` | MIT/Apache-2.0 | 1.0.151 / 2026-07-20 | 1.40B / 346M | 2 (dtolnay) | 13 | **Already in tree**. No comments (needs `json5`/`serde_json5`, the latter Apache-2.0 AND ISC, last release 2025-02) | JSON Schema editor support; universal tooling |
| `kdl` | Apache-2.0 | 6.7.1 / 2026-05-31 | 2.7M / 1.6M | 1 | 13 | Document model, not serde-derive; typed definitions would need glue | Poor fit |
| homebrew | n/a | n/a | n/a | n/a | n/a | Parser + error spans | Not realistic |

- Economical: **adopt `toml`**. Green light (huge, stable, no flag).
- Lean: **adopt `toml`**; no realistic homebrew (a TOML 1.1 parser with spans is thousands of lines).
- ADR-0006 (saves) stays on postcard: this choice is for authored content only.
- Fit to F19 exit criteria: id-keyed tables give "later root overrides an entry by stable string id" as a `BTreeMap` merge; "a load error names the file
  and the id" via `toml::de::Error::span` plus the key. Behaviours referenced by id are plain string fields.
- Runner-up: `ron` if FPS item/weapon definitions become enum/tuple-heavy; it costs the same 15 packages. Because definitions are serde types,
  switching later touches loader code only, not the schema types.

#### Cross-cutting notes

- Duplicates avoided: choices above add no new glam/winit/cpal versions. `kira` was the only candidate that would have.
- Known follow-ups for ENG-F2 (not decided here): remove `env_logger` after `tracing` lands; move `gilrs` to its consuming crate; enable `image` jpeg only on demand.
- Checked caveat: `cargo deny` runs used the current RustSec db offline-cached on this machine, 2026-10-07.

