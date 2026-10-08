# In-game time advances only through the engine's tick

**Feature:** [ENG-F11](_feature.md)

## Summary

`GameClock` (time of day, sun and moon) moves from `moho_game` into `moho_app`. The loop
advances it once per tick, scaled by a time scale; anything else that changes the time
is an explicit reset. Takes over ENG-F6's tick criterion.

## Deliverables

- The engine owns `GameClock`; no game crate can advance it.
- The strategy game's day/night runs as before: it stops outside Playing and resumes in it.

## Acceptance criteria

```gherkin
Scenario: The clock advances only when a tick runs
  Given a clock at 12:00
  When a frame shorter than one tick runs
  Then the clock still reads 12:00

Scenario Outline: The time scale scales the advance
  Given a clock at 12:00 with time scale <scale>
  When the loop steps 60 ticks
  Then the clock has advanced <scale> times the scale-1 advance

  Examples:
    | scale |
    | 0     |
    | 2     |

Scenario: A skip is an explicit reset
  Given a clock at 12:00
  When the game resets it to 18:30
  Then the clock reads 18:30 before any tick runs

Scenario Outline: Strategy day/night runs only while playing
  Given the strategy game in <state> at 12:00
  When 60 ticks run
  Then the time of day is <result>

  Examples:
    | state       | result    |
    | Playing     | advanced  |
    | Menu        | 12:00     |
    | Paused      | 12:00     |
    | ConsoleOpen | 12:00     |
```

- [ ] Autosave then load restores the time of day (existing test, still passing).
- [ ] `moho_game` has no clock type and no clock field.

## Tech spec

**Design.**
- `moho_game::game_clock` moves to `moho_app::clock::GameClock` with its sun/moon
  math, day/night lengths and `time_string`. `tick(dt)` becomes `pub(crate) advance`,
  called only by `run_tick` after `Game::tick`, by `tick_length × time_scale`. So
  game code can hold `&mut GameClock` and still can't advance it.
- New public API: `set_time_scale(f32)` (negative or non-finite clamps to 0),
  `time_scale()`, `reset_to(hours)` (replaces `set_time`; wraps like today).
- `LoopConfig` carries the initial `GameClock`; the loop owns it. `TickContext` gets
  `clock: &mut GameClock`, `FrameContext` gets `&GameClock`.
- `moho_game::SimulationController` loses `game_clock`, `with_clock`, `game_clock[_mut]`,
  `celestial_directions`, `time_of_day` and `set_time_of_day`. `apply_input` stops
  advancing time. This is required, not cleanup: `moho_game` is a domain crate and
  can't depend on `moho_app` (which reaches winit), so it can't name the clock.
- Strategy binary: at the start of each tick it sets the scale to 1 in Playing and 0
  otherwise, which matches today's "clock moves only in `update_game_state`". Scene
  load and the console `time` command call `reset_to`. Lighting, HUD and autosave read
  the clock from their contexts.

**Out of scope.**
- A per-game calendar abstraction or a second clock (ADR-0009: one seam).
- Saving `elapsed_seconds` or the time scale; save format unchanged.
- Routing the console `time` command through a command (ADR-0011 rule 5; it already
  runs inside the tick here).
- Seeded RNG (stays with ENG-F6).

**Test map** (gate class: **domain**: removes rules from `moho_game`; tests reviewed first).
| Scenario | Test |
|---|---|
| Advances only on tick | `moho_app::clock::tests::frame_without_tick_leaves_clock_unchanged` |
| Time scale outline | `moho_app::clock::tests::time_scale_scales_the_advance` |
| Reset | `moho_app::clock::tests::reset_to_sets_time_without_a_tick` |
| Clamp | `moho_app::clock::tests::negative_or_nan_time_scale_clamps_to_zero` |
| Strategy states outline | `moho::app::game::tests::time_of_day_advances_only_while_playing` |
| Load restores time | existing `moho::app::autosave::tests::autosave_then_load_restores_actors_camera_time_and_blocks` |
| Moved clock tests | `moho_game::game_clock::tests::*` → `moho_app::clock::tests::*` |

**Risks.**
- `SimulationController`'s public API shrinks; callers are the binary only (confirm
  with GitNexus `impact` and a text search).
- Depends on ENG-F11-02 (the strategy must be on the loop before the loop advances its clock).
