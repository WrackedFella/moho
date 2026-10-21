# Opening a wall lets neighbouring light in

**Feature:** SG-F7
**Status:** Draft
**Gate class:** domain
**Labels:** line:strategy

## Summary

Removing an opaque block queues a light removal at a voxel that holds no light, so nothing happens: light from the neighbours never flows into the opening. Removing an opaque block must refill it from its brightest neighbours.

## Deliverables

- Removing an opaque block relights the opening and everything the light can now reach through it.

## Acceptance criteria

```gherkin
Scenario: A removed wall block lets light through
  Given a level 15 light 2 blocks from an opaque wall, and dark air behind the wall
  When the wall block in line with the light is removed
  Then each voxel has the level a fresh fill of the same blocks gives it

Scenario: Removing an opaque block far from any light changes nothing
  Given no light within 16 blocks
  When an opaque block is removed
  Then no voxel's block light changes and no chunk is reported affected

Scenario: Removing an emissive block still clears its light
  Given an emissive block and no other light
  When it is removed
  Then every voxel within 16 blocks reads 0
```

## Tech spec
## Verification
## Notes

- After SG-F7-01 (same removal code).
