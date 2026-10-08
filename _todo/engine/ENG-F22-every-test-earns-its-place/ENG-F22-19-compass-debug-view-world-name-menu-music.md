# Compass, debug view, world name, menu music and volume sliders behave as decided

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#186](https://github.com/WrackedFella/moho/issues/186)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

Five small UI behaviours are settled: the compass shows its labels in left-to-right order, the debug-view console command rejects unknown modes, a new world keeps the name the player typed, the start menu's music keeps playing when the start menu is shown again, and volume sliders move in the same 0.1 steps the save file keeps.

## Deliverables

- The five behaviours below, each pinned by a test that failed before the change.

## Acceptance criteria

```gherkin
Scenario Outline: The compass lists visible labels left to right
  When the camera yaw is <yaw>
  Then the compass shows <labels> in that order, with <facing> marked as facing

  Examples:
    | yaw   | labels       | facing |
    | 0     | NW, N, NE    | N      |
    | π/2   | NE, E, SE    | E      |

Scenario: Unknown debug view modes are rejected
  When the player enters "r_debug_view 9"
  Then the console says the mode must be between 0 and 5
  And no debug view change is requested

Scenario: A new world keeps its name
  Given the player names a new world "Highlands"
  When the world is generated
  Then the new-world request carries the name "Highlands"

Scenario: Showing the start menu again keeps its music
  Given the start menu music is playing
  When the start menu is shown again
  Then no music stop is published

Scenario: Volume sliders keep what the save file keeps
  When the player drags a volume slider
  Then the value moves in steps of 0.1
```

## Tech spec

**Design**
- Compass: extract a pure `visible_labels(yaw) -> Vec<(label, facing)>` sorted by screen angle; `render_compass` draws from it.
- `r_debug_view`: reject ids above 5 the way the quality commands reject theirs.
- New World: map the spec's name into `NewWorldRequested` instead of the fixed "New World".
- Menu music: showing "start" while it is already playing is a no-op in `update_menu_music`.
- Volume sliders and their drag values use a 0.1 step; other sliders keep theirs.

**Test map**
| Scenario | Test |
|---|---|
| Compass order | `overlays::gameplay_hud` `compass_visible_labels_by_yaw` (replaces the three compass tests) |
| Debug view | `overlays::console::commands` `debug_view_rejects_unknown_mode` |
| World name | `adapter::event_routing` `generate_world_carries_the_typed_name` |
| Music on re-show | `adapter::event_routing` `menu_music_keeps_playing_when_start_is_shown_again` |
| Slider step | `screens::form_controls` `volume_slider_steps_by_tenths` (or Verification if egui gives no seam) |

**Out of scope:** `ToggleCollision { enabled: false }`; other console commands.

**Gate class:** glue (UI).

**Risks:** low. Lands after #180 (ENG-F22-13) or rebases onto it, since both touch the same test modules.

## Verification

In game: the compass reads NW, N, NE facing north; a world named "Highlands" shows that name; the start music doesn't cut out when returning to the start menu.
