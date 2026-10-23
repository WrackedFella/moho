<!-- board: gate=unset; eligible=no; priority=P2 -->
# The tester presses an in-world button to call the next wave

**Feature:** FPS-F9 (#616)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
After a wave is cleared, the next wave waits until the tester presses a button standing in
the arena, so there is time to loot and reload first. This card builds the use mechanic
every world object shares: the use key, the look-and-reach test, the prompt, and a use
the host resolves. Formerly FPS-F2-09.

## Deliverables
- A use key, bound by default and shown in the prompt.
- A next-wave button placed in the arena by a marker.
- While the player looks at a usable object within reach, a prompt names the use key and
  the action; it goes when they look away or step out of reach.
- After a cleared wave the next wave waits for the button; wave 1 still starts on load and
  on restart.
- Next wave is a host action, honored from player slot 0 only.

## Acceptance criteria
```gherkin
Scenario: A cleared wave is followed by a pause, not an instant next wave
  Given the player has killed the last enemy of wave 1
  When 30 seconds pass
  Then no enemy of wave 2 appears

Scenario: The player sees that the button can be used
  Given the player is within reach of the next-wave button and looking at it
  Then a prompt names the use key and "Next wave"

Scenario Outline: The prompt goes when the player looks or walks away
  Given the prompt is showing
  When the player <moves>
  Then no prompt shows

  Examples:
    | moves              |
    | looks away         |
    | steps out of reach |

Scenario: The button cannot be used through a wall
  Given the button is within reach but behind a wall from the player
  When the player looks toward it and presses use
  Then no prompt shows
  And no wave spawns

Scenario: Using the button calls the next wave
  Given wave 1 is cleared and the next wave is waiting
  And the player is looking at the button within reach
  When the player presses use
  Then wave 2 spawns at the enemy markers
  And the HUD shows wave 2

Scenario: The button does nothing while enemies remain
  Given wave 1 still has enemies alive or yet to appear
  When the player uses the button
  Then no extra enemies spawn
  And the wave number is unchanged

Scenario: The first wave still starts without the button
  Given the arena has just loaded or the player has restarted after death
  Then wave 1 spawns without the player using the button

Scenario: A restart clears a waiting wave
  Given wave 2 is cleared and the next wave is waiting
  When the player dies and restarts
  Then wave 1 spawns and the HUD shows wave 1
```

## Verification
- Clear a wave, loot and reload at leisure, walk to the button and call the next wave:
  the pause feels under the tester's control.
- The button is easy to find and the prompt reads at a glance.

## Notes
- TL: a use from any slot is resolved from that slot's own eye; host actions ignore slots
  other than 0. The sim holds one player today, so prove it at the command level.
- TL: the prompt needs on-screen text (#407); until it lands, the title-bar HUD carries it.
- TL: the use key joins the keybindings #460 shows and ENG-F26 #359 rebinds.
- Out: an automatic timer, calling a wave early, a console alternative (FPS-F9 deferred).
