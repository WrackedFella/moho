<!-- board: gate=unset; eligible=no; priority=P2 -->

# A round hits for less the slower it arrives

**Feature:** FPS-F15 (#745)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A hit's damage comes from the round, scaled by how fast it is still going: full at muzzle velocity, half at half speed, then body-part rules as today. Damage moves from the weapon's definition to the round's.

## Deliverables
- Round definitions carry damage; weapon definitions no longer do.
- Damage of a flying round scales in proportion to its arrival speed.

## Acceptance criteria
```gherkin
Scenario: A far target takes less than a near one
  Given dummies 10 m and 90 m from the player
  When each takes one round of the same type to the torso
  Then the far dummy loses less health than the near one

Scenario Outline: Damage is in proportion to arrival speed
  Given a round with damage 30 and muzzle velocity 900 m/s
  When it strikes a torso arriving at <speed> m/s
  Then the hit's damage before body-part rules is <damage>
  Examples:
    | speed | damage |
    | 900   | 30     |
    | 450   | 15     |
    | 90    | 3      |

Scenario: A hit always does some damage
  Given a round arriving just above its minimum speed
  When it strikes a torso
  Then the hit does at least 1 damage

Scenario: Body-part rules still apply
  Given two rounds of the same type arriving at the same speed
  When one strikes a head and one a torso
  Then each does what its body part does today to that damage

Scenario: The round sets the damage, not the weapon
  Given two round definitions for the rifle with different damage
  When each strikes a torso at muzzle velocity
  Then each does its own damage

Scenario: A weapon definition that still names damage is refused
  Given a weapon definition with a damage field
  When the content loads
  Then loading fails with a message naming the weapon and the field

Scenario: Simple does full damage at any distance
  Given Simple is on
  When dummies 10 m and 90 m away each take a torso hit
  Then both lose the same health
```

## Tech spec

## Verification
1. Shoot a dummy at the arena's longest sightline and one up close; the far one survives more hits.

## Notes
- Rounding below a whole point is the TL's call; a hit never rounds to 0.
- Enemy rounds follow the same rule once FPS-F15-03 is in; whichever card lands second covers the player taking it.
