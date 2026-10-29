# 0031 — Effects are engine presentation state placed by handle

**Status:** Proposed; amends [0012](0012-engine-crate-map-for-m2.md): the seam list gains `Effects` beside seam 2

## Context

[ADR-0025](0025-world-meshes-are-placed-per-frame.md) places meshes per frame and holds
nothing between frames. An effect such as a tracer streak needs a life (start, end, a
forgotten end that must not leak), a fade after the end, a cap that follows the saved
effects quality, and a draw that casts no shadow. Every game would copy these.

## Decision

- Effects are retained, presentation-only engine state, owned by the `Scene` and keyed by a
  handle the engine issues: monotonic and never reused. A stale, unknown or inert handle is
  ignored.
- The game places a followed effect every frame, with the head already interpolated between
  ticks; the engine never reads sim state.
- Effect meshes are placed unshadowed: they reach the backend through
  `enqueue_unshadowed_draw` and the shadow pass skips them.
- Effects run on frame time, not sim time ([ADR-0011](0011-simulation-stays-network-ready.md)),
  so pause and time scale do not slow them.
- The effects quality caps live effects; a game hands the saved value to
  `Effects::set_quality`.
- Mesh ids from `EFFECT_MESH_IDS_FROM` (`u64::MAX - 63`) are reserved for the engine.

## Consequences

- The seam list gains `Effects` beside seam 2 (world meshes).
- Fade is a taper in width: world-mesh draws are opaque. Blended effects arrive with the
  transparency pass of slice 3.
- A game must keep its own mesh ids below `EFFECT_MESH_IDS_FROM`; this is documented, not
  enforced.
- A streak faces the eye at draw time, so a second camera shows it wrongly until mirrors
  (ENG-F31) revisit it.
