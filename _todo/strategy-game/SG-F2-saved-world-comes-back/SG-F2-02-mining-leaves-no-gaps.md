# Mining leaves no see-through gaps in the terrain

**Feature:** [SG-F2](https://github.com/WrackedFella/moho/issues/308)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

Mining a block can tear a visible hole in nearby terrain instead of closing over the missing material. Seen at terrain section edges and, rarely, where angled surfaces meet; mining makes the latter worse.

## Deliverables

- A recorded reproduction for each case, then a failing test, then the fix.

## Acceptance criteria

```gherkin
Scenario: Mining next to a terrain section edge leaves no gap
  Given a block on the edge of a terrain section
  When the player mines it
  Then the neighbouring terrain closes over the hole with no see-through

Scenario: Mining where angled surfaces meet leaves no gap
  Given terrain where two angled surfaces intersect
  When the player mines a block beside the intersection
  Then the surface stays closed with no see-through
```

## Verification

Manual: mine along a section edge and beside a steep intersection; look from several angles.

## Notes

- A gap at an angled intersection that exists before any mining belongs to the world-generation feature, not here.
