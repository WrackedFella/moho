<!-- board: gate=unset; eligible=no; priority=P3 -->
# Controls in the proving ground spawn an enemy of a chosen grade and reset the targets

**Feature:** FPS-F10 (#617)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Two new actions join the control list: spawn one enemy of a named grade at a named
marker, and reset every target. The proving ground gets one spawn control per grade and a
reset control at the firing line. Depends on FPS-F10-01, FPS-F9-02 and #585 (FPS-F5-01,
grades).

## Deliverables
- A spawn action naming a grade and an enemy marker; a reset action.
- Five spawn controls, one per grade, and one reset control in the proving ground.
- Reset brings every dummy back standing and unhurt and removes spawned enemies.
- Both are host actions.

## Acceptance criteria
```gherkin
Scenario Outline: A spawn control brings one enemy of its grade
  Given the proving ground has no enemies
  When the player uses the <grade> spawn control
  Then one <grade> enemy appears at its marker

  Examples:
    | grade        |
    | rookie       |
    | professional |

Scenario: Spawn controls add to the enemies already there
  Given one rookie enemy is alive
  When the player uses the rookie spawn control
  Then two rookie enemies are alive

Scenario: The grade force overrides a spawn control
  Given the console grade force is set to veteran
  When the player uses the rookie spawn control
  Then one veteran enemy appears

Scenario: Reset restores the targets
  Given two dummies are dead and one enemy is alive
  When the player uses the reset control
  Then every dummy stands unhurt
  And no enemy remains
```

## Verification
- Spawn a rookie, then a professional, and fight each from the same cover: the difference
  shows (FPS-F5 grades).

## Notes
- The force-overrides rule follows #587 (a forced grade overrides the mix).
- TL: the grade of a spawned enemy needs to be readable in a test; it is not shown in play.
