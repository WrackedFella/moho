# The player sees their own body's shadow, and can show the hit volumes

**Feature:** FPS-F4
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
The player gets a placeholder body: one box per body part, built like the enemies' segmented
bodies, sitting on the player's hit volumes. It is hidden from the first-person view but casts a
shadow, so the player sees their posture on the floor and walls. A console toggle draws every
character's hit volumes for playtesting.

## Deliverables
- A segmented player body, one box per hit volume, that moves with the player.
- The body casts a shadow and is not drawn in the player's own view.
- The body follows the lean (FPS-F4-01) and later stances (slice 2) because it sits on the hit
  volumes.
- A hidden console command shows and hides outlines of every character's hit volumes, player and
  enemies.

## Acceptance criteria
```gherkin
Scenario: The body has one box per hit volume
  Given the arena is loaded
  Then the player has a body box at each of their six hit volumes

Scenario: The player's own view does not show the body
  Given the player looks straight ahead and straight down
  Then no part of the player's body is drawn in the view

Scenario: The body casts a shadow
  Given the player stands in sunlight beside a wall
  Then the player's body casts a shadow on the floor or the wall

Scenario: The body follows the lean
  Given the player is fully leaning right
  Then each body box sits at its leaned hit volume

Scenario Outline: The hit-volume toggle shows and hides outlines
  Given hit-volume outlines are <before>
  When the player runs the hit-volume console command
  Then hit-volume outlines are <after> for the player and every enemy
  Examples:
    | before | after  |
    | hidden | shown  |
    | shown  | hidden |

Scenario: Outlines are off at launch
  When the player launches Searchlight
  Then no hit-volume outlines are drawn
```

## Verification
1. Stand in sunlight beside a wall and lean left and right: the shadow leans with you.
2. Turn hit-volume outlines on: the boxes match where enemy shots land on you and where yours
   land on them.

## Notes
Slice 3 (#543) replaces the boxes with the rigged body; the toggle stays.
