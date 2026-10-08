# HUD, console and UI routing tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#180](https://github.com/WrackedFella/moho/issues/180)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

The HUD, console and adapter-routing tests in `moho_ui` stop passing for wrong implementations, and the hotbar's spec, HUD mode gating, console quality commands and console-action routing get tests. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- A seam so menu music can be tested without `audio/music/menu.ogg` in cwd: `update_menu_music` takes the "menu music exists" answer from its caller.
- The `#[cfg(test)]` history accessors on the console output are deleted once the history tests no longer use them.

## Acceptance criteria

| Test (`moho_ui`) | Pins | Red against |
|---|---|---|
| `overlays::gameplay_hud` `render_hotbar_shows_tool_and_resource_labels` (SG-F1-02) | slot 0 shows the tool label; `hotbar[0] = (7, 3)` shows `R7` and `3`; slot 7 is empty with fewer entries (text from `FullOutput.shapes`, or a pure `hotbar_slot_labels` helper) | count shown from slot 0 |
| `overlays` `hud_draws_only_in_its_mode` | Fps and Gameplay HUDs draw only in FPS mode, RTS HUD only outside it | mode check removed |
| `overlays::overlay_manager` `toggle_overlay_by_name` | two overlays with different names; toggling one leaves the other | first overlay toggled regardless of name |
| `overlays::overlay_manager` `toggle_unknown_name_is_noop` | a registered overlay is unchanged after toggling "nonexistent" | toggle all on miss |
| `overlays::console::output` `test_add_to_history` | after `navigate_up`, adding "cmd3" resets navigation so `navigate_up() == Some("cmd3")` | reset dropped |
| `overlays::console::output` `history_navigation_clamps_up` and `history_navigation_past_end_clears` (split) | returned values only, no index accessor | clamp removed |
| `overlays::console::output` `test_history_max_size` | after 110 commands, `navigate_up() == Some("cmd109")` and it bottoms out at "cmd10" | newest evicted |
| `overlays::console::output` `navigate_on_empty_history_returns_none` | up and down on empty history → `None` | — (panic on empty) |
| `overlays::console` `execute_command_echoes_records_and_appends_messages` (from `test_execute_help_command`) | `"  help  "` echoes `> help`, appends the help text, and history recalls "help" | no trim |
| `overlays::console` `blank_input_is_ignored` | `"   "` → `None`, output and history unchanged | blank echoed |
| `overlays::console` `backtick_closes_console_except_first_frame` (replaces `test_reset_on_open`) | after `reset_on_open`, a first frame with Backtick pressed stays open; the next frame with Backtick closes | first-frame guard removed |
| `overlays::console::commands` `test_time_command_clamping`, `test_spawn_command_valid`, `quality_commands` | `time -5` → 0.0; `spawn tree oak wood` → "oak wood"; `r_shadow_quality`/`r_ssao_quality`: 4 accepted, 5 rejected with "between 0 and 4", `x` invalid, no argument → usage | lower clamp removed; only `parts[2]`; `> 4` check removed |
| `adapter::event_routing` `test_emit_audio_events` | each UI audio event publishes its mapped `AudioEvent` | Confirm and Cancel swapped |
| `adapter::event_routing` `menu_music_starts_on_show_start`, `menu_music_stops_when_leaving_start` (table: load scene, generate world, exit, settings), `menu_music_no_double_stop` | `MusicStart { looped: true }` when music exists, nothing when it doesn't; exactly one `MusicStop` on leaving; none when not playing | flag cleared without `MusicStop` |
| `adapter::event_routing` `console_action_publishes_matching_event` | each console action's event; sun 90°, 45° → yaw π/2, pitch π/4 (`ToggleCollision` row left out) | degrees passed through |
| `adapter::event_routing` menu table | `SettingsSaved` → `SettingsSaved` + `WindowSettingsChanged { mode, width, height }`; `Close` → no UI event | window event missing |
| `adapter::rendering` `render_game_state_by_state` (merges the playing and paused tests) | the Paused frame contains the "Paused" text and the Playing frame doesn't; in Menu, a clicked disabled item yields no action | Paused routed to the Playing arm |

## Tech spec

**Design:** tests drive egui with `RawInput` events where input matters and read `FullOutput.shapes` for text.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- Compass tests, `r_debug_view` out-of-range ids, the New World name, re-showing the start menu while music plays: #186 (ENG-F22-19).
- `ToggleCollision { enabled: false }`: open decision.

**Test map:** the table above.

**Gate class:** glue (UI).

**Risks:** text-shape assertions couple to egui's output format; assert on text content, not layout.
