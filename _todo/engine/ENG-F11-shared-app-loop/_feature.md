# ENG-F11 — A game runs on the engine without copying the app loop

**Issue:** #81


## Summary

Both games need the window, event loop and fixed-tick accumulator. Today they
live in the strategy binary, so the FPS line would have to copy them, which is
a fork. The engine owns the loop and a game plugs into it. Relies on
[ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md) and [ADR-0011](../../adr/0011-simulation-stays-network-ready.md).

## Exit criteria

- An engine-line crate (engine row in the layering check) owns the window,
  event loop, renderer/physics/audio setup and the ADR-0009 accumulator with
  a catch-up cap.
- A game plugs in through one interface: init, fixed tick (with input),
  frame (with interpolation alpha), event.
- The tick takes a per-tick command set as its input (ADR-0011 rule 1).
- Same input, any frame rate:
  ```gherkin
  Scenario Outline: Frame rate does not change simulation results
    Given the same input sequence
    When the loop runs N ticks at <fps> frames per second
    Then the simulation state equals the 60 fps result

    Examples:
      | fps |
      | 30  |
      | 144 |

  Scenario: Catch-up is capped
    Given a frame that arrives after a long stall
    When the loop catches up
    Then no more than the cap of ticks run in that frame

  Scenario: The tick runs headless
    Given a game with no window
    When the loop is stepped N ticks
    Then the game's fixed tick ran N times
  ```
- `GameClock` is engine-owned and advances only through the tick, with a
  time scale; a skip is an explicit reset (test). Takes over ENG-F6's tick
  criterion.
- The strategy game runs on the shared loop with unchanged behaviour
  (existing tests pass; manual check).

## Scope

- In: loop extraction, accumulator, headless stepping, `GameClock` move.
- Out: networking; scene/asset management; a second plug-in interface.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Who produces the per-tick command? | The game, through a `command()` hook the loop calls once before each tick | The loop owning an action map would tie `moho_app` to ENG-F12's design; the hook lets ENG-F12 change how the value is built without changing the seam |
| Raw winit events or an engine event enum at the seam? | Raw winit events | egui-winit needs them; an enum adds a translation layer with no consumer. Cost: a winit major upgrade changes seam 1 |
| Where does `GameClock` live? | `moho_app` (ADR-0012); `moho_game` stops holding it | `moho_game` is platform-free and can't depend on `moho_app` |
| Does physics setup move into the engine loop? | **Open, for Justin** (proposed: no, stays with the game until ENG-F15) | Moving it now drags the strategy's chunk colliders and KCC wiring into the engine with no second consumer |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Rendering faster than the tick, with interpolation | Needs camera interpolation; strategy runs at 60/60 | ENG-F21 (FPS camera) |
| Engine-owned physics stepping | No engine physics API yet | ENG-F15 |
| Bus requests become tick commands (ADR-0011 rule 5) | Their owners rework them | ENG-F12 (input), ENG-F18 (UI shell) |

## Items

| Item |
|---|
| [ENG-F11-01](ENG-F11-01-fixed-tick-whatever-the-frame-rate.md) a game's tick runs at a fixed rate whatever the frame rate |
| [ENG-F11-02](ENG-F11-02-strategy-game-runs-on-the-engine-loop.md) the strategy game runs on the engine's app loop |
| [ENG-F11-03](ENG-F11-03-in-game-time-advances-only-through-the-tick.md) in-game time advances only through the engine's tick |

## Notes

- Moves the binary's renderer setup, which names wgpu today. Add no new wgpu use outside the
  renderer crate; [ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md) contains it.
- Order: 01 → 02 → 03. 02 rewrites `src/main.rs` and `src/app/event_loop`; run it
  after, not alongside, any ENG-F10 card touching those files.
- ENG-F12-01 (moves `GameState` out of `moho_types`) lands before 02, so the loop moves
  with the final path. Whichever of ENG-F12-03 and 02 lands second calls the action map's
  per-tick read from the strategy's `Game::command()`.
