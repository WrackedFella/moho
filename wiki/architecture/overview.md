# Architecture overview

**Source:** crate `Cargo.toml`s, `moho_core/src/rng.rs`, `scripts/layering.txt`, `moho_app/src/`, `searchlight_rules/src/`, `searchlight/src/`, `flagstone/src/app/`, `flagstone/src/main.rs`.
**Decisions:** ADRs
[0001](../../_todo/adr/0001-render-api-boundary.md),
[0003](../../_todo/adr/0003-core-owns-event-types.md),
[0004](../../_todo/adr/0004-entity-storage-without-a-general-ecs.md),
[0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md),
[0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md),
[0012](../../_todo/adr/0012-engine-crate-map-for-m2.md),
[0017](../../_todo/adr/0017-ui-shell-and-debug-values-are-engine-seams.md).

## 1 Crate graph

Arrows run from dependent to dependency; colour is the line from `scripts/layering.txt`.
The `flagstone` binary depends on all of them and is omitted from the edges.

```mermaid
flowchart TD
    ui["flagstone_ui<br/>egui menus, console, HUD"]:::strategy
    game["flagstone_game<br/>pawn, tools, controller,<br/>scenes"]:::strategy
    voxel["flagstone_voxel<br/>voxel grid, meshing, lighting"]:::strategy

    renderer["moho_renderer<br/>wgpu backend"]:::engine
    rapi["moho_render_api<br/>Renderable, RenderMaterial,<br/>GPU-layout data"]:::engine
    core["moho_core<br/>event bus, persist,<br/>prefs, seeded RNG"]:::engine
    audio["moho_audio<br/>rodio"]:::engine
    physics["moho_physics<br/>rapier3d"]:::engine
    input["moho_input<br/>Key, Action, ActionBindings,<br/>ActionMap, mouse smoothing"]:::engine
    app["moho_app<br/>window, fixed-step loop, Game trait"]:::engine
    level["moho_level<br/>glTF level loader"]:::engine
    shell["moho_ui_shell<br/>egui host, console,<br/>UI layers"]:::engine
    fps["searchlight_rules<br/>rifle, magazine, body damage"]:::fps
    fpsgame["searchlight<br/>arena, shots, sim, binary"]:::fps

    ui --> renderer & input & core & game & shell
    game --> core & rapi & voxel
    voxel --> core & rapi
    renderer --> rapi
    app --> renderer & audio
    audio --> core
    core --> rapi
    physics --> rapi
    level --> rapi
    shell --> rapi & input & app
    fpsgame --> fps & app & physics & level & input & audio & renderer & rapi & shell

    classDef engine fill:#dbeafe,stroke:#2563eb,color:#111
    classDef strategy fill:#dcfce7,stroke:#16a34a,color:#111
    classDef fps fill:#ffedd5,stroke:#ea580c,color:#111
```

Blue is the engine line, green is the flagstone line, orange is the searchlight line.
`searchlight_rules` holds plain serialisable weapon and body rules (`Rifle`, `Magazine`, `Body`)
with no engine dependency. `searchlight` is the only searchlight-line crate that reaches the engine: a library
plus the `searchlight` binary, described in [Searchlight](../searchlight/architecture.md). Every shot resolves
through one path (ray, surface tag, part, rule call) for player and enemy alike
([ADR-0015](../../_todo/adr/0015-fps-rules-are-a-platform-free-crate.md)).
`moho_ui_shell` is the engine's only egui-facing crate: the console every game opens and adds commands to, the egui host that draws it, and the screen text a game sets each frame (`UiShell::set_texts` with `ScreenText` and `Anchor`, `moho_ui_shell/src/text.rs`; each call replaces the list and refuses a size that is not finite and positive, painted under the console and cleared by the frame that paints it) ([ADR-0017](../../_todo/adr/0017-ui-shell-and-debug-values-are-engine-seams.md)).
`moho_app` owns the window and winit event loop; the binary implements its `Game` trait and hands it
to `moho_app::run` ([frame loop](frame-loop.md)).

## 2 Lines and rules

Each crate belongs to exactly one line ([ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md)).
`just check` enforces these over normal and build edges:

| Rule | Reason |
|---|---|
| Engine crates never depend on a game-line crate (dev-dependencies allowed) | The engine ships to both games and, after the split, from its own repo |
| A game line never depends on another game line | Flagstone and Searchlight must separate cleanly |
| Domain crates (`moho_core`, `flagstone_voxel`, `flagstone_game`, `moho_level`, `searchlight_rules`) never reach `winit`, `wgpu` or `egui` | Domain logic runs headless |
| Only `moho_renderer` (the `gpu` row of `scripts/layering.txt`) reaches `wgpu`, `egui-wgpu` or `naga` without going through another workspace member, dev-dependencies included | GPU code stays behind the renderer boundary |

Voxels live in the flagstone-line `flagstone_voxel` ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md));
the engine sees world geometry only as meshes.

## 3 Target crate graph (ENG-M2)

Where the ENG-M1–ENG-M2 engine work lands, from [ADR-0012](../../_todo/adr/0012-engine-crate-map-for-m2.md)
(Proposed). Capabilities land as modules of an existing crate first; a new crate needs a
deployable, reuse or compile-time boundary. Dashed boxes are crates that don't exist yet, or (`moho_app`) exist with only part of the listed scope: today the window and event loop, accumulator, `GameClock` and `Game` trait.
Edges are the intended direction, not a list each crate must have.

```mermaid
flowchart TD
    sbin["flagstone binary<br/>strategy wiring"]:::strategy
    ui["flagstone_ui<br/>strategy HUD, menus"]:::strategy
    game["flagstone_game<br/>pawn, tools, scenes"]:::strategy
    voxel["flagstone_voxel<br/>voxel terrain"]:::strategy

    app["moho_app<br/>window, loop, accumulator,<br/>GameClock, Game interface"]:::engineNew
    shell["moho_ui_shell<br/>egui, console, settings,<br/>modal stack"]:::engineNew
    input["moho_input<br/>action map, gamepad,<br/>bindings, mouse smoothing"]:::engine
    renderer["moho_renderer<br/>wgpu backend"]:::engine
    physics["moho_physics<br/>rapier3d"]:::engine
    audio["moho_audio<br/>rodio"]:::engine
    core["moho_core<br/>event bus, engine events, prefs,<br/>content roots, save envelope,<br/>seeded RNG"]:::engine
    rapi["moho_render_api<br/>engine/game contract"]:::engine
    level["moho_level<br/>glTF level loader"]:::engine

    sbin --> app & ui & game & voxel
    ui --> shell & game
    game --> core & rapi
    voxel --> core & rapi
    app --> renderer & physics & audio & input & core
    shell --> renderer & input & core
    input --> core
    renderer --> rapi
    physics --> rapi
    audio --> core
    core --> rapi
    level --> rapi

    classDef engine fill:#dbeafe,stroke:#2563eb,color:#111
    classDef engineNew fill:#dbeafe,stroke:#2563eb,stroke-dasharray:5 5,color:#111
    classDef strategy fill:#dcfce7,stroke:#16a34a,color:#111
    classDef strategyNew fill:#dcfce7,stroke:#16a34a,stroke-dasharray:5 5,color:#111
```

Changes from today:

- Scene import (ENG-F14) is its own crate, `moho_level`: a headless server needs level
  collision without linking `wgpu` or `winit`.
- Navigation (`moho_level::NavMesh`: walkable polygons built from world meshes for an
  `AgentProfile`, and corner paths between points) is a module of `moho_level`
  ([ADR-0019](../../_todo/adr/0019-navigation-is-an-engine-seam.md)). The character
  controller and camera (ENG-F21) are modules of an engine crate unless their specs name a
  boundary.

The searchlight line sits beside the flagstone line and depends only on engine crates.

### 3.1 Engine seams

The public APIs the [ENG-F5](https://github.com/WrackedFella/moho/issues/232)
gate holds fixed: game plug-in interface, world-geometry contract, action map, physics
queries, spatial audio, content roots, save envelope, scene import, navigation
([ADR-0019](../../_todo/adr/0019-navigation-is-an-engine-seam.md)). Adding or changing
one is an ADR-level change.

## 4 Fixed-step loop (`moho_app`)

Window-free scheduling for any game line ([ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md)).

| Item | Role |
|---|---|
| `Game` trait (`lib.rs`) | `command()` sampled once per tick, `tick()` applies it, `frame()` presents with an interpolation `alpha`; optional `init()` (window and renderer exist), `event()` (winit window/device events) and `apply_while_paused()` (a command applied at the tick boundary while paused) |
| `run`, `AppConfig` (`runner.rs`) | Creates the window, renderer and optional `AudioSystem`, drives the `Game` from the winit loop; `AppError` on startup failure |
| `InitContext` / `EventContext` / `FrameContext` (`lib.rs`) | What a game may touch: window, renderer, audio, `request_exit`, `set_paused` / `set_speed` (`EventContext`, `FrameContext`; recorded, applied by the loop from the next tick) and `frame_dt` (`FrameContext`: time since the previous frame, capped at `max_catch_up_ticks` ticks) and a read-only `clock()` (`EventContext`, `FrameContext`). Renderer and audio are `Option` (absent headless) |
| `GameClock` (`clock.rs`) | Time of day, day/night lengths, time scale and sun/moon directions. Seeded from `LoopConfig::clock`, owned by the loop (`run` and `HeadlessLoop`); `TickContext::clock` is `&mut` for the tick, and the loop advances it by one tick length times its time scale after `Game::tick` returns |
| `LoopConfig` / `FixedStep` (`fixed_step.rs`) | Integer accumulator (`nanoseconds * tick_hz`), so no drift; returns whole ticks due per frame. `SimSpeed` (`X1`/`X2`/`X3`) multiplies ticks per second of real time, never the tick length |
| `HeadlessLoop` (`headless.rs`) | Drives a `Game` without a window: `advance(game, frame_dt)` runs due ticks then one frame; `step(game, n)` runs ticks only (also while paused); `set_paused`, `set_speed`, `paused()`, `next_tick()` |

Invariants: ticks per frame are capped by `max_catch_up_ticks` (default 5) and time beyond the cap is
dropped except the sub-tick remainder (the cap scales with `SimSpeed`); `tick_hz` must be non-zero (`FixedStep::new` panics); `TickContext::tick` counts from 0.
Pause and speed are loop state, not simulation state ([ADR-0018](../../_todo/adr/0018-paused-commands-apply-at-the-tick-boundary.md)): while paused no tick runs and the sub-tick remainder is untouched, the game's `command()` goes to `Game::apply_while_paused(next_tick, command)` before the frame, and tick index and simulation time do not move.

## 5 Runtime composition

`App::from_config` (`flagstone/src/app/initializer.rs`) builds each subsystem and passes dependencies
explicitly. There is no service locator. The window, renderer and `AudioSystem` are created by
`moho_app::run` and passed to the game through its contexts.

```mermaid
flowchart LR
    init["App::from_config<br/>initializer.rs"] --> bus["EventBus (Arc)<br/>event_setup.rs"]
    init --> rend["Renderer + egui adapter<br/>renderer_setup.rs"]
    init --> phys["PhysicsController<br/>physics_controller.rs"]
    init --> sim["Simulation<br/>flagstone_game"]
    init --> disp["InputDispatcher<br/>handlers registered in<br/>renderer_setup.rs"]
    bus -->|"subscribers forward into<br/>mpsc channels"| chans["ui · audio · graphics ·<br/>world · debug receivers"]
    chans --> loop["Game::tick / Game::frame<br/>drain receivers"]
```

## 6 Where state lives

| State | Owner |
|---|---|
| Voxel grid, light | `flagstone_voxel::LightSystem` (owns the grid) |
| Chunk meshes | `ChunkStore` in `flagstone_voxel`; the renderer holds its own copy as `WorldMeshes` ([rendering](rendering.md#2-world-geometry)) |
| Actors (spheres, cubes) | `ActorStore` in `flagstone_game` |
| Time of day | `moho_app::GameClock`, owned by the loop and advanced after each `Game::tick` |
| App mode | `flagstone_ui::GameState` ([input-and-state](input-and-state.md#3-gamestate)) |
| GPU resources | `moho_renderer`; game types reach it only through `moho_render_api` ([rendering](rendering.md)) |
| Prefs | `moho_core::prefs::Prefs` ([format](../reference/prefs-format.md)) |
| Simulation randomness | `moho_core::rng::SimRng`, one per system, held by value in that system's state and keyed by `StreamId::named(<system name>)`. Integer-only SplitMix64, so a seed and stream replay identically on every platform; the sequence is pinned by tests, and changing it breaks saved games and replays. Not `Copy`: duplicate only with an explicit `clone` |
| Player character, noclip | `PhysicsController` (`flagstone/src/app/physics_controller.rs`): `player` is a `CharacterHandle` into `moho_physics::PhysicsWorld`, which holds each character's `CharacterState` (position, velocity, yaw, pitch, grounded; `moho_physics/src/world/character.rs`). Per-tick movement rules are the pure `moho_physics::step` (`moho_physics/src/movement.rs`), driven by a `CharacterCommand` and game-supplied `MoveSettings`; an airborne tick applies gravity in two halves around the slide, so displacement matches constant gravity at any tick rate and a landing's impact speed is the mid-tick fall speed; the game owns the tuning (`STRATEGY_MOVE_SETTINGS` in `flagstone/src/app/event_loop/frame_processor.rs`). A grounded walker follows walkable slopes (vertical speed tracks the ground, horizontal speed is kept) and is snapped down onto ground within `GROUND_SNAP`, so it stays on ground that levels out (a ramp onto a plateau) but leaves with its momentum where the ground falls away; the slide reports the surface under the capsule as `Slide::ground` (`moho_physics/src/movement.rs`). It also keeps the player's pre-tick state for the [frame camera](frame-loop.md#41-frame-camera) (`moho_physics::first_person_view`, `moho_physics/src/camera.rs`), reset on spawn and teleport. `reset` clears the handle and that state |

Entities live in typed stores; there is no general ECS (ADR-0004).

## 7 Logging

All crates emit `tracing` events (`tracing::info!(count = n, "message")`, structured
fields over interpolated text); the `log` facade is not a workspace dependency.
`init_logging` in `flagstone/src/main.rs` installs the only subscriber, filtered by `RUST_LOG`
(default `info`), and also receives `log` records from third-party dependencies.
Library crates never install a subscriber.

## 8 Next

[Frame loop](frame-loop.md) · [Searchlight](../searchlight/architecture.md) · [Events](events.md) · [Rendering](rendering.md)
