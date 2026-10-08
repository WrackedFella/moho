# ENG-F3 — Renderer Pipeline Cleanup

**Note:** Earlier pipeline phases shipped with the renderer decoupling — see
**Status:** Draft
**Labels:** feature, line:engine
[ADR-0001](../../adr/0001-render-api-boundary.md)

## Summary

Structural cleanup so the renderer scales to larger scenes without forcing
rewrites. None block current gameplay; higher payoff once scene complexity
grows (more relevant once level-based content exists, not just voxel
terrain).

## Items

| Item |
|---|
| [ENG-F3-01 bind-group-split](ENG-F3-01-bind-group-split.md) |
| [ENG-F3-02 frustum-culling-sorting](ENG-F3-02-frustum-culling-sorting.md) |
| [ENG-F3-03 no-unsound-frame-callback](ENG-F3-03-no-unsound-frame-callback.md) |
| [ENG-F3-04 renderer-polish](ENG-F3-04-renderer-polish.md) |

## Notes

Sequencing: bind-group split is mechanical once nothing else is mid-flight.
Culling is the largest perf payoff but only worth it once the rest of the
pipeline isn't doing redundant work — do bind-group split first.

Unscoped finding, to revisit (2026-10-04, seen during an unrelated manual
playtest): shadowed and grazing-lit terrain faces show a fine per-pixel
speckle at dusk with a low sun. It is most visible on large flat faces, both
dirt slopes and grass. It looks like shadow acne or PCF/blocker-search noise,
but that is not diagnosed. Relevant knobs are in `moho_renderer/src/shadow.rs`:
depth bias constant 4, slope scale 3.0, two cascades split at 400/1500, and
PCSS sample counts per quality level. This is a visual defect, not a cleanup.
Triage it into a card here or a separate feature at the next planning review.
