# Footsteps bob the view, and the player sets how much

**Feature:** FPS-F4
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The view bobs with the player's steps, more when sprinting than walking, and is still when
standing. A head-bob setting from 0 to 100 % scales it, is saved, and is set from the console
now and from the Settings screen once FPS-F3-03 #460 lands.

## Deliverables
- The view bobs while walking, more while sprinting, and not at all while standing still or
  airborne.
- Head bob never moves the aim ray or the player's hit volumes.
- A head-bob setting, 0 to 100 %, default moderate, saved across launches.
- A console command reads and sets it.
- #460's settings table gains a "Head bob | slider" row.

## Acceptance criteria
```gherkin
Scenario Outline: Bob follows movement
  Given head bob is at 100 %
  When the player is <moving> for a full step cycle
  Then the view's bob is <amount>
  Examples:
    | moving        | amount                      |
    | standing still| none                        |
    | walking       | present                     |
    | sprinting     | larger than walking         |
    | airborne      | none                        |

Scenario: 0 % keeps the view still
  Given head bob is at 0 %
  When the player sprints for a full step cycle
  Then the view does not bob

Scenario: Bob does not move the aim
  Given head bob is at 100 % and the player walks toward a target
  When the player fires at any point in the step cycle
  Then the shot leaves from the unbobbed eye along the view direction

Scenario: The setting survives a relaunch
  Given the player set head bob to 25 % from the console
  When the player relaunches Searchlight
  Then head bob is 25 %

Scenario: Out-of-range values are refused
  When the player sets head bob to 150 % from the console
  Then the console reports the valid range
  And head bob keeps its previous value
```

## Verification
1. Walk then sprint in the arena: the bob is visible, larger when sprinting, and feels grounded,
   not seasick.
2. Set head bob to 0 % from the console: the view is still while sprinting.

## Notes
The Settings slider behaves as every #460 setting (staged, Apply persists).
