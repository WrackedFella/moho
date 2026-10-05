# Mining shows a pickup feedback popup

**Status:** not started
**Feature:** [SG-F1](_feature.md)

## Summary

Transient "+1 <resource>" popup on a successful mine, via the event bus (not
`HudData` — this is a discrete fire-and-forget occurrence, not per-frame
snapshot state).

## Deliverables

- `GameEvent::ResourceGained { resource_id, amount }`, published only when a
  mine actually yields something.
- A `moho_ui` overlay subscribing to it: renders a fading "+N" popup (~1s),
  multiple concurrent pickups stack rather than overwrite.
