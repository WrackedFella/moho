# A game's tick runs at a fixed rate whatever the frame rate

**Feature:** [ENG-F11](_feature.md)
**Issue:** [#143](https://github.com/WrackedFella/moho/issues/143)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

Creates `moho_app` with the game plug-in interface and the
[ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md) accumulator, steppable
without a window. Nothing runs on it yet; ENG-F11-02 moves the strategy game onto it.

## Deliverables

- An engine-row crate `moho_app` exposing the plug-in interface (init, command,
  tick, frame, event) and a headless loop that advances by frame time or by tick count.
- Each tick takes the game's per-tick command as input
  ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md) rule 1).

## Acceptance criteria

```gherkin
Scenario Outline: Frame rate does not change simulation results
  Given a game whose tick integrates a scripted command sequence
  When the loop runs 600 ticks fed by frames of 1/<fps> s
  Then the game state equals the 60 fps result

  Examples:
    | fps |
    | 30  |
    | 144 |

Scenario: Catch-up is capped
  Given a loop with a catch-up cap of 5 ticks
  When one frame arrives 1 s after the previous one
  Then 5 ticks run in that frame
  And a following 17 ms frame runs 1 tick

Scenario: A frame shorter than a tick runs no tick
  When a frame of 1/144 s arrives on a fresh 60 Hz loop
  Then no tick runs
  And the frame receives an interpolation alpha of 60/144

Scenario: The tick runs headless
  Given a game with no window
  When the loop is stepped 10 ticks
  Then the game's tick ran 10 times
  And it received tick indices 0 to 9 in order

Scenario: Each tick receives the command sampled for it
  Given a game whose command() returns 1, 2, 3 on successive calls
  When one frame runs 3 ticks
  Then the ticks receive commands 1, 2 and 3 in that order
```

## Tech spec

**Design.**
- New crate `moho_app` (engine row, [ADR-0012](../../adr/0012-engine-crate-map-for-m2.md)). This card's code
  names no winit, renderer or audio type; ENG-F11-02 adds the windowed runner.
- `pub trait Game`: `type Command`; `init(&mut self, ctx)`, `command(&mut self) -> Self::Command`,
  `tick(&mut self, ctx: &mut TickContext, command: &Self::Command)`,
  `frame(&mut self, ctx, alpha: f32)`, `event(&mut self, ctx, event)`. This card
  defines `command`, `tick` and `frame`; ENG-F11-02 adds `init` and `event` with their
  contexts. One trait with an associated type: static dispatch, no second interface.
- `TickContext { tick: u64, tick_length: Duration }`. Domain code gets time only as tick
  index or length (ADR-0009).
- `LoopConfig { tick_hz: u32, max_catch_up_ticks: u32 }` (default cap 5). The game
  supplies `tick_hz`; ADR-0009 keeps the 60 Hz value on the game side.
- `FixedStep` holds the accumulator as an integer: `frame_dt.as_nanos() * tick_hz`,
  one tick per `1_000_000_000`. 1/60 s isn't a whole number of nanoseconds, so a
  `Duration` accumulator drifts and a float one jitters; this doesn't. Over the cap, the
  excess time is dropped, keeping only the sub-tick remainder. `alpha = remainder / 1e9`.
- `HeadlessLoop<G>`: `advance(&mut G, frame_dt) -> u32` (ticks run, then `frame(alpha)`)
  and `step(&mut G, n)` (n ticks, no frame). Both go through one private
  `run_tick` that calls `command()` once and then `tick()`. ENG-F11-02's runner reuses
  `run_tick`, so the windowed path is the tested path.

**Out of scope.**
- Window, renderer, audio and the strategy game (ENG-F11-02); `GameClock` (ENG-F11-03).
- Input mapping (ENG-F12), rendering faster than the tick, a variable or per-game
  catch-up policy beyond the cap.
- Root `Cargo.toml` and `scripts/layering.txt` gain the `moho_app` member and engine-row
  entry in this card's PR ([ADR-0012](../../adr/0012-engine-crate-map-for-m2.md) consequence); nothing else in those files.

**Test map** (gate class: **glue**; `moho_app` is not a domain path).
| Scenario | Test |
|---|---|
| Frame rate outline | `moho_app::headless::tests::frame_rate_does_not_change_simulation_results` (30 and 144 vs 60) |
| Catch-up capped | `moho_app::fixed_step::tests::catch_up_after_stall_runs_at_most_the_cap` |
| Short frame | `moho_app::fixed_step::tests::frame_shorter_than_tick_runs_none_and_reports_alpha` |
| Headless | `moho_app::headless::tests::step_runs_tick_n_times_in_order` |
| Commands per tick | `moho_app::headless::tests::each_tick_receives_its_own_command` |
| Edge: no drift | `moho_app::fixed_step::tests::sixty_frames_of_one_sixtieth_run_exactly_sixty_ticks` (proptest over frame splits summing to 1 s: always 60 ticks) |

**Risks.** Low: new crate, no callers. The `Game` trait is seam 1 on ADR-0012's list,
so its shape is frozen at M3; ENG-F11-02 is its first real test.
