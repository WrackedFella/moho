# ENG-F3 — Renderer Pipeline Cleanup

**Status:** not started (earlier pipeline phases shipped with the renderer decoupling — see
[ADR-0001](../../../docs/adr/0001-render-api-boundary.md))

## Summary

Structural cleanup so the renderer scales to larger scenes without forcing
rewrites. None block current gameplay; higher payoff once scene complexity
grows (more relevant once level-based content exists, not just voxel
terrain).

## Items

| Item | Status |
|---|---|
| [ENG-F3-01 bind-group-split](ENG-F3-01-bind-group-split.md) | not started |
| [ENG-F3-02 frustum-culling-sorting](ENG-F3-02-frustum-culling-sorting.md) | not started |
| [ENG-F3-03 misc-cleanups](ENG-F3-03-misc-cleanups.md) | not started |

## Notes

Sequencing: bind-group split is mechanical once nothing else is mid-flight.
Culling is the largest perf payoff but only worth it once the rest of the
pipeline isn't doing redundant work — do bind-group split first.
