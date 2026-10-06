# 0003 — `moho_core` owns all event types

**Status:** Accepted; narrowed by [0010](0010-world-geometry-is-a-mesh-contract.md): `moho_core` owns engine-wide events, and line-specific events live with their line's crate

## Context

`AudioEvent` was once defined in both `moho_core` and `moho_audio` with
diverging variants, requiring a translation layer that silently dropped
events with no equivalent.

## Decision

Event payload types are defined once, in `moho_core::events`. Subsystem
crates re-export them (`moho_audio` does `pub use moho_core::events::AudioEvent`)
and never define parallel event enums.

## Consequences

- No mapping layer between event definitions; a variant added in
  `moho_core` reaches subscribers directly.
- Subsystem-internal types (e.g. `AudioCategory`) stay out of event shapes.
