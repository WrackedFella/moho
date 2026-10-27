# Each action takes a secondary input

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Every action gets a secondary slot beside the primary. Either input does the action.

## Deliverables
- A secondary slot per action, captured, staged and saved as the primary is.
- Default secondaries shown (Check magazine's second default appears here).

## Acceptance criteria
```gherkin
Scenario: A secondary input works alongside the primary
  Given Reload has primary R and the player captures T as its secondary and chooses Apply
  When the player presses either key in the arena
  Then the player reloads

Scenario: A secondary survives a relaunch
  Given the player applied a secondary input
  When the player relaunches Searchlight
  Then Controls shows it and the game uses it

Scenario: One action's own slots do not conflict
  Given Reload's primary is R
  When the player captures R for Reload's secondary
  Then no dialog appears and the secondary is unchanged

Scenario: A secondary in use by another action asks first
  Given Lean left is on Q
  When the player captures Q for Reload's secondary
  Then the conflict dialog names Lean left
```

## Verification
1. Give Fire a secondary of F, Apply, play: both the left button and F fire.
