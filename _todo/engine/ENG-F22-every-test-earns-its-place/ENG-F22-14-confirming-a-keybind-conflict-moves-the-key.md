# Confirming a keybind conflict moves the key, Sprint included

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#181](https://github.com/WrackedFella/moho/issues/181)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

Confirming the "key already bound" dialog in Settings does nothing today: the binding the player chose is dropped. A conflict that involves Sprint also clears Move Forward instead of Sprint. After this, Confirm moves the key to the chosen action and unbinds the action that held it, whichever actions they are.

## Deliverables

- Confirm in the conflict dialog applies the pending binding in the game, not only in tests.
- Conflicts involving any binding action, Sprint included, resolve onto the right actions.

## Acceptance criteria

```gherkin
Scenario: Confirming a conflict moves the key
  Given default bindings
  And the player rebinds Move Left to W, which Move Forward holds
  When the conflict dialog is shown and the player confirms
  Then Move Left is bound to W
  And Move Forward is unbound

Scenario: Cancelling a conflict changes nothing
  Given default bindings
  And the player rebinds Move Left to W
  When the conflict dialog is shown and the player cancels
  Then every binding is unchanged

Scenario Outline: A conflict involving Sprint resolves onto Sprint
  Given default bindings
  When the player rebinds <target> to the key <holder> holds and confirms
  Then <target> has that key
  And <holder> is unbound
  And Move Forward is unchanged unless it is the target or the holder

  Examples:
    | target    | holder    |
    | Move Down | Sprint    |
    | Sprint    | Move Left |
```

## Tech spec

**Design**
- The adapter takes the dialog from the screen (`Screen::take_pending_modal`) on one frame and calls `Screen::on_modal_confirm` on a later one. Taking the dialog must hide it but keep the pending binding (`ConflictModalState::hide` already does; `KeybindCaptureHandler::take_conflict_modal` uses `mem::take`, which discards it).
- Map binding ids to settings fields through `BindingId` (which has Sprint) instead of the 0-5 match with a Move Forward fallback.

**Test map**
| Scenario | Test |
|---|---|
| Confirm moves the key | `tests/settings_menu.rs` `confirming_conflict_through_modal_flow_replaces_binding`: `with_prefs`, listen 1, key W, `take_pending_modal()` is `Some`, then `on_modal_confirm()` |
| Cancel changes nothing | `tests/settings_menu.rs` `cancelling_conflict_through_modal_flow_keeps_bindings` |
| Sprint outline | `screens::settings::keybind_capture` `pending_apply_resolves_sprint_conflict` (handler level, both rows) |

Also update `tests/settings_menu.rs` `confirming_conflict_replaces_binding` and `keybind_capture` `test_pending_binding_apply` to go through the same take-then-confirm order, or fold them into the tests above.

**Out of scope:** capture staying armed while a modifier-key conflict dialog is up (suspected, unconfirmed); binding format changes (#149, ENG-F12-02).

**Gate class:** glue (UI).

**Risks:** low; the change is confined to the settings screen.

## Verification

In game: Settings → Controls, rebind Move Left to W, confirm. Move Left shows W and Move Forward shows Unbound; save and restart, the bindings persist.

## Notes

Both defects were reproduced on `dev` with throwaway tests: after take-then-confirm, Move Left stayed A; a Move Down/Sprint conflict cleared Move Forward.
