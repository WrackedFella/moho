# Settings tests stay off the real prefs file and pin every binding

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#179](https://github.com/WrackedFella/moho/issues/179)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

The settings screen's tests stop touching the real `moho_ui/config/prefs.ini` and stop passing for wrong implementations: every binding field, the video settings path the UI uses, and saving are pinned. Each test is shown red against the wrong implementation it targets.

## Deliverables

- No `moho_ui` test reads or writes `config/prefs.ini`: tests build state with `with_prefs`/`from_prefs`, and saving goes to a path the test owns.
- A save-path seam: `Prefs::save_to` becomes public and the settings state can be given a save path (e.g. `SettingsMenu::with_prefs_at(prefs, path)`); production keeps the default path. The `with_prefs` rustdoc and the `tests/settings_menu.rs` header stop claiming nothing is written.
- Seams `SettingsMenu::set_window_mode` and `set_window_resolution` (stage and mark dirty), used by the video tab.
- The tests below exist and each was shown red against its named wrong implementation; `BindingRegistry::get_binding` and `calculate_content_width` (no other caller) are deleted.

## Acceptance criteria

| Test (`moho_ui`) | Pins | Red against |
|---|---|---|
| `tests/settings_menu.rs` `staged_changes_tracked_and_cleared_on_save` (merges `settings::state` `apply_changes_clears_dirty_fields`) | binding a non-default key marks dirty; saving to a temp path returns `Ok`, clears dirty, and the file reloads with that key | save leaves dirty set |
| `tests/settings_menu.rs` `revert_changes_works` | no `if` in the body; 'Q' is unbound by default, so no conflict; no save step | revert keeps staged value |
| `settings::state` `get_staged_binding_returns_correct_value` | all seven binding fields set to distinct bindings round-trip without touching the others | KeyUp wired to `set_key_down` |
| `settings::state` `non_binding_fields_handled_gracefully` | staging a binding on a non-binding field leaves every binding unchanged | falls through to KeyW |
| `settings::binding_registry` `from_prefs_loads_all_bindings` | seven distinct non-default bindings including sprint; for each, `find_conflict(&b, other) == Some(id)` | sprint read from the wrong getter |
| `settings::binding_registry` `find_conflict_distinguishes_modifiers` | KeyW = W; Ctrl+W for KeyA → no conflict | mods ignored |
| `tests/settings_menu.rs` `multiple_unique_bindings_work` | ids 0-6 including sprint, arrows as 0x100/0x101 (not 38/40) | sprint not stored |
| `tests/settings_menu.rs` `render_doesnt_crash` | renders all three tabs, including Video | — (no-panic; the Video tab is the only one with window widgets) |
| `settings` `video_setting_change_marks_dirty_and_reverts` (merges `window_mode_dirty_tracking` and `window_resolution_dirty_tracking`) | through the new setters: dirty after a change, original after revert | setter not marking dirty |
| `settings` `modifier_only_conflict_shows_modal` | built with `with_prefs`, key_down moved off Ctrl | as today |
| `tests/menu_system.rs` `settings_menu_structure` | with nothing clicked every item's action is `MenuAction::None` | Save always emits `SettingsSaved` |
| `tests/adapter_lifecycle_smoke.rs` `adapter_lifecycle_smoke` | `hide_menus()` then `show_menu("settings")` → `EguiAdapter::active_screen_name() == Some("settings")` (new pass-through) | `show_menu` a no-op |
| `tests/start_menu.rs` `start_menu_returns_expected_action_and_rects` | the four items' actions in order: load scene, new world, settings, exit; none clicked; don't assert `enabled` (depends on cwd `saves/`) | items reordered |
| `screens::form_controls` `gutter_is_percent_with_min_20` | 1000 → 300; 50 → 20 | min clamp removed |

## Tech spec

**Design:** the seam keeps `Prefs::save()` and `SettingsMenu::new()` unchanged for production; tests never call them.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- Conflict confirm and sprint conflict: #181 (ENG-F22-14).
- Whether a non-binding field becomes dirty when a binding is staged on it: open decision; don't assert it.
- `input_handling::hit_test_menu_items` (no production caller): open decision; don't strengthen its test.
- Binding labels and key codes: #149 (ENG-F12-02).

**Test map:** the table above.

**Gate class:** glue (UI).

**Risks:** the save seam touches `moho_core::prefs`'s public API; keep `save_to` behaviour identical.
