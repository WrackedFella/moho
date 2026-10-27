# #719 FPS-F14-01 The player rebinds an action's key and keeps it

**Gate class:** glue (`searchlight` is not a domain path). **Depends on:** #728 core.

## Card changes

- `Move forward | Up arrow` cannot pass: the engine refuses a lone arrow key or Enter as a
  binding (`Binding::is_protected`; menus read them), and the prefs loader drops one on load.
  Replace the example with `Move forward | I | the player moves ahead`, and the feature's
  verification line likewise (Justin decides; see the report).
- Reserved keys outline: add a row `| Up arrow | capture refuses it with a note |`, since the
  game refuses protected bindings.

## Tech spec

**Design**
- New `searchlight::controls` module, platform-free (no shell or winit types):
  - `ControlsDraft`: the bindings being edited, one row per `SearchlightAction` in `ALL`
    order. Each row holds two positional slots (`[Option<Binding>; 2]`) and the rest of the
    action's bindings beyond two, kept untouched. Built from `ActionBindings`; `bindings()`
    returns `ActionBindings` (filled slots in order, then the rest).
  - `ControlsDraft::capture(action, input) -> Capture`: `Set` (primary now holds the plain
    press of `input`), `InUse { by }` (another action has that exact `Binding`; nothing
    changes; 02 replaces this outcome), `Reserved` (`Binding::is_protected`; nothing
    changes). Capturing the binding the slot already holds is `Set` with no change.
  - `label(action) -> &'static str`: player-facing names ("Move forward", "Check magazine",
    "Restart"). An exhaustive `match`, so a new action fails to compile until it is labelled.
- `menu`: Settings gains a Controls button (`MenuAction::Controls`); `controls_screen(&draft)`
  builds the page.
- `game`: Controls opens a draft from the live bindings; each capture event goes through
  `capture`, and the page shows the new label or the refusal note (`"Q is used by Lean
  left"`, `"Up arrow is kept for menus"`). Apply saves and goes live; Back drops the draft.
- Going live: `ActionMap` fixes its hold split and toggle mask at construction, so Apply
  replaces the game's map with `input::build_action_map(draft.bindings(), prefs)`, which also
  re-reads the look settings from the same prefs. The map is fed nothing while a menu is
  open, so no held input is lost.
- `debug_settings`: `DebugSettings::set_bindings(&ActionBindings<_>)` writes `[bindings]` and
  saves, keeping the `console` and `fullscreen` entries the file holds (`CONSOLE_BINDING`,
  `FULLSCREEN_BINDING`); the game's loaders split those out at start, so dropping them would
  reset the console and fullscreen keys.
- Engine seam (#728 core): the screen item that lists the actions and slots, the capture
  event and the call that updates a slot's label or note in place. Names follow the engine
  card that answers #728.
- Why a game-side draft over editing `ActionBindings` directly: positional slots and the
  conflict rule are the game's, and the draft is testable without the shell.

**Out of scope**
- Secondary slot UI (#721), conflict dialog (#720), clear and reset (#722), pickers (#723),
  toggle (#725): the draft gets only `capture` into the primary.
- `moho_input` and `moho_ui_shell` changes, including an `ActionMap::set_bindings`: engine
  work goes through #728.
- Showing which key a tap fires on (release, when the key also has a hold binding).
- Strategy's key mapping screen.

**Footprint**
- `searchlight/src/controls.rs` (new) and its tests
- `searchlight/src/lib.rs` (module line)
- `searchlight/src/menu.rs` (Controls button, `controls_screen`) and its tests
- `searchlight/src/game.rs` (open, answer, apply, back) and its tests
- `searchlight/src/debug_settings.rs` (`set_bindings`) and its tests

**Test map**
| Scenario | Test |
|---|---|
| Controls lists every action | `menu::tests::controls_lists_every_action_with_its_primary_label` |
| A rebound input does the action (3 rows) | `game::tests::an_applied_rebinding_does_the_action_and_the_old_input_does_not` (table over Reload/T, Fire/Mouse Right, Move forward/I) |
| A rebinding survives a relaunch | `debug_settings::tests::applied_bindings_load_back_as_main_loads_them` (save, `Prefs::load_from`, split console and fullscreen, `ActionBindings::load`) |
| Back discards a captured input | `game::tests::back_from_controls_keeps_the_old_binding_live_and_shown` |
| Reserved keys: Escape, console key | `game::tests::escape_and_the_console_key_never_reach_capture` |
| Reserved keys: Up arrow | `controls::tests::a_protected_input_is_refused_as_reserved` |
| An input in use is refused | `controls::tests::an_input_another_action_holds_is_refused_naming_it` |
| edge | `controls::tests::capture_keeps_check_magazines_second_binding` |
| edge | `controls::tests::recapturing_the_same_input_changes_nothing` |
| edge | `controls::tests::bindings_beyond_two_survive_a_round_trip` |
| edge | `debug_settings::tests::set_bindings_keeps_the_console_and_fullscreen_entries` |
| edge | `controls::tests::every_action_has_a_distinct_label` |

**Risks**
- Engine API unknown until #728 is answered; only the seam lines should move.
- Prefs: every action is written on Apply (as `to_section` does), so a hand-edited invalid
  line is replaced by the value in use. No format change.
