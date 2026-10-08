# Engine and game logs are structured tracing events

**Feature:** [ENG-F2](_feature.md)
**Issue:** [#98](https://github.com/WrackedFella/moho/issues/98)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

ENG-F13 replaced `log` and `env_logger` with `tracing` and
`tracing-subscriber` ([audit](../ENG-F13-dependency-audit/audit.md)): `log`
has no spans or structured fields, which the engineering standards require.

## Deliverables

- No workspace crate declares `log` or `env_logger`; `tracing` is in
  `[workspace.dependencies]`.
- Every log macro call site is a `tracing` call with structured fields for its
  payload (175 sites across 7 crates).
- Output from dependencies that still use `log` (wgpu, winit) appears through
  the same subscriber.

## Acceptance criteria

- [ ] `RUST_LOG=debug cargo run` prints engine and dependency events through one subscriber, filtered by `RUST_LOG`.
- [ ] No log call interpolates its payload into the message string.
- [ ] `log` remains in the lockfile only as a transitive dependency.

## Tech spec

**Design.**
- `[workspace.dependencies]`: `tracing = "0.1"` and `tracing-subscriber = { version = "0.3.20", features = ["env-filter"] }`. Version 0.3.20 or later is required for RUSTSEC-2025-0055. The default features include `fmt`, `ansi` and `tracing-log`.
- Each crate that declares `log` (root, `moho_audio`, `moho_core`, `moho_game`, `moho_physics`, `moho_renderer`, `moho_ui`) swaps it for `tracing`. The root also swaps `env_logger` for `tracing-subscriber`.
- Subscriber: a `fn init_logging()` in `src/main.rs`, called once at the top of `main` before `AppInitializer::build`. It runs `tracing_subscriber::fmt().with_env_filter(EnvFilter::builder().with_default_directive(LevelFilter::INFO.into()).from_env_lossy()).init()`. With the `tracing-log` feature, `init` also installs the `LogTracer`, so `log` records from wgpu, winit and rodio reach the same subscriber.
- The `let _ = env_logger::…try_init()` in `AppInitializer::build` is deleted, not translated. Initialising in `main` means tests never install a global subscriber, so there is no second-init error left to discard.
- Call sites, one mechanical pass (175 macro calls in 36 files on `dev`, 2026-10-07):
  - Import `tracing::{debug, info, …}` or path-qualify `tracing::info!`, matching the file's current style.
  - The message is a static string. Values move into fields, as in `devflow:rust-standards` (Observability): `%` for `Display`, `?` for `Debug`, bare for primitives and `&str`. Example: `warn!(chunk = ?pos, error = %e, "failed to save evicted chunk")`.
  - Field names are snake_case nouns. An error value is always `error`; positions are `chunk` or `pos`; counts are `count`.
  - Levels stay as they are. Adjusting a level is a judgement call per site and is not part of this mechanical pass.

**Out of scope.**
- Spans, including frame-stage spans. The standard wants them, but they wait for a consumer; the profiler bridge is deferred by the feature.
- Changing log levels, rewording messages beyond moving values into fields, and adding new events.
- `println!`/`eprintln!` sites (the doctest in `initializer.rs`), and any `log` that remains transitive.

**Test map.** This card has no domain rule to unit-test, and a global subscriber can't be asserted without process-global state. Proof is per criterion:
| Criterion | Proof | Gate class |
|---|---|---|
| One subscriber, filtered by `RUST_LOG`, carries engine and dependency events | Verification step 1 (manual run) | glue |
| No call interpolates its payload | The PR quotes an empty result from `rg -n '\b(trace\|debug\|info\|warn\|error)!\([^;]*"[^"]*\{' --type rust`; the reviewer checks it | glue |
| `log` only transitive | `cargo tree --workspace -e normal,build -i log` lists no workspace crate as a direct dependent | glue |
| Nothing else changed | `just check` (existing tests) | glue |

**Gate class:** glue. Suited to a cheap model: the edits are mechanical, with the field convention above.

**Risks.**
- Every crate is touched. Merge conflicts are likely with [#94](https://github.com/WrackedFella/moho/issues/94) (`save.rs`, `chunk_streamer.rs`, `scene_persistence.rs`), [#99](https://github.com/WrackedFella/moho/issues/99) and [#100](https://github.com/WrackedFella/moho/issues/100). Land it after #94, and keep the PR to this pass so a rebase stays mechanical.
- `init()` panics if a subscriber is already set. It runs only in `main`, which is the reason it moves out of `build`.

## Verification

1. `RUST_LOG=debug cargo run`: engine events and wgpu/winit events print in one format, with fields shown as `key=value`.
2. `RUST_LOG=warn cargo run`: info and debug lines disappear. With `RUST_LOG` unset, the level is info.

## Notes

Profiler bridges (`tracing-tracy`, `puffin`) are deferred until a consumer
needs them. Mechanical migration; suited to a cheap model.
