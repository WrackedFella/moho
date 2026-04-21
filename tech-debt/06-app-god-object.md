# TD-06: Dissolve the App God Object

**Priority:** Medium — not blocking today but snowballs badly as gameplay systems
are added.

## Problem

`src/main.rs` defines an `App` struct with ~30 fields covering every system in the
engine:

```
world, scene, camera, voxel_grid, light_system, game_state, input_router,
window_renderer, event_bus, 5× event receivers, audio_system, ui_adapter,
world-gen plumbing (receiver/handle/cancel), debug_mode, dispatcher,
input channels, simulation, mouse_sensitivity, input_system, prefs,
active_keys, frame timing, light_gizmos, physics_world, chunk_colliders,
test_physics_bodies, jump_pressed
```

`FrameProcessor`, `EventProcessor`, and `GenerationProcessor` were extracted to
separate files but all take `&mut App` — they just moved code, they didn't break up
ownership. Adding a new gameplay system means adding more fields to `App`, more arms
to `process_ui_events`, and more arms to `finalize_frame`. This does not scale.

The event bus already exists precisely to avoid this coupling. Systems should own
their own state and communicate via events, not share one giant god struct.

## Acceptance Criteria

Each extracted system:
- Owns its own state (no fields remaining on `App` for that system).
- Communicates state changes via the event bus or via typed channels (for systems
  that can't use the bus due to `!Sync` constraints like `AudioSystem`).
- Has a clear `tick(dt)` / `process_events()` interface.

Target end state: `App` holds only what is truly cross-cutting (event bus, window,
frame timing, game state) and delegates everything else.

## Suggested Extraction Order

1. **Physics** — `PhysicsWorld`, `chunk_colliders`, `test_physics_bodies`,
   `jump_pressed`. Wrap in a `PhysicsController` struct with a
   `tick(dt, input) -> Option<Vec3>` interface. Physics results (new position) flow
   back to simulation via a channel or direct call.

2. **World generation** — `generation_receiver`, `generation_handle`,
   `generation_cancel`, `last_world_spec`. These form a complete async-job pattern;
   wrap in `WorldGenerator` with `poll() -> Option<GenerationResult>`.

3. **Input state** — `active_keys`, `input_system`, `prefs`, `unconsumed_input_tx/rx`.
   The resolution of TD-04 (input consolidation) should happen first; this
   follows naturally from it.

4. **Rendering** — `window_renderer` already wraps the renderer; push `scene`,
   `camera`, and `light_gizmos` into a `RenderState` or `SceneController` companion.

5. **Audio** — already conceptually isolated; formalize by extracting the
   audio event-handler loop into an `AudioController` method called from the main
   loop rather than via `App::handle_audio_event`.

## Files

- `src/main.rs` — primary target
- `src/app/event_loop/event_processor.rs` — will shrink as systems own their events
- `src/app/event_loop/frame_processor.rs` — physics tick, input application live here
- `src/app/event_loop/generation_processor.rs` — world gen polling
