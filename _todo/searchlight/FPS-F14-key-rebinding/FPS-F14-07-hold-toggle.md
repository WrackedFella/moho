# Held actions toggle by a checkbox

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Each held action shows a hold/toggle checkbox beside its binding (GDD §3.3). In toggle, one press holds the action until the next press.

## Deliverables
- Checkbox beside every action the game marks toggleable (today lean left and lean right), in the basic view.
- Choice staged, applied and saved with the bindings; the default is hold.

## Acceptance criteria
```gherkin
Scenario: A held action toggles
  Given Lean left is set to toggle and applied
  When the player presses its key once and then again
  Then the lean holds after the first press and ends after the second

Scenario: Hold is the default
  Given Lean left has never been set
  When the player holds and releases its key
  Then the lean ends on release

Scenario: The choice survives a relaunch
  Given the player set Lean left to toggle and applied it
  When the player relaunches Searchlight
  Then Controls shows toggle and the game uses it

Scenario: Only held actions offer the checkbox
  When the player views Controls
  Then Reload shows no hold/toggle checkbox
```
