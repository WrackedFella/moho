# Frame loop

**Source:** `src/main.rs` (`ApplicationHandler for App`), `src/app/event_loop/`.
**Related:** [ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md) (fixed tick; the loop does not implement it yet).

The binary is a `winit` `ApplicationHandler`. Simulation work runs in `new_events`,
window input in `window_event`, drawing in the `RedrawRequested` window event.

## Callbacks

```mermaid
flowchart TD
    NE["new_events"] --> gate{"now ≥ last_frame<br/>+ frame_duration<br/>(60 Hz)?"}
    gate -- no --> cf["update_control_flow<br/>(wait until next frame)"]
    gate -- yes --> PF["FrameProcessor::process_frame"]
    PF --> DR["EventProcessor drains receivers"]
    DR --> GEN["check_generation_cancel<br/>poll_generation"]

    WE["window_event"] --> tab{"Tab while Playing?"}
    tab -- yes --> cam["toggle camera mode, stop"]
    tab -- no --> disp{"InputDispatcher<br/>consumed it?"}
    disp -- yes --> stop(["stop"])
    disp -- no --> weh["WindowEventHandler<br/>(resize, keys, close, redraw)"]
    weh --> RR["RedrawRequested"] --> render["scene.render(...)"]

    DE["device_event"] --> mm["raw MouseMotion → camera look"]
```

## `process_frame` order

`FrameProcessor::process_frame` advances `last_frame` by one `frame_duration`, then:

```mermaid
sequenceDiagram
    participant FP as FrameProcessor
    participant Bus as EventBus
    participant Sim as Simulation + Physics
    participant LS as LightSystem
    participant UI as UI adapter
    FP->>Bus: SystemEvent::FrameStart
    FP->>Sim: update_game_state (Playing only)
    Note over Sim: controller input → camera,<br/>KCC or free-fly, step rigid bodies
    FP->>LS: update_chunk_streaming
    Note over LS: evict / load chunks,<br/>publish WorldEvent::ChunkMeshDirty
    FP->>LS: update_light_system (budgeted)
    FP->>FP: update_lighting (sun, moon, ambient)
    FP->>UI: update_hud_data
    FP->>FP: request_redraw
    FP->>Bus: SystemEvent::FrameEnd
```

All gameplay steps early-return unless `GameState::Playing`.

## Event drain

After `process_frame`, `new_events` drains one channel per event family, in this order:

| # | Drain | Effect |
|---|---|---|
| 1 | `process_ui_events` | load/new world, exit (auto-save), menus |
| 2 | `process_audio_events` | forward to `AudioSystem` |
| 3 | `process_graphics_events` | time of day, quality settings |
| 4 | `process_world_events` | `ChunkMeshDirty` → remesh + collider |
| 5 | `process_input_events` | wheel zoom, mine action |
| 6 | `process_debug_events` | spawn, god mode, noclip |

Why channels instead of handling inside bus handlers: [events](events.md#the-channel-hand-off).

## Redraw

`handle_redraw_requested` calls `scene.render(...)` with actors, chunk meshes, the
camera, and material indices, then recalls the egui staging belt. A
`FrameError` skips the frame with a `warn`. Pass order inside the renderer:
[rendering](rendering.md#pass-order).

## Known gaps

- `dt` is the constant `frame_duration`, so each simulation step is tied to one rendered
  frame with no catch-up cap. [ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md)
  replaces this with an accumulator and fixed ticks (ENG-F6).
