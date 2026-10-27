# #720 FPS-F14-02 Taking an input in use asks before it replaces

**Gate class:** glue. **Depends on:** #719 (shares every file), #728 core.

## Card changes

None. Notes gains: the dialog is an ordinary menu screen (title and two buttons), not a new
engine widget.

## Tech spec

**Design**
- `controls`: `Capture::InUse { by }` becomes `Capture::Conflict { by, slot }`, and the draft
  keeps that capture pending. `replace()` puts the pending binding in the target slot and
  empties the slot of `by` that held it; `cancel()` drops it. Nothing changes until one is
  called. A second capture while one is pending replaces the pending one.
- `controls`: `ControlsDraft::is_unbound(action)`: every slot and the rest empty.
- `menu`: `conflict_screen(input_label, by_label)`: title `"<input> is used by <action>"`,
  buttons Replace and Cancel (`MenuAction::Replace`, `MenuAction::Cancel`), Escape is Cancel.
  It is pushed over the Controls screen and popped on either answer, so the Controls screen
  keeps its focus and staged state.
- `game`: answers Replace and Cancel through the draft and refreshes the two affected rows.
- Engine seam (#728 core): Unbound slot text and the unbound-action mark.
- Why a pushed screen over a modal widget: the menu kit already does it, with Escape and
  focus handled; a modal kind would be engine work for no extra behavior.

**Out of scope**
- Secondary slot capture (#721); conflict on a picker change (#723).
- Swapping the two inputs instead of leaving the other slot Unbound (not the decided rule).

**Footprint**
- `searchlight/src/controls.rs` and its tests
- `searchlight/src/menu.rs` (`conflict_screen`, two actions) and its tests
- `searchlight/src/game.rs` (push, answer, pop) and its tests

**Test map**
| Scenario | Test |
|---|---|
| Taking an input in use asks first | `game::tests::capturing_an_input_in_use_pushes_a_dialog_naming_its_action` and `controls::tests::a_conflict_changes_nothing_until_answered` |
| The dialog resolves: Replace | `controls::tests::replace_moves_the_input_and_leaves_the_other_slot_unbound` |
| The dialog resolves: Cancel | `controls::tests::cancel_keeps_both_actions` |
| A replaced input moves in play | `game::tests::an_applied_replace_moves_the_input_in_play` |
| An unbound action is visible | `menu::tests::an_action_with_no_binding_is_marked_unbound` |
| edge | `game::tests::escape_on_the_dialog_cancels_and_returns_to_the_same_row` |
| edge | `controls::tests::a_new_capture_replaces_a_pending_conflict` |
| edge | `controls::tests::replace_empties_only_the_slot_that_held_the_input` (Check magazine keeps Hold R when C is taken) |

**Risks**
- Small; game-only once #719 is in.
