# The player clears a slot or resets every binding

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Each slot can be cleared, and one Reset on the page restores every action's defaults. Both are staged until Apply.

## Deliverables
- Clear on each slot.
- Reset for the whole Controls page.

## Acceptance criteria
```gherkin
Scenario: A slot can be cleared
  Given Reload has primary R and secondary T
  When the player clears the secondary and chooses Apply
  Then T does nothing in play and R still reloads

Scenario: Clearing every slot leaves the action Unbound
  Given the player clears both of Move forward's slots
  Then Move forward shows Unbound, marked

Scenario: Reset the page
  Given the player rebound several actions and cleared one
  When the player resets Controls and chooses Apply
  Then every action is on its defaults in Controls and in play

Scenario: Reset waits for Apply
  Given the player rebound Reload and applied it
  When the player resets Controls and goes back without Apply
  Then Reload keeps the player's input
```
