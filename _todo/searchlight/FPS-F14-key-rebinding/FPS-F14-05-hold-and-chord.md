# An action's details set hold and modifier per slot

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The basic view shows plain presses. A chevron on a row expands that action's details: per slot, a kind picker (press or hold) and a modifier picker (none, Ctrl, Shift, Alt), plus a reset for that action. Capture still records only the key or button.

## Deliverables
- Per-action expander; the basic view marks actions that have more than plain presses.
- Kind and modifier pickers per slot; per-action reset in the details.
- Hold and chord bindings work in play and persist.

## Acceptance criteria
```gherkin
Scenario: The basic view shows plain presses
  Given no row in Controls is expanded
  Then each action shows its press bindings
  And an action with a hold or chord binding is marked as having more

Scenario: Expanding shows one action's details
  When the player expands Check magazine
  Then both its slots show kind and modifier pickers and the action shows its reset
  And no other row expands

Scenario Outline: The pickers set what the input means
  Given the player set a Check magazine slot to <kind> and <modifier>
  When the player captures Y into it and chooses Apply
  Then the slot shows <binding>
  And <gesture> in play checks the magazine
  Examples:
    | kind  | modifier | binding | gesture                      |
    | hold  | none     | Hold Y  | holding Y past the hold time |
    | press | Ctrl     | Ctrl+Y  | holding Ctrl and pressing Y  |

Scenario: Capture records the key, not the gesture
  Given a slot's kind is hold
  When the player taps Y during capture
  Then the slot shows Hold Y

Scenario: A press and a hold of one key do not conflict
  Given Reload is on R
  When the player sets Check magazine to Hold R
  Then no dialog appears

Scenario: Reset one action
  Given the player rebound Reload and Fire
  When the player resets Reload from its details and chooses Apply
  Then Reload is on its defaults and Fire keeps the player's choice
```

## Notes
Per-action reset is the first thing cut if this card runs long; it then becomes its own card.
