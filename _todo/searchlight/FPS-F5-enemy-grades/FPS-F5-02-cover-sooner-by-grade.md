# Better-graded enemies take cover sooner

**Feature:** FPS-F5
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary

An enemy under fire heads for cover sooner the higher its grade. Higher grades react to
shots that pass close to them; lower grades react late, or only once hit.

## Deliverables

- A per-grade rule for when an enemy under fire runs for cover, in ladder order.
- Shots that pass close to an enemy count as being under fire for the grades that react to them.
- Cover choice, running to cover and giving up on cover work as they do today.

## Acceptance criteria

```gherkin
Scenario: A professional is in cover before a rookie
  Given a rookie and a professional stand in the open with cover in reach
  When the player fires the same shots past each without hitting
  Then the professional reaches cover before the rookie does

Scenario: A near miss sends a professional to cover
  Given a professional in the open with cover in reach
  When the player's shot passes close to it without hitting
  Then the professional runs for cover

Scenario: A near miss alone does not move a rookie
  Given a rookie in the open with cover in reach
  When the player's shot passes close to it without hitting
  Then the rookie stays where it is

Scenario: A hit sends a rookie to cover
  Given a rookie in the open with cover in reach
  When the player hits it without killing it
  Then the rookie runs for cover

Scenario: A distant miss is not a near miss
  Given a professional in the open with cover in reach
  When the player's shot lands far from it
  Then the professional does not run for cover
```

## Verification

- Force professional, fire past an enemy in the open: it ducks into cover. Force rookie,
  do the same: it keeps standing until hit.
