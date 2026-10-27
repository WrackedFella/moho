# #723 FPS-F14-05 An action's details set hold and modifier per slot

**Gate class:** glue. **Depends on:** #725 (shares every file), #728 details (expander,
pickers, has-more mark).

## Card changes

None. Per-action reset stays in (it is `reset_all` for one row); it is still the first thing
cut.

## Tech spec

**Design**
- `controls`: a slot holds its input (or none) plus a `Trigger` and `Modifiers`, so a picker
  set on an empty slot applies to the next capture. `set_kind(action, slot, Trigger)` and
  `set_modifier(action, slot, Modifiers)` change the slot; on a bound slot they change its
  binding and run the same conflict check as capture (a pending conflict, #720's dialog).
  `capture` builds the binding from the input and the slot's kind and modifier.
  `reset(action)` restores one action's defaults and mode.
- `controls`: `has_more(action)`: some slot is not plain (`Binding::is_plain`).
- A modifier key captured into a slot whose modifier is itself (`Ctrl` with `Ctrl`) is
  refused as `Reserved`: the prefs loader rejects that chord and would drop it on load.
- The modifier picker offers one modifier. A loaded binding with two (`Ctrl+Shift+R`, only by
  hand-editing prefs) shows as is and is replaced if the player changes the picker.
- Engine seam (#728 details): the expander, the two pickers and the per-action reset, and the
  collapsed view's plain-press filter and has-more mark.

**Out of scope**
- Double press (#724); multi-modifier chords in the picker; per-action hold thresholds.

**Footprint**
- `searchlight/src/controls.rs`, `searchlight/src/menu.rs`, `searchlight/src/game.rs` and
  their tests

**Test map**
| Scenario | Test |
|---|---|
| The basic view shows plain presses | `controls::tests::has_more_is_true_only_with_a_hold_or_chord_slot` and `menu::tests::collapsed_rows_show_press_bindings_and_mark_more` |
| Expanding shows one action's details | `game::tests::expanding_one_action_shows_its_pickers_and_reset_only` |
| The pickers set what the input means: Hold Y | `game::tests::an_applied_hold_slot_fires_after_the_hold_threshold` |
| The pickers set what the input means: Ctrl+Y | `game::tests::an_applied_chord_slot_fires_with_its_modifier` |
| Capture records the key, not the gesture | `controls::tests::capture_takes_kind_and_modifier_from_the_slot` |
| A press and a hold of one key do not conflict | `controls::tests::a_hold_and_a_press_of_one_key_do_not_conflict` |
| Reset one action | `controls::tests::reset_restores_one_action_and_keeps_the_others` |
| edge | `controls::tests::a_picker_change_into_a_held_binding_asks_first` |
| edge | `controls::tests::a_modifier_key_in_its_own_chord_is_refused` |
| edge | `debug_settings::tests::hold_and_chord_slots_load_back_as_main_loads_them` |

**Risks**
- Engine rule players may trip on: a key with a hold binding fires its press on release
  (ENG-F26). Fire on Mouse Left plus Hold Mouse Left elsewhere makes firing late. Not
  guarded; noted for playtest.
