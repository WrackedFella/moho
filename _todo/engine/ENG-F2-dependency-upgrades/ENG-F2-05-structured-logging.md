# Engine and game logs are structured tracing events

**Feature:** [ENG-F2](_feature.md)
**Issue:** #98

## Summary

ENG-F13 replaced `log` and `env_logger` with `tracing` and
`tracing-subscriber` ([audit](../ENG-F13-dependency-audit/audit.md)): `log`
has no spans or structured fields, which the engineering standards require.

## Deliverables

- No workspace crate declares `log` or `env_logger`; `tracing` is in
  `[workspace.dependencies]`.
- Every log macro call site is a `tracing` call with structured fields for its
  payload (about 165 sites across 7 crates).
- Output from dependencies that still use `log` (wgpu, winit) appears through
  the same subscriber.

## Acceptance criteria

- [ ] `RUST_LOG=debug cargo run` prints engine and dependency events through one subscriber, filtered by `RUST_LOG`.
- [ ] No log call interpolates its payload into the message string.
- [ ] `log` remains in the lockfile only as a transitive dependency.

## Notes

Profiler bridges (`tracing-tracy`, `puffin`) are deferred until a consumer
needs them. Mechanical migration; suited to a cheap model.
