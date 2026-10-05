# The binary's frame and event wiring is tested without a window

**Status:** done
**Feature:** [ENG-F1](_feature.md)

## Summary

The binary's event-loop processors, scene loader, autosave and world
generator have no tests, so `just mutants` reports 18 survivors on any change
that touches them (first seen on [ENG-F7-02](../ENG-F7-maintained-ecs/ENG-F7-02-legion-removed.md)). `App` already holds its window,
renderer, UI and audio as `Option`s. Only its constructor ties it to on-disk
prefs. Tests can therefore build a headless `App` and drive the processors
directly.

## Deliverables

- Unit tests build an `App` with no window, GPU, audio or prefs file.
- Every surviving mutant from the 2026-10 ENG-F7-02 run that doesn't need a
  window is killed.
- The three that do need one are excluded in `.cargo/mutants.toml`, each with
  a stated reason.
- Autosave and world generation write to a directory the caller passes in,
  not a hard-coded `saves/` under the working directory.

## Acceptance criteria

- [ ] `just mutants` on this card's diff reports no missed mutants in
      `src/app/**`.
- [ ] No test needs a display, GPU, audio device or prefs file, so `just check`
      passes on headless CI on all three OSes.
- [ ] `cargo run` behaves as before: save on quit, load, generate, console
      `spawn sphere|cube|torch|light` (see Verification).

## Tech spec

**Design.**
- `App::new()` becomes `App::from_config(AppConfig)`. `main` passes
  `AppConfig::from_prefs()`. A `#[cfg(test)] App::headless()` passes
  `AppConfig::builder().init_audio(false).build()`. No new types; the
  `Option` fields stay `None`.
- `autosave::auto_save_on_shutdown` and `world_generator::generate_new_world`
  take a `saves_dir: &Path`. `App`'s wrappers pass `saves`.
- Chunk files (`saves/<world>/chunks`) stay cwd-relative. Tests use unique
  world names and unmodified chunks, so they never read or write one.
- Tests call processor methods directly, never through `ApplicationHandler`.
  None of the targets take `&ActiveEventLoop`.
- `tempfile` becomes a binary dev-dependency, promoted to
  `[workspace.dependencies]` alongside `moho_renderer`'s.
- `.cargo/mutants.toml` gains `exclude_re` entries for
  `WindowEventHandler::handle_redraw_requested`,
  `WindowManager::initial_render`, and the `"light"` arm of
  `EventProcessor::handle_debug_event`. All three only call the renderer
  backend, which needs a `Window`.

**Out of scope.**
- Extracting the window-free state into its own struct (see Notes).
- Tests for functions that take `&ActiveEventLoop`.
- The `"light"` arm parsing bad colour args as `1.0` (an
  [ENG-F1-02](ENG-F1-02-error-handling-backlog.md) item).

**Test map** (gate class: **glue**, so tests and code land together).
| Surviving mutant | Test |
|---|---|
| `step_physics_bodies` → `()` | `frame_processor::tests::update_game_state_moves_falling_sphere` |
| `update_game_state_fps_kcc` → `()` | `frame_processor::tests::fps_with_active_kcc_moves_player_under_gravity` |
| `update_chunk_streaming` → `()` | `frame_processor::tests::chunk_streaming_while_playing_loads_chunks_and_publishes_mesh_dirty` |
| `update_lod_transitions` → `()`, `!=` → `==` | `frame_processor::tests::lod_transition_marks_only_stale_chunks_dirty` |
| `process_world_events` → `()` | `event_processor::tests::chunk_mesh_dirty_event_stores_meshed_chunk_with_collider` |
| `handle_debug_event` → `()`; `SpawnEntity`, `"cube"`, `"sphere"` arms deleted | `event_processor::tests::spawn_sphere_adds_actor_with_physics_body`, `spawn_cube_adds_actor_with_physics_body` |
| `load_scene` → `Ok(())`; `auto_save_on_shutdown` → `Ok(())` | `autosave::tests::autosave_then_load_restores_actors_camera_time_and_blocks` |
| `generate_new_world` → `Ok(())` | `world_generator::tests::generate_new_world_starts_job_that_writes_the_scene_and_completes` |
| `handle_completed` → `()`; `setup_physics_for_world` → `()` | `generation_processor::tests::completed_generation_enters_playing_with_terrain_colliders` |
| `handle_redraw_requested`, `initial_render`, `"light"` arm | excluded (window-only) |

**Risks.**
- `App::new` has one caller (`main`); autosave and generation each have one
  `App` wrapper. GitNexus `impact`: LOW.
- The world-generation tests run real terrain generation. Use the smallest
  `WorldSpec`. If a test takes over ~2 s, gate it with `#[ignore]`.

## Notes

Extracting the window-free fields (entities, physics, streamer, simulation,
light system) into their own struct would be a better SRP split, but it
doesn't improve testability: the GPU fields are already optional. If `App`'s
size becomes the problem, do the split under
[ENG-F1-03](ENG-F1-03-god-module-splits.md).
