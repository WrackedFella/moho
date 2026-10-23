# Enemies open fire, aim and keep firing by grade

**Feature:** FPS-F5
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary

Each enemy spawns with one of five grades: rookie, novice, experienced, veteran,
professional. Its grade sets how soon it fires after spotting the player, how far its
shots stray, and how often it fires. Toughness and damage don't depend on grade.

## Deliverables

- Five grades in ladder order, each with reaction time, aim error and fire cadence.
- Every grade is at least as quick, accurate and steady as the grade below it, and
  professional is strictly better than rookie on all three.
- Enemies spawn as *experienced* unless something sets another grade; experienced plays as
  the arena enemy does today.
- A console command forces the grade of new spawns, or clears the force.

## Acceptance criteria

```gherkin
Scenario: A professional opens fire before a rookie
  Given a rookie and a professional each spot the standing player at the same range
  When both engage
  Then the professional fires its first shot before the rookie does

Scenario: Accuracy rises up the ladder
  Given one enemy of each grade fires at a standing player at the same range for the same time
  When the hits on the player are counted
  Then each grade has hit at least as often as the grade below it
  And the professional has hit more often than the rookie

Scenario: A professional fires more often than a rookie
  Given a rookie and a professional each engage a standing player for the same time
  When their shots are counted
  Then the professional has fired more shots than the rookie

Scenario Outline: Grade does not change toughness
  Given an enemy of grade <grade>
  When the player lands the same sequence of hits on it as on an experienced enemy
  Then it dies on the same hit as the experienced enemy

  Examples:
    | grade        |
    | rookie       |
    | novice       |
    | veteran      |
    | professional |

Scenario Outline: Grade does not change damage dealt
  Given an enemy of grade <grade> hits the player once
  Then the player loses the same health as from an experienced enemy's hit

  Examples:
    | grade        |
    | rookie       |
    | professional |

Scenario: Unforced spawns are experienced
  Given no grade is forced
  When an enemy spawns
  Then its grade is experienced

Scenario: The console forces the grade of new spawns
  Given the player forces the grade to rookie in the console
  When the next enemies spawn
  Then every one of them is a rookie

Scenario: Clearing the force restores the normal grade
  Given the grade is forced to rookie
  When the player clears the force in the console
  Then the next enemy spawns as experienced

Scenario: An unknown grade is refused
  When the player forces the grade to "legend" in the console
  Then the console reports the five valid grades
  And new spawns are unchanged
```

## Verification

- Force rookie and fight a wave, then force professional: the rookie is slow to fire,
  misses more and fires less often, and both die to the same hits.
- With nothing forced the arena plays as it did before this card.
