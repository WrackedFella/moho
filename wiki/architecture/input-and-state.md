# Input and state

**Source:** `flagstone/src/input_dispatcher.rs`, `moho_input/src/`, `flagstone_ui/src/actions.rs`, `flagstone/src/app/renderer_setup.rs` (registrations),
`moho_core/src/pointer.rs`, `flagstone/src/app/game.rs` (`Game::event`, `poll_gamepads`), `flagstone/src/main.rs` (`handle_keyboard_input`), `flagstone_ui/src/app_state/game_state.rs`,
`flagstone_ui/src/app_state/state_coordinator.rs`, `flagstone_ui/src/screens/pause.rs`, `moho_ui_shell/src/console/` (console key routing).

## Input routing

Every `WindowEvent` the runner forwards reaches `Game::event` (`App::handle_window_event`) and goes through this order. The first stage that
consumes the event ends the path.

```mermaid
flowchart TD
    ev["WindowEvent"] --> tab{"Tab pressed<br/>and Playing?"}
    tab -- yes --> cam["toggle FirstPerson / Isometric<br/>stop"]
    tab -- no --> d200["priority 200: keybind capture<br/>only when the active screen is capturing"]
    d200 -- consumed --> stop1(["stop"])
    d200 -- no --> d100["priority 100: egui<br/>only when a menu or console is visible"]
    d100 -- consumed --> stop2(["stop"])
    d100 -- no --> d0["priority 0: forwarders<br/>wheel, left click"]
    d0 -- "sent to channel,<br/>unless UI is capturing" --> chan[("unconsumed InputEvent channel")]
    d0 -- no --> weh["close → auto-save and exit<br/>keyboard → handle_keyboard_input"]
    chan --> drain["drained by process_input_events<br/>(zoom, mine)"]
    weh --> hot["handle_keyboard_input<br/>global hotkeys, then game keys"]
```

`InputDispatcher` is a sorted `Vec<(priority, handler)>`. **Higher number runs first.**
This is the opposite of `EventBus::subscribe_with_priority` (lower first); the two are
independent.

Two properties that bite:

- Each input kind gets its own `register` call (wheel and mine-click are separate
  handlers), so a new input type is a new registration, not a new branch.
- A forwarder drops the event rather than blocking if it can't take the UI adapter lock.
- During gameplay, with a free cursor, egui sees pointer motion and mouse buttons (`EguiAdapter::handle_gameplay_event`, `feeds_egui`), so it knows what lies under the pointer and widgets such as the view-height slider can be dragged. While the cursor is free (strategy view), a mouse press over a panel is consumed and never reaches the game; in first person egui gets no pointer input. The mine-click forwarder leaves the press unconsumed, so the action map sees it too.

Global hotkeys in `handle_keyboard_input`, checked before game keys:

| Key | Effect |
|---|---|
| `` ` `` (the `console` binding) | Playing → console; ConsoleOpen → Playing. Routed by `App::route_console_input` through `Console::on_key` ahead of the dispatcher, see [console](console.md#input-routing) |
| `Esc` | ConsoleOpen → Playing (taken by `Console::on_key` before this handler, as the console key is); Playing → Paused; Paused → Playing while the pause window is the active screen (inert on Settings). Taken before the dispatcher when it pauses or resumes, so a focused egui button cannot swallow it; key repeats are ignored |
| `F3` | Toggle debug HUD |
| `F4` | Toggle chunk-boundary minimap |
| `Tab` | Toggle camera mode (handled before the dispatcher) |

Gameplay input is action-keyed. `moho_input` holds the platform-free `Key` and
`MouseButton` (`Key::from_winit` maps `PhysicalKey`), the `Action` trait (stable id plus
default bindings), `ActionBindings<A>`, and `ActionMap<A>`
(`moho_input/src/action_map.rs`). Each game declares its own action enum
(`StrategyAction` in `flagstone_ui/src/actions.rs`); the binary owns one `ActionMap` in
`InputState` (`flagstone/src/app/input_state.rs`).

`handle_window_event` and `handle_device_event` feed key, mouse-button and raw
mouse-motion events into the map. Gamepads feed it too: `Gamepads::poll`
(`moho_input/src/gamepad.rs`, the only module that names `gilrs`) drains backend events
into `pad_button` / `pad_axis` once per frame from `App::poll_gamepads`
(`InputState::gamepads`, `None` when the backend fails to start). Outside `Playing`, or
while the window lacks focus (`InputState::focused`), the events are discarded unapplied
and `pad_disconnected` releases pad bindings. Every menu and console transition (`App::apply_transition`,
`flagstone/src/main.rs`) also calls `ActionMap::release_all`, so nothing stays held across a menu or the
console and no press made there lands on resume; a key still down on resume acts only after it
is pressed again. Once per tick `App::command` (`flagstone/src/app/game.rs`) calls `end_tick`,
which returns an `ActionFrame` and resets the per-tick state:

- `held`: down at the end of the tick; `pressed` / `released`: edges at any point in the
  tick, so a tap shorter than a tick is not lost.
- `look`: the pending look that rendered frames sampled with `ActionMap::sample_frame(frame_dt)` since the last tick (read with `pending_look`); a tick consumes exactly what the last frame showed. The right stick adds `PAD_LOOK_PER_SECOND` times its deflection per second of frame time (after a
  small dead zone), on top of the mouse input below. The mouse part is the motion accumulated since the previous frame, smoothed when `LookSettings::smoothing`
  is on (a two-sample mean of raw counts, `MouseSmoothing` in `moho_input/src/filter.rs`, which loses no motion),
  then scaled by `LookSettings::look_per_count`. `ActionMap::set_look` applies new settings. Motion is unnegated; `App::command` inverts it for
  yaw and pitch.

A stick direction binding is down while its axis is at or past `STICK_DIRECTION_THRESHOLD`.
An action is held when any of its bindings is down. A frame holds at most 64 actions.
The map also tracks the pointer in physical pixels: `ActionFrame::pointer` is its position at the end of the tick (`None` outside the window), and `press_pointer` is its position at the last mouse press in the tick. `StrategyAction::Select` (left mouse by default) is the strategy view's primary pointer action.

Bindings persist in the `[bindings]` section of [prefs](../reference/prefs-format.md).

## Pointer picking

In the isometric view, a tick with `Select` pressed turns `press_pointer` into a world `Ray` with `moho_core::pointer::screen_ray` (view and projection of the frame the player saw, window size as viewport) and carries it in the command as `select`. The ray, not the pixel, is the command, so it replays without a window. The apply step casts it with `flagstone_game::raycast::raycast_surface` over `POINTER_RAY_RANGE` and stores the result in `App::pointer_hit`; a miss clears it. First-person never selects.

## GameState

`flagstone_ui::GameState` decides input routing, rendering, cursor and whether the
simulation runs. `can_transition_to` is the legality table; `StateTransitionCoordinator`
returns the UI visibility and the menu to show for a legal transition. The cursor
follows `cursor_grabbed(state, camera_mode)`, not the coordinator.

```mermaid
stateDiagram-v2
    [*] --> Menu
    Menu --> Playing: start game
    Playing --> ConsoleOpen: open console
    Playing --> Paused: pause
    Playing --> Menu: return to menu
    ConsoleOpen --> Playing: close console
    ConsoleOpen --> Menu: exit to menu
    Paused --> Playing: resume
    Paused --> Menu: exit to menu
```

Same-state transitions are allowed (no-op). Everything else is rejected, including
`Menu → ConsoleOpen` and `ConsoleOpen ↔ Paused`.

| State | Input goes to | Draws | Cursor | Simulation |
|---|---|---|---|---|
| `Menu` | menu | menu screens | free | stopped |
| `Playing` | game | world | grabbed in first person, free in the isometric view | running |
| `ConsoleOpen` | console text | world (frozen) + console | free | paused |
| `Paused` | pause menu | world (frozen) + menu | free | paused |

`GameState::home_screen` names the screen a state shows on entry and that Back returns to: `start` for `Menu`, `pause` for `Paused`, none for `Playing` and `ConsoleOpen`. The pause window (`PauseMenu`, screen `pause`) offers Continue, Settings and Exit to main menu. Continue and Exit publish `UiEvent::ResumeRequested` and `UiEvent::MainMenuRequested` from `process_menu_action`; the binary resumes through `StateTransitionCoordinator::resume_game`, and on a main-menu request `App::frame` autosaves, then shows the main menu.

`flagstone/src/game_state.rs` re-exports `flagstone_ui::GameState`; the UI adapter takes the same type. Per-frame gameplay steps check `== Playing` ([frame-loop](frame-loop.md)).
