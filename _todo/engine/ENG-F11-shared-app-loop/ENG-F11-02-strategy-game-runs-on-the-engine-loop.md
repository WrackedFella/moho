# The strategy game runs on the engine's app loop

**Feature:** [ENG-F11](_feature.md)
**Issue:** #144

## Summary

Moves the window, winit event loop, renderer and audio setup from the strategy binary
into `moho_app`, and makes the binary a `Game`. After this, a second game gets the
loop by implementing one trait instead of copying `src/app`.

## Deliverables

- `moho_app::run(game, config)` owns window creation, the winit event loop, renderer
  creation and resize, and audio initialisation.
- The binary implements `Game` and keeps no `ApplicationHandler` of its own.
- The strategy game behaves as before, at the same 60 Hz.

## Acceptance criteria

```gherkin
Scenario: The strategy simulation steps through the engine loop headless
  Given the strategy game in Playing with a dynamic sphere at height 50
  When moho_app's headless loop steps it 30 ticks
  Then the sphere has fallen below 49

Scenario: Strategy input reaches the tick as a command
  Given the strategy game in Playing with the forward binding held
  When one tick runs
  Then the command that tick received has forward input 1
  And the player has moved forward
```

- [ ] No `ApplicationHandler` impl remains in the binary; `main` builds the game and calls `moho_app::run`.
- [ ] `moho_app` depends on no strategy-line crate (layering check in `just check`).
- [ ] Existing binary tests pass, ported to drive the game through `Game` hooks.

## Verification

`cargo run`: menu, new world, walk (FPS + KCC), fly, Tab between camera modes, console
(`` ` ``) and `time`, F3/F4 overlays, Esc menu, quit (autosaves), relaunch and load.
Movement speed, day/night speed and frame pacing match `dev`.

## Tech spec

**Design.**
- `moho_app::run<G: Game + 'static>(game: G, config: AppConfig) -> Result<(), AppError>`.
  `AppConfig { window_title, loop_config: LoopConfig, init_audio: bool }`. An internal
  `ApplicationHandler` creates the window on `resumed`, calls
  `moho_renderer::create_renderer` (keeping today's one-time window leak and its
  SAFETY note), creates `moho_audio::AudioSystem` when asked, then calls `Game::init`.
- Timing reproduces today's 60 fps behaviour: on each wake it advances `FixedStep` by
  the time since the last wake, runs the ticks through ENG-F11-01's `run_tick`,
  requests a redraw, and sets `ControlFlow::WaitUntil(next tick deadline)`.
  `RedrawRequested` calls `Game::frame(alpha)`.
- `Resized` resizes the renderer, then is forwarded. `CloseRequested` is forwarded, then
  the loop exits. Every other `WindowEvent` and `DeviceEvent` goes to `Game::event`
  unchanged. egui-winit needs raw winit events, so the seam passes them through rather
  than translating to an engine enum.
- Contexts: `InitContext { window: &Arc<Window>, renderer: &mut dyn RendererBackend,
  audio: Option<&mut AudioSystem> }`; `FrameContext` and `EventContext` carry the same
  renderer/audio access plus `request_exit()`. Headless, `init` is not called and
  `frame` gets no renderer (`Option`).
- Binary: `App` implements `Game`. `Command` is a value snapshot of today's input:
  `ControllerInput` fields, jump, and the look delta from `sample_frame_input`.
  `command()` builds it (body of `update_controller_input`); `tick` applies it and runs
  what `process_frame` and `new_events` do today except presentation: game state,
  physics, chunk streaming, light propagation, bus event processing, generation
  polling. `frame` does lighting upload, HUD data and the scene render. `init` keeps the
  game half of `renderer_setup` (mesh and material registration, egui adapter,
  dispatcher subscribers, video settings). `event` keeps Tab, dispatcher and
  keyboard/mouse handling.
- `moho_game::TICK_HZ: u32 = 60` (ADR-0009's constant) feeds `LoopConfig`. The HUD's
  frame time reads the tick length.
- `.cargo/mutants.toml` excludes the winit `ApplicationHandler` methods in `moho_app`,
  with the reason (need a display), as ENG-F1-08 did.

**Out of scope.**
- `GameClock` (ENG-F11-03); the clock still advances inside the strategy tick here.
- Physics setup and stepping stay in the binary's tick (feature decision; engine physics API is ENG-F15).
- Turning bus requests into commands (ADR-0011 rule 5), the `FrameStart`/`FrameEnd`
  bus events, `FRAME_COUNTER`, `GameState` (ENG-F12), egui/console in the engine
  (ENG-F18), splitting `App`'s fields (ENG-F1-03), camera interpolation (ENG-F21).
- No new wgpu use outside `moho_renderer` (ENG-F20).

**Test map** (gate class: **glue**).
| Criterion | Test |
|---|---|
| Sphere falls via loop | `moho::app::event_loop::frame_processor::tests::update_game_state_moves_falling_sphere`, rewritten to step `HeadlessLoop` |
| Input becomes the command | `moho::app::game::tests::held_forward_binding_is_in_the_tick_command_and_moves_player` |
| Existing behaviour | existing `frame_processor`, `event_processor`, `generation_processor`, `autosave` tests, ported |
| Resize/close routing | `moho_app::runner::tests::*` on the event-routing function, split out from the winit handler so it runs without a display |
| Layering | `just check` |

**Risks.**
- Large move across `src/main.rs` and `src/app/*`; GitNexus `impact` on `App` and
  `FrameProcessor` before starting. Mostly relocation, but tick/frame split order matters:
  keep `update_game_state` → streaming → light system → bus processing in the tick.
- Conflicts with ENG-F10 cards that touch `src/app`; run them one after the other.
- The seam names winit types, so a winit major upgrade changes seam 1. ADR-0008 keeps
  winit and upgrades are once per milestone, so this is accepted.
