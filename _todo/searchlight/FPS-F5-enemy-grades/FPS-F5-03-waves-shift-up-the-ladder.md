# Later waves field better-graded enemies

**Feature:** FPS-F5
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary

Each wave spawns a mix of grades, and the mix shifts up the ladder as waves go on, so
later waves are harder because their enemies are better, not tougher.

## Deliverables

- A grade mix for each wave, tunable as data; wave 1 is mostly rookie and novice.
- The share of experienced-and-above never falls from one wave to the next.
- A forced grade (FPS-F5-01) overrides the mix while it is set.

## Acceptance criteria

```gherkin
Scenario: Wave 1 is mostly low grades
  When wave 1 spawns
  Then most of its enemies are rookies or novices

Scenario: Later waves sit higher on the ladder
  Given wave 1 and wave 5 of the same run
  When each wave's enemies spawn
  Then wave 5 has a larger share of experienced, veteran and professional enemies

Scenario: The mix never steps down
  Given any two consecutive waves
  When both have spawned
  Then the later wave's share of experienced and above is at least the earlier wave's

Scenario: A forced grade overrides the mix
  Given the grade is forced to professional in the console
  When wave 1 spawns
  Then every enemy in it is a professional
```

## Verification

- Play to wave 5: early waves hesitate and miss, later waves fire fast and duck; kills
  still take the same hits.
