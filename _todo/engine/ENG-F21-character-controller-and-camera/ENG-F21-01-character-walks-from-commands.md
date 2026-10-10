# A character walks and jumps from per-tick commands

**Feature:** ENG-F21 (#123)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

The engine moves a character from a per-tick command (move axes, look delta, jump)
using movement settings the game supplies. The strategy game's walking mode moves onto
it, and its own copy of the walking logic is deleted. Consumers: the strategy game now,
P.2 next.

## Deliverables

- A character turns by the command's look delta and walks relative to its facing, at
  the game's current speed, with gravity, landing and step-up.
- A jump command lifts a grounded character by the game's jump strength; a jump in the
  air does nothing.
- The game can change speed and jump strength between ticks.
- The strategy game's walking mode uses it; no walking logic remains on the strategy line.

## Acceptance criteria

```gherkin
Scenario Outline: Frame rate does not change where the character ends up
  Given the same input command sequence
  When the character is driven for N ticks at <fps> frames per second
  Then its position equals the 60 fps result

  Examples:
    | fps |
    | 30  |
    | 144 |

Scenario: Forward walks the way the character faces
  Given a character on flat ground facing +Z with speed 4 m/s
  When it is driven forward for 60 ticks
  Then it has moved 4 m along +Z

Scenario: Diagonal input is no faster than straight input
  Given a character on flat ground with speed 4 m/s
  When it is driven forward and right together for 60 ticks
  Then it has moved 4 m

Scenario: Looking up does not slow walking
  Given a character on flat ground with speed 4 m/s, looking 60° up
  When it is driven forward for 60 ticks
  Then it has moved 4 m horizontally
  And its height is unchanged

Scenario: The game changes speed between ticks
  Given a character walking forward at 4 m/s
  When the game sets the speed to 6 m/s
  Then the next tick moves it 0.1 m

Scenario: Jumping only from the ground
  Given a grounded character
  When it is sent a jump command
  Then it rises
  And a second jump command while it is in the air does not raise it further

Scenario: Look is clamped short of straight up and down
  Given a character
  When it is sent a look delta of 3 radians upward
  Then its pitch stops short of straight up
```

## Verification

- Strategy game, first-person walking: walk, sprint, jump, climb a one-block step, fall
  off the map and respawn; feel matches `dev`.

## Notes

- Sprint is the game setting a higher speed for the tick; the engine has no sprint flag.
