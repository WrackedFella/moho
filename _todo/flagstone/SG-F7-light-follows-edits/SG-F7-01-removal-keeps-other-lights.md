# Removing a light leaves every other light's area lit

**Feature:** SG-F7
**Status:** Draft
**Gate class:** domain
**Labels:** line:strategy

## Summary

Block-light removal clears a surviving emitter's own voxel when the removed light was at least as bright there, and nothing re-seeds emitters from their emission, so that emitter's whole area goes dark. Removal must leave exactly what the remaining lights produce.

## Deliverables

- After any removal, every voxel's block light equals what a fresh fill of the remaining emitters gives.

## Acceptance criteria

```gherkin
Scenario Outline: A dimmer light survives the removal of a brighter one nearby
  Given a <survivor> light and a <removed> light <distance> blocks apart on one channel
  When the <removed> light is removed
  Then every voxel within 16 blocks has the level the <survivor> light alone gives it

  Examples:
    | survivor | removed | distance |
    | 8        | 15      | 6        |
    | 9        | 15      | 6        |
    | 15       | 15      | 1        |

Scenario: Removing a light inside another emitter's reach keeps that emitter at full strength
  Given an emitter of level 10 and a level 15 light 3 blocks away
  When the level 15 light is removed
  Then the emitter's own voxel reads 10
```

## Tech spec
## Verification
## Notes

- Repro from ENG-F9-10: blue 8 at (19,19,19), blue 15 at (19,19,25), remove the second: blue at (12,19,19) should be 1, reads 0.
