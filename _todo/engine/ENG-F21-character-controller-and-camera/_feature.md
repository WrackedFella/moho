# ENG-F21 — A game gets a character controller and camera from the engine

**Issue:** [#123](https://github.com/WrackedFella/moho/issues/123)
**Status:** Backlog
**Labels:** feature, line:engine

## End state

Both games move characters and point cameras through one engine controller and
camera, driven by the game's own input commands on the fixed tick
([ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md)). The FPS line
builds first-person play on it without copying strategy code.

## Summary

The character controller and camera live with the strategy game today, so the
FPS prototype would have to copy them, which is a fork. When this ships, a game
hands the engine input commands each tick and gets a moving character and a
camera that follows it. Consumers: FPS prototype P.2 (first-person) and the
strategy game (its existing free-fly and walking modes). The strategy game
moves first, so its unchanged behaviour proves the move.
**Moves toward the end state by:** removing the last generic piece P.2 would
otherwise copy, and adding the controller to the seam list that
[ENG-F5](../ENG-F5-physical-repo-split/_feature.md)'s gate freezes.

## Exit criteria

- A character moves from game-supplied input commands on the fixed tick:
  ```gherkin
  Scenario Outline: Frame rate does not change where the character ends up
    Given the same input command sequence
    When the character is driven for N ticks at <fps> frames per second
    Then its position equals the 60 fps result

    Examples:
      | fps |
      | 30  |
      | 144 |
  ```
- The camera follows the character without writing simulation state (test).
- The engine-side controller and camera contain no strategy type, and the
  layering check passes with an FPS-line crate using them.
- The strategy game's controller and camera behave as before (existing tests;
  manual check).

## Scope

- In: character movement from input commands, a camera that follows a
  character, the strategy game moved onto both.
- Out: weapon handling, head bob and other game feel (game lines); third-person
  modes.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Engine feature or per-game code? | Engine feature (Justin, 2026-10-07) | Per-game controllers mean the FPS line copies strategy code, which the layering check would then have to tolerate |
| Which crate holds the controller? | **Open, for the Tech Lead** (`moho_physics` or another engine-row home, per [ADR-0012](../../adr/0012-engine-crate-map-for-m2.md)) | Decided in the spec, not here |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Third-person camera modes | No consumer yet | A game line asks for one |
| Game feel (head bob, sway, weapon handling) | Game-specific; belongs to each line | Never in the engine |

## Items

| Item |
|---|

## Notes

- Depends on [ENG-F11](../ENG-F11-shared-app-loop/_feature.md) (fixed tick),
  [ENG-F12](../ENG-F12-input-actions/_feature.md) (input commands) and
  [ENG-F15](../ENG-F15-physics-queries-and-bodies/_feature.md)'s character
  handles.
