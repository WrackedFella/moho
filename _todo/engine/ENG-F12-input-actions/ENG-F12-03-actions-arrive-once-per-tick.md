# The game reads its actions once per tick

**Feature:** [ENG-F12](_feature.md)
**Issue:** [#150](https://github.com/WrackedFella/moho/issues/150)
**Status:** unknown
**Gate class:** glue
**Labels:** line:engine

## Summary

The game polls live key state and a separate mouse-smoothing system. After this
card the engine's action map takes device input (keys, mouse buttons, mouse
motion) and hands the game one frame of actions and look delta per tick: the
input to ENG-F11's tick ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md) rule 1).

## Deliverables

- One per-tick action frame: held, pressed and released actions plus the look delta.
- Mouse buttons bind like keys.
- Mouse-delta filtering moves from `moho_core::input` into the action map; that module is deleted.
- The strategy game moves, sprints, jumps and looks as before, reading only action frames.

## Acceptance criteria

```gherkin
Scenario: A held key keeps its action held across ticks
  Given "move_forward" bound to W
  When W goes down and two ticks end
  Then both frames hold "move_forward"
  And only the first reports it pressed

Scenario: A tap shorter than a tick still registers
  Given "jump" bound to Space
  When Space goes down and up before the tick ends
  Then that frame reports "jump" pressed and released
  And does not hold it

Scenario: A mouse button drives an action
  Given "fire" bound to Mouse Left
  When the left button goes down and the tick ends
  Then the frame reports "fire" pressed

Scenario: Look delta is what moved since the previous tick
  Given filtering off and sensitivity 2
  When the mouse moves (1, 0) then (2, 1) and the tick ends
  Then the frame's look delta is (6, 2)
  And the next frame's look delta is (0, 0)

Scenario: Losing focus releases everything
  Given W held for "move_forward"
  When the window loses focus and the tick ends
  Then "move_forward" is released and not held

Scenario: Two keys on one action release it only when both are up
  Given "move_forward" bound to W and ArrowUp, both down
  When W goes up and the tick ends
  Then "move_forward" is still held
```

- [ ] The filter pipeline's existing tests pass in the action map's crate.
- [ ] `moho_core::input` no longer exists.

## Tech spec

**Design.**
- `moho_input::ActionMap<A: Action>` owns `ActionBindings<A>` (02), held
  bindings, this tick's press/release edges, the look accumulator and the
  `FilterPipeline` (moved verbatim from `moho_core::input`; `InputSystem`'s
  accumulate/sample logic folds in). Inputs are platform-free:
  `key(Key, bool)`, `mouse_button(MouseButton, bool)`, `mouse_motion(f64, f64)`,
  `release_all()`, `reset_look()`, `set_sensitivity`, `set_filtering`.
  A `winit` adapter fn feeds it `WindowEvent`/`DeviceEvent`, so device
  handling is in one place and tests need no window.
- `end_tick(&mut self) -> ActionFrame<A>`: the only read. `ActionFrame` is
  `Copy` plain data (`held`, `pressed`, `released` as `u64` masks indexed by
  position in `A::ALL`, const-asserted ≤ 64; `look: (f32, f32)`), with
  `held(A)`, `pressed(A)`, `released(A)`, `look()`. Plain data so a later
  replay or netcode ADR can store it (ADR-0011 rule 2).
- `Binding` gains `Mouse(MouseButton)` (`Left`, `Right`, `Middle`), named
  `Mouse Left` etc. in prefs.
- Strategy: `InputState.active_keys` and `.system` become one
  `ActionMap<StrategyAction>`; `update_controller_input` and the jump flag read
  the frame. Until ENG-F11 lands, the binary calls `end_tick` once per update,
  which matches today's per-frame sampling, so feel is unchanged.
  `clear_pending_input` call sites call `reset_look`.
- ENG-F11 meeting point: the game owns its `ActionMap` and feeds it the raw
  events `moho_app` forwards to `Game::event`. The game's `Game::command()`,
  which the loop calls once before each tick, calls `end_tick` and converts
  the frame into the game's command type. A catch-up tick after the first gets
  no new edges and zero look. Whichever of this card and ENG-F11 lands second
  wires it.

**Out of scope.** Gamepad (04). Analog axes, action contexts, chords
(deferred in the feature). Per-frame look for presentation (ENG-F21). Tool
clicks routed through egui stay as they are; no strategy action is bound to a
mouse button yet.

**Test map** (`moho_input::action_map::tests::…`, glue).
| Scenario | Test |
|---|---|
| Held across ticks | `held_key_holds_action_and_presses_once` |
| Sub-tick tap | `tap_inside_one_tick_reports_press_and_release` |
| Mouse button | `mouse_button_drives_bound_action` |
| Look delta | `look_is_scaled_motion_since_previous_tick` |
| Focus loss | `release_all_releases_held_actions` + adapter test `focus_lost_event_releases_all` |
| Two keys, one action | `action_held_while_any_binding_is_down` |
| Filter tests moved | `filter::tests::*` (moved verbatim) |

Edge: `frame_masks_index_by_action_position` (property: for any press set, `held(a)` matches the set).

**Gate class:** glue (`moho_input`, binary wiring).

**Risks.**
- After ENG-F11, smoothing runs per tick instead of per frame: mouselook feel
  changes at frame rates other than 60 and becomes frame-rate independent.
  ENG-F21 owns any per-frame presentation smoothing.
- Overlaps ENG-F11's edits to the binary's input path; coordinate order.

## Verification

- Strategy at 60 fps: walk, sprint, jump, fly up/down and mouselook feel as before.
- Alt-tab while holding W: the player stops.
