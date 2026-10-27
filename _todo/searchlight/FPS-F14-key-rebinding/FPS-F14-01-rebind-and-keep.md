# The player rebinds an action's key and keeps it

**Feature:** FPS-F14 (#718)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Settings gets a Controls page listing every player action with its primary input. The player captures a new key or mouse button for an action, and Apply makes it live and saves it. The skeleton the other FPS-F14 cards extend.

## Deliverables
- Controls page reachable from Settings, listing every player action with its current primary input.
- Capture of a key or mouse button into the primary slot, staged until Apply; Back discards.
- Applied bindings take effect at once and survive a relaunch.

## Acceptance criteria
```gherkin
Scenario: Controls lists every action
  Given the player opens Controls in Settings
  Then every player action is listed with its current primary input

Scenario Outline: A rebound input does the action
  Given the player captures <input> for <action> and chooses Apply
  When the player uses <input> in the arena
  Then <effect>
  And the action's old input no longer does it
  Examples:
    | action       | input              | effect                 |
    | Reload       | T                  | the player reloads     |
    | Fire         | right mouse button | the rifle fires        |
    | Move forward | Up arrow           | the player moves ahead |

Scenario: A rebinding survives a relaunch
  Given the player applied a rebinding
  When the player relaunches Searchlight
  Then Controls shows the new input and the game uses it

Scenario: Back discards a captured input
  Given the player captured a new input and did not Apply
  When the player goes back
  Then the old input still works and Controls shows it when reopened

Scenario Outline: Reserved keys are never captured
  Given the player is capturing an input for Reload
  When the player presses <key>
  Then <result>
  And Reload's input is unchanged
  Examples:
    | key         | result               |
    | Escape      | capture is cancelled |
    | console key | capture ignores it   |

Scenario: An input in use is refused until the conflict dialog exists
  Given Lean left is on Q
  When the player captures Q for Reload
  Then Controls says Q is used by Lean left
  And neither action changes
```

## Verification
1. Rebind Fire to the right mouse button and Move forward to Up arrow, Apply, play a wave: both work, and the left button no longer fires.
2. Relaunch: both rebindings hold.

## Notes
FPS-F14-02's dialog replaces the last scenario.
