<!-- board: gate=unset; eligible=no; priority=P2 -->

# A round bounces once off a world surface hit at a glancing angle

**Feature:** FPS-F15 (#745)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A flying round that meets level geometry at a shallow angle can bounce off near the mirror direction, keeping less of its speed the steeper the hit. The steeper the hit, the less likely the bounce; a square hit never bounces. A round bounces at most once, never off a body, and a bounced round can hit anyone, the shooter included. A ricochet sound plays.

## Deliverables
- One ricochet off world surfaces, decided by angle and a seeded roll from its own stream.
- A ricochet sound (placeholder asset, played like today's shot sound).

## Acceptance criteria
```gherkin
Scenario Outline: The angle decides the bounce
  Given a round strikes a wall at <angle> degrees from its surface
  When the hit is resolved
  Then the round <outcome>
  Examples:
    | angle | outcome                           |
    | 5     | bounces, on every seed            |
    | 45    | stops, on every seed              |
    | 90    | stops, on every seed              |

Scenario: Between the bands a roll decides
  Given rounds strike a wall at an angle between the always and never bands
  When many are fired with different seeds
  Then some bounce and some stop
  And fewer bounce at a steeper angle

Scenario: A bounced round goes on near the mirror heading
  Given a round bounces off a wall
  Then its new heading is within a few degrees of the mirror of its old one
  And it is slower than when it struck

Scenario: A bounced round can hit a target beyond
  Given a dummy placed on the mirror path of a glancing shot at a wall
  When the player fires the glancing shot
  Then the dummy is hit after the bounce

Scenario: A bounced round can hit its shooter
  Given a glancing shot whose mirror path returns to the player
  When the bounce happens
  Then the round can hit the player who fired it

Scenario: A round bounces only once
  Given a round that already bounced strikes a second wall at a glancing angle
  Then it stops

Scenario: Bodies never bounce a round
  Given a round strikes a dummy at a glancing angle
  Then the dummy is hit and the round stops

Scenario: A ricochet is heard
  Given a round bounces
  Then a ricochet sound plays

Scenario: Ricochet rolls do not change enemy spread
  Given the same seed and the same commands
  When one fight has a ricochet and the same fight is run with ricochets turned off for the test
  Then every enemy shot leaves on the same heading in both

Scenario: Simple has no bounce
  Given Simple is on
  When the player fires at a wall at a glancing angle
  Then the shot stops at the wall
```

## Tech spec

## Verification
1. Fire along a long wall at a shallow angle: a ricochet sound plays and a dummy past the bounce can be hit. Fire square at the same wall: no sound, no bounce.

## Notes
- Band edges and speed kept are tuning numbers for the TL (X-Ray: always at 18° or less, never from 30°, mirror jittered within 10°).
- Damage after a bounce follows FPS-F15-02 when it is in; enemy rounds bounce too when FPS-F15-03 is in.
