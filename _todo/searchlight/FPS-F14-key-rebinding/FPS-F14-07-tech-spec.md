# #725 FPS-F14-07 Held actions toggle by a checkbox

**Gate class:** glue. **Depends on:** #722 (shares every file), #728 details (checkbox).
Built before #723 (see the README).

## Card changes

None.

## Tech spec

**Design**
- `input`: `SearchlightAction::toggleable` returns true for `LeanLeft` and `LeanRight`. The
  engine's action map already latches toggle-mode actions (ENG-F26-03), and the toggle mask
  is fixed when the map is built, which #719's rebuild on Apply covers.
- `main`: loads `[action_modes]` with `ActionBindings::load_modes`, logging each warning, as
  the strategy loader does. Today Searchlight never reads it.
- `controls`: the draft carries each toggleable action's `ActionMode`; `set_mode`;
  `reset_all` puts every mode back to `Hold`.
- `debug_settings::set_bindings` also writes `[action_modes]` (`modes_to_section`).
- `menu`: the checkbox on toggleable rows only.
- Engine seam (#728 details): the checkbox on an action's row.

**Out of scope**
- Toggle for crouch or prone (FPS-F4 slice 2 marks those when it adds them).
- Ending a latched lean on pause: `release_all` already does it, by engine rule.

**Footprint**
- `searchlight/src/input.rs` (`toggleable`) and its tests. FPS-F4 stance cards also edit
  `SearchlightAction`; whichever merges second rebases (one match arm).
- `searchlight/src/main.rs` (load modes)
- `searchlight/src/controls.rs`, `searchlight/src/menu.rs`, `searchlight/src/game.rs`,
  `searchlight/src/debug_settings.rs` and their tests

**Test map**
| Scenario | Test |
|---|---|
| A held action toggles | `game::tests::a_toggled_lean_holds_after_one_press_and_ends_after_the_next` |
| Hold is the default | `input::tests::lean_is_held_unless_set_to_toggle` |
| The choice survives a relaunch | `debug_settings::tests::applied_modes_load_back_as_main_loads_them` |
| Only held actions offer the checkbox | `menu::tests::only_toggleable_actions_show_the_checkbox` |
| edge | `input::tests::only_the_leans_are_toggleable` |
| edge | `controls::tests::reset_all_sets_every_mode_to_hold` |

**Risks**
- `resolve_lean` reads `pressed` before `held`: the press that releases a latched lean must
  not lean for a tick. The first scenario's test checks the tick after the second press.
