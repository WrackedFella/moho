# Taking an input in use asks before it replaces

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Capturing an input another action uses opens a confirm dialog naming that action. Replace moves the input and leaves the other action's slot Unbound; Cancel changes nothing. Unbound slots are marked so they are not missed.

## Deliverables
- Conflict dialog with Replace and Cancel, replacing FPS-F14-01's refusal.
- Unbound slots shown as Unbound, visibly marked.

## Acceptance criteria
```gherkin
Scenario: Taking an input in use asks first
  Given Lean left is on Q
  When the player captures Q for Reload
  Then a dialog names Lean left and asks the player to confirm
  And nothing changes until the player answers

Scenario Outline: The dialog resolves
  Given the dialog is asking about Q, held by Lean left
  When the player chooses <choice>
  Then <result>
  Examples:
    | choice  | result                                               |
    | Replace | Reload shows Q and Lean left shows that slot Unbound |
    | Cancel  | both actions keep their inputs                       |

Scenario: A replaced input moves in play
  Given the player replaced Lean left's Q with Reload and chose Apply
  When the player presses Q in the arena
  Then the player reloads and does not lean

Scenario: An unbound action is visible
  Given an action has no input in any slot
  When the player views Controls
  Then that action shows Unbound, marked so it is not missed
```

## Verification
1. Take Q for Reload, Replace, Apply, play: Q reloads, nothing leans left, and Controls marks Lean left.
