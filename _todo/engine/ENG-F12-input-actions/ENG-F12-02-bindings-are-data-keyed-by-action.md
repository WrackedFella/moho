# Bindings are data keyed by action name

**Feature:** [ENG-F12](_feature.md)
**Issue:** [#149](https://github.com/WrackedFella/moho/issues/149)
**Status:** unknown
**Gate class:** domain
**Labels:** line:engine

## Summary

Prefs hold one hard-coded field per strategy key (`key_w`, `key_jump`, ...), so
the engine knows the strategy game's controls. After this card a game declares
its actions with default bindings, prefs store a `[bindings]` section keyed by
action name, and key naming lives in the engine's input crate without `phf`.

## Deliverables

- A game declares its actions (stable name, default bindings) in its own crate.
- Prefs persist bindings by action name; rebinding at runtime survives a restart.
- The strategy game and its settings screen use the actions; controls behave as before.
- `phf` is gone from every manifest.

## Acceptance criteria

```gherkin
Scenario: A rebinding survives a restart
  Given action "jump" bound to Space
  When the player rebinds it to F, saves prefs and reloads
  Then pressing F triggers "jump"
  And pressing Space does not

Scenario: An action missing from prefs gets its default
  Given a prefs file whose [bindings] section has no "jump" line
  When bindings load
  Then "jump" is bound to its default

Scenario Outline: A bad bindings line is reported and the default kept
  Given a prefs file with "<line>" in [bindings]
  When bindings load
  Then a warning names "<reported>"
  And every declared action keeps its default unless its line was valid

  Examples:
    | line              | reported |
    | jump = Banana     | jump     |
    | teleport = T      | teleport |

Scenario: "Unbound" leaves an action with no binding
  Given "jump = Unbound" in [bindings]
  When bindings load
  Then no key triggers "jump"

Scenario: Old per-key lines are reported, not migrated
  Given a prefs file with "key_w = Z" in [prefs]
  When prefs load
  Then a warning reports the unknown key "key_w"
  And "move_forward" is bound to its default

Scenario: Every key name round-trips
  Given any key the engine names
  When its name is written and parsed back
  Then the same key results
```

- [ ] Aliases parse as today (`Up`, `Esc`, `Return`, `Space`/`Spacebar`, `Control`), case-insensitively.
- [ ] No manifest names `phf`.

## Tech spec

**Design.**
- `moho_input::Key`: platform-free enum, one variant per name in today's
  `KEY_NAME_TO_CODE` (letters, digits, punctuation, arrows, Escape, Tab,
  Backspace, Enter, Space, Shift, Ctrl, Alt). `name()`, `label()` and
  `parse(&str) -> Option<Key>` are `match` functions; canonical names are what
  `binding_to_string` writes today. `Key::from_winit(PhysicalKey) -> Option<Key>`
  replaces `physical_key_to_binding_code`. `u32` key codes and the `mods` field go.
- `moho_input::Binding`: `enum Binding { Key(Key) }` (03 and 04 add variants).
  A value is a comma-separated list of bindings; `Unbound` or empty means none.
- `moho_input::Action`: trait for the game's action enum:
  `Copy + Eq + Hash + Debug + 'static`, `const ALL: &'static [Self]`,
  `fn name(self) -> &'static str` (stable id),
  `fn default_bindings(self) -> &'static [Binding]`. Data on an enum, no
  registration machinery.
- `moho_input::ActionBindings<A>`: action → `Vec<Binding>`.
  `load(&BTreeMap<String, String>) -> (Self, Vec<BindingWarning>)`,
  `to_section() -> BTreeMap<String, String>` (writes every action),
  `get(A) -> &[Binding]`, `set(A, Vec<Binding>)`. Takes the raw section, not
  `Prefs`, so `moho_input` needs no `moho_core` dependency.
- `moho_core::prefs`: `key_*` fields, `Binding`, `parse_binding`,
  `binding_to_string` and `key_names.rs` are deleted. `Prefs` gains a raw
  `[bindings]` section (`bindings() -> &BTreeMap<String, String>`,
  `set_bindings`) round-tripped verbatim. Old `key_*` lines become the reader's
  existing `UnknownKey` warning; nothing else is needed.
- Strategy: `moho_ui::actions::StrategyAction` (`move_forward`, `move_back`,
  `move_left`, `move_right`, `ascend`, `descend`, `sprint`, `jump`) with today's
  defaults. `moho_ui`, not `moho_game`: `moho_game` is platform-free and
  `moho_input` names winit.
- Settings screen: `BindingId` → `StrategyAction`; it lists the same seven
  actions as today (not `jump`, which shares Space with `ascend`).
  `BindingRegistry` holds `ActionBindings<StrategyAction>`; capture converts
  `egui::Key` → `Key` in `key_mapping.rs`; labels come from `Key::label`.
  `modifier_encoding.rs` is deleted.
- Binary: `active_keys: HashSet<Key>`; `update_controller_input` asks whether
  any binding of an action is held. ENG-F12-03 replaces this.

**Out of scope.** Mouse buttons and gamepad bindings (03, 04). Per-tick
sampling (03). A new settings layout, a `jump` row, or conflict-rule changes.
Bumping `ini`.

**Test map.**
| Criterion | Test | Class |
|---|---|---|
| Rebinding survives restart | `moho_input::bindings::tests::rebinding_survives_save_and_reload` (dev-dep `moho_core`, temp prefs file) | domain |
| Missing action → default | `…::tests::missing_action_gets_its_default` | domain |
| Bad line reported | `…::tests::bad_line_is_reported_and_default_kept` (cases table) | domain |
| Unbound | `…::tests::unbound_leaves_action_without_binding` | domain |
| Old lines | `moho_core::prefs::reader::tests::old_key_lines_are_unknown_keys` | domain |
| Names round-trip | `moho_input::key::tests::every_key_round_trips_through_its_name` (over all variants), `aliases_parse_to_their_key` | domain |
| Section round-trips raw | `moho_core::prefs::tests::bindings_section_round_trips` | domain |
| Settings unchanged | `moho_ui/tests/settings_menu.rs` passes after the type swap | glue |

**Gate class:** domain (changes `moho_core` prefs).

**Risks.** Players' saved key choices reset once (decided: no migration).
Touches the settings screen that ENG-F1-04 (keybind test layers) also edits;
whichever lands second rebases.

## Verification

- Rebind forward to another key in settings, save, restart: the new key moves
  the player and W does not.
