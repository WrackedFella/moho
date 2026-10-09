# A key held when play stops is released, not stuck on resume

**Feature:** SG-F2
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

Holding a movement key while opening the menu or console, then releasing it there,
leaves the action held: on resume the pawn keeps moving until the key is pressed
again. The game stops reading input when it leaves play, so the release is never
seen (found in #239, ENG-F12-03). Release every held action when the game leaves the
playing state, as already happens when the window loses focus.

## Deliverables

- Whenever the game stops reading play input (menu, console), every held action is released.

## Acceptance criteria

```gherkin
Scenario Outline: An action held when play stops is not held on resume
  Given the game is playing and the move-forward action is held
  When the player opens the <screen>
  And the key is released while the <screen> is open
  And the player returns to play
  Then move-forward is not held
  And the pawn does not move on the next tick

  Examples:
    | screen      |
    | menu        |
    | console     |

Scenario: A key still down on resume acts only after it is pressed again
  Given the game is playing and move-forward is held
  When the player opens the menu and returns to play without releasing the key
  Then move-forward is not held until the key is pressed again
```

## Verification

Manual: hold W, press Esc, release W, resume. The pawn stands still.

## Notes

- Engine lane drafted this because it surfaced in ENG-F12; the fix is in the strategy
  game's state handling, and the engine's release-all already exists.
