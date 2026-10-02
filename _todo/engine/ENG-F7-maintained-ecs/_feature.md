# ENG-F7 — The ECS foundation is maintained

**Status:** proposed

## Summary

`legion` 0.4.0 (last release February 2021, archived Amethyst project) is the
ECS under `moho_core`, `moho_game`, `moho_renderer` and the binary. It pulls
in unmaintained `instant` and `paste` (RUSTSEC-2024-0384, RUSTSEC-2024-0436)
and will not track Rust or ecosystem changes. Both game lines will be built
on whatever the engine chooses here, so decide before the repo split.

## Exit criteria

- An ADR records the choice: migrate to a maintained ECS (candidates to
  evaluate include `bevy_ecs` standalone, `hecs`, `flecs` bindings) or adopt a
  maintained fork of `legion`, with the evaluation criteria used.
- The chosen ECS is maintained by an upstream or by us, with an owner and
  update policy.
- The `instant` / `paste` ignores are removed from `deny.toml`.
- No regression in the event-bus and chunk-streaming benchmarks.

## Scope

- In: evaluation, ADR, migration or fork, ECS-facing API in engine crates.
- Out: redesigning game systems beyond what the ECS swap forces.

## Items

| Item | Status |
|---|---|

## Notes

Footprint at proposal time: 11 source files use `legion`; `moho_render_api`
is deliberately legion-free (ADR-0001). Discuss in the pre-agentic feature
planning review.
