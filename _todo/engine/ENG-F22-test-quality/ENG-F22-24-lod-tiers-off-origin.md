# LOD tiers follow the player away from the origin

**Feature:** ENG-F22
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

The LOD tier tests always put the player in chunk column (0, 0), so an implementation
that ignores the player's position, or adds it instead of subtracting, passes (found
in #211, ENG-F22-01). Pin the tiers with the player elsewhere, negative coordinates
included.

## Deliverables

- The existing LOD tier table gains rows with the player off the origin.

## Acceptance criteria

```gherkin
Scenario Outline: A chunk's LOD tier is measured from the player's chunk
  Given the player is in chunk <player>
  When the LOD tier of chunk <chunk> is computed
  Then it is <tier>

  Examples:
    | player       | chunk        | tier |
    | (10, 0, -7)  | (13, 0, -7)  | 0    |
    | (10, 0, -7)  | (14, 0, -7)  | 1    |
    | (10, 0, -7)  | (10, 0, -11) | 1    |
    | (-5, 2, 5)   | (-2, 0, 8)   | 0    |
    | (-5, 2, 5)   | (-5, 9, 1)   | 1    |
```
