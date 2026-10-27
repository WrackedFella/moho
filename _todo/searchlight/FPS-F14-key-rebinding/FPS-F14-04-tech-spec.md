# #722 FPS-F14-04 The player clears a slot or resets every binding

**Gate class:** glue. **Depends on:** #721 (shares every file; clears the secondary), #728
core.

## Card changes

None.

## Tech spec

**Design**
- `controls`: `clear(action, slot)` empties the slot; `reset_all()` sets every row to the
  action's `default_bindings()`. Both act on the draft, so Back discards them.
- `menu`: a Reset button on the Controls screen (`MenuAction::ResetControls`), a plain button
  the menu kit already has.
- No guard against an action left with no binding (Fire, Move): the feature allows it, shows
  it as Unbound (#720) and Reset recovers it.
- Engine seam (#728 core): the clear gesture on a focused slot.

**Out of scope**
- Per-action reset (#723); a confirm dialog on Reset (not asked for).

**Footprint**
- `searchlight/src/controls.rs` and its tests
- `searchlight/src/menu.rs` and its tests
- `searchlight/src/game.rs` and its tests

**Test map**
| Scenario | Test |
|---|---|
| A slot can be cleared | `game::tests::a_cleared_secondary_stops_working_and_the_primary_still_does` |
| Clearing every slot leaves the action Unbound | `controls::tests::clearing_both_slots_leaves_the_action_unbound` |
| Reset the page | `game::tests::an_applied_reset_puts_every_action_on_its_defaults` |
| Reset waits for Apply | `game::tests::reset_then_back_keeps_the_applied_bindings` |
| edge | `controls::tests::reset_all_restores_check_magazines_two_defaults` |
| edge | `controls::tests::clearing_an_empty_slot_changes_nothing` |

**Risks**
- None beyond #719's.
