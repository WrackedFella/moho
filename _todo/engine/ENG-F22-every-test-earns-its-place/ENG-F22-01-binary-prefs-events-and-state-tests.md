# Binary, prefs, events and state tests carry no duplicates or can't-fail tests

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#168](https://github.com/WrackedFella/moho/issues/168)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

Tests in the binary, `moho_core`'s prefs and events, and `moho_types` that cannot fail, duplicate a twin, or differ from a neighbour only by data are deleted or folded into tables. Removal and merge only; new assertions are limited to what a merge carries over.

## Deliverables

- The removals and merges below are done; nothing else in these files changes.
- The PR shows, per touched source file, that the branch misses no mutant that `dev` catches (see Tech spec).

## Acceptance criteria

- [ ] Removed: `tests/console_toggle.rs` (empty `#[ignore]` placeholder; delete the file).
- [ ] Removed: binary `app::audio_init` `test_audio_initialization_does_not_panic` (no assertion, host-audio dependent, reads cwd `assets/`).
- [ ] Removed: binary `app::event_setup` `test_event_bus_setup_creates_bus` (`strong_count >= 1` is always true).
- [ ] Removed: binary `app::initializer` `test_multiple_builds_with_same_config` (`build()` only returns `Ok`).
- [ ] Removed: binary `app::event_loop::window_manager` `test_window_error_display` (log-only text).
- [ ] Removed: `moho_core::prefs` `test_default_video_settings` (the defaults golden pins it).
- [ ] Removed: `moho_core::prefs` `test_default_prefs`, after `prefs::reader` `to_ini_string_of_defaults_is_the_save_format` also asserts that parsing the expected text gives `(Prefs::default(), no issues)`. That parse-back is what pins the exact default key codes.
- [ ] Removed: `moho_core::prefs::reader` `prefs_section_wins_over_keys_before_any_header` (`keys_before_header_are_reported_when_prefs_section_exists` catches everything it does).
- [ ] Removed: `moho_core::events` `test_recent_history`, together with `EventBus::recent_history` (no caller in the workspace).
- [ ] Merged: binary `event_processor` `test_lod_player_chunk_origin`, `_positive`, `_negative` → one table `lod_player_chunk_maps_world_pos_to_chunk` with rows 0→0, 16→1, -1→-1, plus y rows `(0,-1,0)→(0,-1,0)` and `(0,32,0)→(0,2,0)` (no current test pins y).
- [ ] Merged: binary `event_processor` `test_lod_for_chunk_same_position`, `_within_lod0`, `_boundary`, `_far` → `lod_for_chunk_tiers_by_chebyshev_xz_distance`: distance 0, 3, -3, (3,3) → LOD 0; 4, (0,4), (3,4), 16 → LOD 1. `_ignores_y` stays or becomes a row.
- [ ] Merged: `moho_core::prefs::reader` `base_file_uses_non_default_values_for_every_key` becomes a helper `assert_base_differs_from_default()` called from both `base_file_loads_to_base_prefs_without_issues` and `malformed_value_warns_and_only_that_key_falls_back`; the standalone test is deleted.
- [ ] Merged: `moho_types::state_coordinator` `test_exit_console` → `test_hide_menu`, renamed `transitions_to_playing_grab_and_hide_cursor`, a table over `(hide_menu, Menu)` and `(exit_console, ConsoleOpen)` asserting the whole `StateTransitionActions` with `assert_eq!`.

## Tech spec

**Design:** test-only edits plus deleting `EventBus::recent_history`. Table tests use a literal case array and one loop whose body is the assertion, with the row in the failure message.

**Proof of no loss:** for each source file whose tests change (`src/app/initializer.rs`, `src/app/event_loop/event_processor.rs`, `moho_core/src/prefs/mod.rs`, `moho_core/src/prefs/reader.rs`, `moho_core/src/events/bus.rs`, `moho_types/src/state_coordinator.rs`), run `cargo mutants --file <path>` on `dev` and on the branch. The branch adds no missed mutant, or the PR names each new one and why it is acceptable (equivalent, or its code was deleted). Files excluded in `.cargo/mutants.toml` are skipped and listed.

**Out of scope**
- `prefs::parser`, `prefs::key_names`, `Binding::default` and their tests: #149 (ENG-F12-02) deletes them.
- `tests/event_bus_integration.rs`: its removals depend on moving two tests into `moho_core` first; that is in #175 (ENG-F22-08).
- Camera and initializer tests that depend on strengthened replacements: in #172 (ENG-F22-05).
- `test_resume_game`, `test_pause_game` and the pause path: deferred (Paused is unreachable in production).
- `GameState` helpers with no caller (`is_simulating`, `should_render_*`): not touched.

**Test map:** the checklist is the test map; each line names its test.

**Gate class:** glue (no behaviour change; tests are only removed or folded).

**Risks:** low. A merged table that loses a row loses coverage; the mutation comparison catches that.
