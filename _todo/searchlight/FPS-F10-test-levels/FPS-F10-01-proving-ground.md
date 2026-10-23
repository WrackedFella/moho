<!-- board: gate=unset; eligible=no; priority=P2 -->
# The tester launches the proving ground by name, and nothing attacks until asked

**Feature:** FPS-F10 (#617)
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A second baked level, the proving ground, runs in a sandbox mode: no waves, no score. It
has targets at marked distances, cover at the heights stance work tests, a slope, stairs
and a ledge to drop items from. Levels launch by name; the arena stays the default.

## Deliverables
- Levels are chosen by name at launch; no name loads the arena.
- An unknown name stops the launch and lists the level names.
- Each level file names its mode: arena (waves, score) or sandbox.
- The proving ground: dummies at 25, 50, 100 and 200 m from a firing line, each distance
  marked on the floor; cover at 0.6, 1.0 and 1.4 m; a slope, stairs and a ledge over a
  drop area.

## Acceptance criteria
```gherkin
Scenario Outline: The tester launches a level by name
  When the tester launches Searchlight with level "<name>"
  Then the <name> loads in <mode> mode

  Examples:
    | name            | mode    |
    | arena           | arena   |
    | proving_ground  | sandbox |

Scenario: No name loads the arena
  When the tester launches Searchlight with no level
  Then the arena loads

Scenario: An unknown name lists the levels
  When the tester launches Searchlight with level "nowhere"
  Then the launch stops
  And the message lists "arena" and "proving_ground"

Scenario: Nothing attacks in a sandbox
  Given the proving ground has loaded
  When 60 seconds pass
  Then no enemy has spawned
  And no wave or score shows

Scenario Outline: Targets stand at their marked distance
  Given the proving ground has loaded
  Then a dummy stands <distance> m from the firing line

  Examples:
    | distance |
    | 25       |
    | 50       |
    | 100      |
    | 200      |

Scenario: Death in a sandbox restarts in the sandbox
  Given the player is in the proving ground with god mode off
  When the player dies and restarts
  Then the proving ground reloads in sandbox mode
```

## Verification
- Shoot each dummy from the firing line: the 200 m dummy is visible and hittable, and its
  shadow draws.
- Crouch and lean behind each cover height once FPS-F4 slice 2 lands: each height covers
  what it should.

## Notes
- TL: the launch already takes a level file path; naming levels may only need a list.
- TL: a 200 m sightline may need the far plane and shadow cascades checked.
- TL: how a sandbox player dies (no enemies) matters only with a future fall or hazard.
