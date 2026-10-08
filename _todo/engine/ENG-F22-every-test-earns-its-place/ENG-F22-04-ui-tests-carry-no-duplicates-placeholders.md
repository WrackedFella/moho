# UI tests carry no duplicates, placeholders or tests of uncalled code

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#171](https://github.com/WrackedFella/moho/issues/171)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

`moho_ui` loses constructor-only and setter/getter tests, empty placeholders, tests of derives and literals, duplicates of integration tests, and tests of uncalled code (deleted with it). Same-scenario tests become tables. Removal and merge only.

## Deliverables

- The removals and merges below are done, with the listed dead code deleted.
- The PR shows, per touched source file, that the branch misses no mutant that `dev` catches.

## Acceptance criteria

Removals:
- [ ] `overlays::overlay_manager` `update_data_replaces_snapshot`; `overlays::console::output` `test_new_output`, `test_add_line`; `overlays::console` `test_new_console`.
- [ ] `adapter::rendering` `test_render_pause_overlay` (no assertion; the paused-state test renders the same overlay).
- [ ] `screens::settings::state` `new_state_has_no_dirty_fields` (reads `config/prefs.ini`; `from_prefs_creates_clean_state` pins the rule).
- [ ] `screens::settings::keybind_capture` `test_start_stop_listening` with `stop_listening` (no caller); `test_binding_label_formatting` (every case is in `key_mapping`'s tests).
- [ ] `screens::settings::conflict_modal` `new_modal_is_hidden`.
- [ ] `screens::settings::binding_registry` `write_to_prefs_updates_all_bindings` with `write_to_prefs`; `update_binding_modifies_registry` with `update_binding`; `iter_returns_all_bindings_in_order` with `iter` and `BindingId::all` (all uncalled). `get_binding` stays until #179 (ENG-F22-12) rewrites its last user.
- [ ] `tests/settings_menu.rs` `tab_switching_works`; `tests/menu_system.rs` `menu_action_types`, `menu_item_creation`.
- [ ] `tests/console_rendering.rs` and `tests/staging_belt_smoke.rs` (an empty `#[ignore]` placeholder, and a no-assert call that `adapter_lifecycle_smoke` already makes); delete the files.

Merges:
- [ ] `fps_hud` and `rts_hud` `starts_visible` + `debug_hud` `starts_hidden` → `overlays` `hud_default_visibility`, a table: Debug hidden; Fps, Rts, Gameplay visible.
- [ ] `gameplay_hud` `render_hotbar_with_more_entries_than_slots_does_not_panic`, `…out_of_range_selected_slot…`, `…empty_inventory…` → `render_hotbar_edge_inputs_do_not_panic`, one table.
- [ ] `console::output` `test_clear_output` → `console` `test_clear`. `console` `test_execute_unknown_command` → deleted; `test_execute_help_command` covers "processor messages are appended".
- [ ] `settings::state` `mark_dirty_sets_dirty_flag` → `set_staged_binding_marks_only_that_field_dirty` (from `set_staged_binding_marks_dirty`), built with `from_prefs(Prefs::default())`, asserting KeyA is not dirty.
- [ ] `settings::state` `revert_changes_clears_dirty_fields` → deleted; `tests/settings_menu.rs` `revert_changes_works` covers it through the public API.
- [ ] `keybind_capture` `test_escape_cancels_listening` → `tests/settings_menu.rs` `escape_cancels_binding_listen`, which also asserts `get_staged_binding(0)` unchanged and `!has_unsaved_changes()`.
- [ ] `keybind_capture` `test_normal_key_capture` → deleted; `tests/settings_menu.rs` `modifier_keys_handled` covers capture on the listened field.
- [ ] `settings::render_ops` `test_render_bottom_panel_returns_items` → `tests/menu_system.rs` `settings_menu_structure`, which switches to `with_prefs`, and keeps `items.len() == 2`.

## Tech spec

**Design:** test-only edits plus deleting the named uncalled items. Drop `#[allow(dead_code)]` attributes that become stale.

**Proof of no loss:** `cargo mutants --file` on `dev` and the branch for each touched file under `moho_ui/src` outside `adapter/` (excluded in `.cargo/mutants.toml`; list as skipped). The branch adds no missed mutant, or the PR names each new one and why it is acceptable.

**Out of scope**
- Compass tests: the label-order decision is open.
- Music, rendering-by-state, video-setting and save-to-disk tests: they need seams; in #179 (ENG-F22-12) and #180 (ENG-F22-13).
- `apply_changes_clears_dirty_fields`: merged once the prefs save seam exists (settings test card).
- `key_mapping` and binding-code tests: #149 (ENG-F12-02) rewrites them.

**Test map:** the checklist; each line names its test.

**Gate class:** glue.

**Risks:** low. Overlaps draft ENG-F1-04 (keybind test tiers) and ENG-F1-07 (placeholder tests); those deliverables are met here.
