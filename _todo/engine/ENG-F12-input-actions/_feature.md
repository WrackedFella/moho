# ENG-F12 — Input is game-defined actions bound as data

**Issue:** [#82](https://github.com/WrackedFella/moho/issues/82)
**Status:** unknown
**Labels:** feature, line:engine


## Summary

Each game names its own actions; the engine maps devices to them from data the
player can rebind. Replaces ROADMAP 1.5, which would have moved engine input
into a strategy crate.

## Exit criteria

- The engine maps keys, mouse buttons, mouse delta and gamepad to a
  game-supplied action type.
- The action map can produce the per-tick command set ENG-F11's tick takes
  as input (ADR-0011 rule 1).
- Bindings are data, rebindable at runtime and persisted in prefs:
  ```gherkin
  Scenario: A rebinding survives a restart
    Given action "jump" bound to Space
    When the player rebinds it to F, saves prefs and reloads
    Then pressing F triggers "jump"
    And pressing Space does not
  ```
- The strategy game's state type lives on the strategy line; the
  engine-side state crate is deleted once empty.

## Scope

- In: action mapping, gamepad, data bindings, prefs persistence.
- Out: rebinding UI beyond what the strategy settings already have.

## Notes

- Gamepad: `gilrs`, confirmed by the [ENG-F13 audit](../ENG-F13-dependency-audit/audit.md).
- ENG-F13 call: `phf` is dropped. The key-name and key-label tables become
  `match` functions when this feature reworks key naming; no `phf` remains in
  any manifest when it closes.
- Prefs stay INI (ENG-F13 call: keep `ini`).

## Items

| Item |
|---|
