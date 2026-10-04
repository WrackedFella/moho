# ENG-F7 — Entity storage is maintained and legion-free

**Status:** approved (Foundation gate, [ENG-F8](../ENG-F8-foundation-gate/_feature.md))
**Issue:** #62
**Integration branch:** `feature/ENG-F8-foundation-gate`

## Summary

`legion` 0.4.0 (last release February 2021, archived Amethyst project) is the
ECS under `moho_core`, `moho_game`, `moho_renderer` and the binary. It pulls
in unmaintained `instant` and `paste` (RUSTSEC-2024-0384, RUSTSEC-2024-0436)
and will not track Rust or ecosystem changes. Both game lines will be built
on whatever the engine chooses here, so decide before the repo split.

## Exit criteria

- [ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md) is accepted: no general-purpose ECS for now, and typed,
  domain-owned stores replace `legion`.
- `cargo tree --workspace -i legion` reports no match.
- The `instant` and `paste` ignores are removed from `deny.toml`.
- No engine crate's public API names a world or ECS type.
- Chunk lookup by position doesn't scan all chunks.
- No regression in `event_bus_bench`. (There is no chunk-streaming benchmark,
  so an earlier criterion naming one was dropped.)

## Scope

- In: removing `legion` from the binary, `moho_game` and `moho_renderer`;
  typed stores; renderer collection API taking domain data.
- Out: adopting `hecs` or another ECS (needs a named consumer, per [ADR-0004](../../adr/0004-entity-storage-without-a-general-ecs.md));
  redesigning game systems beyond what the swap forces.

## Items

| Item | Status |
|---|---|
| [ENG-F7-01 typed-entity-stores](ENG-F7-01-typed-entity-stores.md) | in progress |
| [ENG-F7-02 legion-removed](ENG-F7-02-legion-removed.md) | ready |

## Notes

Footprint at proposal time: 11 source files use `legion`; `moho_render_api`
is deliberately legion-free ([ADR-0001](../../adr/0001-render-api-boundary.md)). Discuss in the pre-agentic feature
planning review.
