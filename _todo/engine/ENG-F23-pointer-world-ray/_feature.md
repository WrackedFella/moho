# ENG-F23 — The game knows what is under the pointer

**Status:** Draft
**Labels:** feature, line:engine

## End state
Any game turns the player's pointer into world queries: the strategy game selects pawns, marks blocks and places buildings by clicking; a future editor picks what it edits. The engine supplies the pointer and the ray; each game decides what a hit means.

## Summary
A game reads the pointer position, gets pointer clicks as bindable actions on the next tick, and turns any screen point into a world ray from its camera. With ENG-F15's ray queries, that ray finds the surface or body under the pointer. Engine request #304; consumer SG-F5 (#309), the first strategy card that selects, then SG-F4 (#310) marking and SG-F3 (#312) placement.
**Moves toward the end state by:** unblocking every point-and-click order in the strategy game; ENG-F24 draws the result.

## Exit criteria
```gherkin
Scenario: The game sees where the pointer is
  Given the window has focus
  When the player moves the pointer
  Then the game's next tick reads the pointer's position on screen

Scenario: A click reaches the game as an action with its position
  Given the game binds the primary pointer button to an action
  When the player clicks
  Then the next tick receives that action with the pointer position at the click

Scenario: A click the UI takes does not reach the world
  Given a menu or HUD panel is under the pointer
  When the player clicks
  Then the game receives no pointer action for that click

Scenario Outline: A screen point becomes a world ray from the camera
  Given a camera with a known position, orientation and field of view
  When the game asks for the ray through <point>
  Then the ray starts at the camera and points <direction>
  Examples:
    | point                  | direction                          |
    | the screen centre      | along the camera's forward axis    |
    | a corner of the screen | along that edge of the view frustum |

Scenario: The ray agrees with the picture on a high-DPI display
  Given a window with a display scale above 1
  When the player clicks a visible surface
  Then the pointer ray hits that surface

Scenario: The game finds what is under the pointer
  Given a level surface under the pointer
  When the game casts the pointer ray
  Then the hit reports that surface and its world position
```

## Scope
- In: pointer position as engine input; pointer buttons as bindable actions; screen point to world ray for the engine's camera; the strategy game wired far enough to cast the pointer ray (walking skeleton).
- Out: what a hit means (pawn or block picking is strategy-line, SG-F5); drawing a highlight (ENG-F24).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does picking live? | The engine gives pointer and ray; the game interprets hits | Voxel blocks belong to the strategy line (ADR-0010); engine picking would need game types |
| When does a click take effect? | As a command on the next tick ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md)) | Reading the pointer inside the sim would break replay |
| Projection | Perspective now; orthographic not precluded | No orthographic camera exists; a top-down view may want one later |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Box (drag) selection | Not requested (#304) | SG-F5 or SG-F4 asks for multi-select |
| Hover events (pointer enters or leaves a thing) | The game can cast every tick | A consumer needs hover cost cut |
| Gamepad virtual cursor | Strategy is mouse-first | A gamepad strategy playtest |
| Pointer ray while the cursor is locked (FPS) | The FPS aims from the camera centre | An FPS cursor mode |

## Items
| Item | Order |
|---|---|
| A click in the strategy view reports the surface under the pointer (position, click action, ray, UI filter) | first: the walking skeleton |
| The pointer ray is right on high-DPI displays and after a window resize | after the first |

## Notes
- Ray queries exist (ENG-F15, #227); this feature adds no new query.
