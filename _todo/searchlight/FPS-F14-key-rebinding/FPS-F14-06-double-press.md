# A slot binds a double press

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The kind picker gains double press: pressing the key twice quickly does the action.

## Deliverables
- Double press in the kind picker; bindings work in play and persist.

## Acceptance criteria
```gherkin
Scenario: A double press does the action
  Given Check magazine has a slot set to double press on Y, applied
  When the player presses Y twice quickly
  Then the player checks the magazine

Scenario: A single press of that key is not a double press
  Given Check magazine is on Double Y and nothing else uses Y
  When the player presses Y once
  Then the magazine is not checked

Scenario: A press and a double press of one key do not conflict
  Given Reload is on Y
  When the player sets Check magazine to Double Y
  Then no dialog appears
```

## Verification
1. With Reload on Y and Check magazine on Double Y: single presses reload and double presses check. Note whether the reload delay is felt.
