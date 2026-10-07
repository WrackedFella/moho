# ENG-F11 — A game runs on the engine without copying the app loop

**Issue:** #81


## Summary

Both games need the window, event loop and fixed-tick accumulator. Today they
live in the strategy binary, so the FPS line would have to copy them, which is
a fork. The engine owns the loop and a game plugs into it. Relies on
[ADR-0009](../../adr/0009-simulation-time-is-one-fixed-tick.md) and [ADR-0011](../../adr/0011-simulation-stays-network-ready.md) (proposed, being written).

## Exit criteria

- An engine-line crate (engine row in the layering check) owns the window,
  event loop, renderer/physics/audio setup and the ADR-0009 accumulator with
  a catch-up cap.
- A game plugs in through one interface: init, fixed tick (with input),
  frame (with interpolation alpha), event.
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

## Items

| Item |
|---|
