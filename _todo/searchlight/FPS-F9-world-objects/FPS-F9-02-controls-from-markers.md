<!-- board: gate=unset; eligible=no; priority=P3 -->
# A level author places a control and names its action in the level file

**Feature:** FPS-F9 (#616)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A control's action and its values come from its marker in the level, from a closed list
the line offers, so a new control needs no code. The arena gains a god-mode switch beside
the next-wave button as the second action. Later actions join the list with their card:
give items (#598), spawn an enemy and reset targets (FPS-F10-02). Depends on FPS-F9-01.

## Deliverables
- A control marker names one action and that action's values.
- Actions on the list: next wave (FPS-F9-01) and god mode toggle.
- A level naming an unknown action, or leaving out a value the action needs, is refused
  at load with the marker's name.
- The prompt names each control's own action.

## Acceptance criteria
```gherkin
Scenario Outline: Each control does what its marker names
  Given the arena has a control whose marker names <action>
  When the player uses it
  Then <result>

  Examples:
    | action          | result                          |
    | next wave       | the waiting wave spawns         |
    | god mode toggle | god mode turns on, or off if on |

Scenario: Each control's prompt names its action
  Given the player looks at the god-mode switch within reach
  Then the prompt names the use key and "God mode"

Scenario: An unknown action is refused at load
  Given a level whose control marker names an action not on the list
  When the level loads
  Then loading fails naming that marker

Scenario: The god-mode switch is a host action
  Given god mode is off
  When a player in a slot other than 0 uses the god-mode switch
  Then god mode stays off
```

## Verification
- Add a second next-wave button to the arena file by marker alone: it works.

## Notes
- TL: the god-mode switch and the console `tgm` command set the same state.
