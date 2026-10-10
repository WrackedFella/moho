# ENG-F21 — A game gets a character controller and camera from the engine

**Status:** Draft (proposed edits to approved #123: decisions, deferrals and items added; exit criteria and scope unchanged)
**Labels:** feature, line:engine

## End state

Both games move characters and point cameras through one engine controller and
camera, driven by the game's own input commands on the fixed tick
(ADR-0009). The FPS line builds first-person play on it without copying strategy code.

## Summary

The character controller and camera live with the strategy game today, so the
FPS prototype would have to copy them, which is a fork. When this ships, a game
hands the engine input commands each tick and gets a moving character and a
camera that follows it. Consumers: FPS prototype P.2 (first-person) and the
strategy game (its walking mode). The strategy game moves first, so its unchanged
behaviour proves the move.
**Moves toward the end state by:** removing the last generic piece P.2 would
otherwise copy, and adding the controller to the seam list that ENG-F5's gate freezes.

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

- In: character movement from input commands, a first-person camera that follows a
  character, the strategy game's walking mode moved onto both.
- Out: weapon handling, head bob and other game feel (game lines); third-person
  modes; the strategy game's isometric camera and free-fly mode (strategy-only, they stay).

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Engine feature or per-game code? | Engine feature (Justin, 2026-10-07) | Per-game controllers mean the FPS line copies strategy code, which the layering check would then have to tolerate |
| Which crate holds the controller? | **Open, for the Tech Lead** (`moho_physics` or another engine-row home, per ADR-0012) | Decided in the spec, not here |
| Which modes move to the engine? | Walking and the first-person camera. Isometric and free-fly stay in the strategy line | Placement test: the FPS reuses walking and first-person as-is; nothing else reuses isometric or free-fly |
| Who sets speeds, jump strength, capsule size and eye height? | The game, as data it may change between ticks | FPS speed depends on stamina and carry weight (GDD §3); a fixed engine speed would be forked |
| Where does look (yaw, pitch) come from? | The per-tick command (ADR-0011); the camera only reads | Keeps the sim the only writer |
| Smooth camera between ticks? | The camera eye interpolates between the last two ticks | Without it the view judders at any refresh rate but 60 Hz, and each game would add its own smoothing |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Third-person camera modes | No consumer yet | A game line asks for one |
| Game feel (head bob, sway, weapon handling) | Game-specific; belongs to each line | Never in the engine |
| Crouch and prone (capsule and eye height change) | Open question for Justin; GDD lists crouch, prone is a cut candidate | FPS Phase 1 playtest, or now if Justin says so |
| Mantle, ladders, swimming | GDD: no mantle in Phase 1 | FPS asks |

## Items

| Item |
|---|
| ENG-F21-01 A character walks and jumps from per-tick commands |
| ENG-F21-02 A first-person camera follows a character smoothly |

## Notes

- Depends on ENG-F11 (fixed tick), ENG-F12 (input commands) and ENG-F15-01 (character handles).
- Order: 01 then 02.
