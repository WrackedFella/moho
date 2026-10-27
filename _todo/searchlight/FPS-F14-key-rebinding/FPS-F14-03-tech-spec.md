# #721 FPS-F14-03 Each action takes a secondary input

**Gate class:** glue. **Depends on:** #720 (shares every file; its dialog is in the last
scenario), #728 core.

## Card changes

None.

## Tech spec

**Design**
- `controls`: `Slot { Primary, Secondary }`; `capture(action, slot, input)`. The slots are
  already positional from #719; this card exposes the second one. Capturing the binding the
  same action holds in its other slot is `Unchanged` (no dialog, the slot keeps its value).
- `menu`: each row shows both slots; Check magazine's default second binding (`Hold R`)
  appears as its secondary.
- Saving is unchanged: the draft writes filled slots in order, so a secondary is the second
  item of the action's `[bindings]` value.
- Known behavior: an empty primary with a filled secondary stays as placed until Apply; on
  the next open the secondary shows as the primary, since prefs store a list, not positions.
  Holding the position would need a prefs format change in the engine for no player benefit.
- Engine seam (#728 core): the second slot on each row.

**Out of scope**
- More than two slots; reordering slots.

**Footprint**
- `searchlight/src/controls.rs` and its tests
- `searchlight/src/menu.rs` and its tests
- `searchlight/src/game.rs` and its tests

**Test map**
| Scenario | Test |
|---|---|
| A secondary input works alongside the primary | `game::tests::an_applied_secondary_does_the_action_beside_the_primary` |
| A secondary survives a relaunch | `debug_settings::tests::applied_bindings_load_back_as_main_loads_them` (extended with a secondary) |
| One action's own slots do not conflict | `controls::tests::capturing_the_other_slots_input_changes_nothing` |
| A secondary in use by another action asks first | `controls::tests::a_secondary_capture_reports_the_conflicting_action` |
| edge | `menu::tests::check_magazine_shows_hold_r_as_its_secondary` |
| edge | `controls::tests::an_empty_primary_with_a_secondary_saves_the_secondary_first` |

**Risks**
- None beyond #719's.
