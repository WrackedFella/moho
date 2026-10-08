# A gamepad drives the same actions

**Feature:** [ENG-F12](_feature.md)
**Issue:** [#151](https://github.com/WrackedFella/moho/issues/151)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

Both games need controller support, and the FPS line needs stick look. After
this card gamepad buttons and stick directions bind to actions like keys, and
the right stick adds to the look delta. Uses `gilrs`
([ADR-0008](../../adr/0008-keep-winit-for-windowing-and-input.md), confirmed by the ENG-F13 audit).

## Deliverables

- Gamepad buttons and stick directions are bindings, written in prefs like keys.
- The right stick turns the view through the same look delta.
- Connecting or unplugging a pad mid-game works; unplugging releases its actions.
- The strategy game ships default pad bindings.

## Acceptance criteria

```gherkin
Scenario: A pad button drives an action
  Given "jump" bound to Pad South
  When South goes down and the tick ends
  Then the frame reports "jump" pressed

Scenario Outline: A stick direction is a digital action past its threshold
  Given "move_forward" bound to Pad LeftStick Up
  When the left stick's Y reads <y> and the tick ends
  Then "move_forward" held is <held>

  Examples:
    | y    | held  |
    | 0.3  | false |
    | 0.6  | true  |
    | -0.9 | false |

Scenario: The right stick adds to the look delta
  Given the right stick held fully right
  When a tick ends
  Then the frame's look delta has a positive x proportional to one tick

Scenario: Unplugging a pad releases its actions
  Given Pad South held for "jump"
  When the pad disconnects and the tick ends
  Then "jump" is released

Scenario: No gamepad support on this machine
  Given the gamepad backend fails to start
  When the game runs
  Then a warning is logged once
  And keyboard and mouse still drive actions
```

## Tech spec

**Design.**
- Engine-owned pad types in `moho_input` (`PadButton`, `Stick`, `StickDir`),
  so `gilrs` stays inside one module and its upgrades touch nothing else.
  `Binding` gains `Pad(PadInput)`: `Button(PadButton)` | `Stick(Stick, StickDir)`.
  Names: `Pad South`, `Pad DPadUp`, `Pad LeftStick Up`.
- `ActionMap` gains `pad_button(PadButton, bool)`, `pad_axis(Stick, axis, f32)`,
  `pad_disconnected()`. Stick direction held when its axis is ≥ 0.5 (a named
  const). Right stick: radial dead zone 0.15, then
  `look += value × PAD_LOOK_PER_TICK × sensitivity` at `end_tick`, so stick look
  is per tick, never per frame.
- `moho_input::gamepad::Gamepads` wraps `gilrs::Gilrs`; `poll(&mut ActionMap)`
  drains `next_event()` each frame and maps `gilrs::Button`/`Axis` to the
  engine types (`ButtonPressed/Released`, `AxisChanged`, `Disconnected`;
  others ignored). `Gilrs::new()` failure → `None` plus one warning.
  All connected pads feed one map (single local player).
- `gilrs = "0.11"` in `[workspace.dependencies]`, used by `moho_input` only.
  Linux needs `libudev-dev`, already installed by CI and `scripts/cloud-tools.sh`.
- Strategy defaults: left stick → move actions, South → `jump`,
  right/left triggers → `ascend`/`descend`, left thumb → `sprint`.

**Out of scope.** Analog movement and trigger pressure (deferred). Pad rows in
the settings screen. Rumble. Per-player pads. A configurable stick look rate.

**Test map** (glue).
| Scenario | Test |
|---|---|
| Pad button | `moho_input::action_map::tests::pad_button_drives_bound_action` |
| Stick threshold | `…::stick_direction_held_past_threshold` (cases table) |
| Right stick look | `…::right_stick_adds_per_tick_look` |
| Unplug | `…::pad_disconnect_releases_pad_bindings` |
| No backend | `moho_input::gamepad::tests::backend_failure_yields_none` (inject the error path) |
| Names | `moho_input::binding::tests::pad_bindings_round_trip_through_names` |
| gilrs mapping | `moho_input::gamepad::tests::gilrs_buttons_map_to_pad_buttons` (every variant) |

**Gate class:** glue.

**Risks.** `just deny` must pass with `gilrs`'s tree (MIT/Apache; checked in
the ENG-F13 audit). A pad disconnect with a keyboard key on the same action
must not release the key's hold: release only pad bindings.

## Verification

- With an Xbox-layout pad: walk with the left stick, look with the right
  stick, jump with South; unplug and replug mid-game.
