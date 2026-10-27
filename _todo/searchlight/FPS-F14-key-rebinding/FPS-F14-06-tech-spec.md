# #724 FPS-F14-06 A slot binds a double press

**Gate class:** glue. **Depends on:** #723 (kind picker), #729 (double press in the engine).

## Card changes

None.

## Tech spec

**Design**
- Game work is small once #729 lands: the kind picker offers the engine's double-press kind,
  and the draft treats it like any other `Trigger` (conflict is still exact binding
  equality, so press, hold and double press of one key never conflict).
- Labels and the prefs name come from the engine (`Binding::label`, `Binding::name`).
- Engine seam (#729): the new trigger kind and its window; (#728 details) the picker entry.

**Out of scope**
- Tuning the double-press window (engine value).

**Footprint**
- `searchlight/src/controls.rs`, `searchlight/src/menu.rs`, `searchlight/src/game.rs` and
  their tests

**Test map**
| Scenario | Test |
|---|---|
| A double press does the action | `game::tests::an_applied_double_press_slot_fires_on_two_quick_presses` |
| A single press of that key is not a double press | `game::tests::one_press_of_a_double_press_key_does_not_fire_it` |
| A press and a double press of one key do not conflict | `controls::tests::a_double_press_and_a_press_of_one_key_do_not_conflict` |
| edge | `debug_settings::tests::double_press_slots_load_back_as_main_loads_them` |

**Risks**
- The delay on a key that carries both a press and a double press is the engine's
  behavior; the card's verification asks Justin to judge it in play.
