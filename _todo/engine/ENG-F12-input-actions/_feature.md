# ENG-F12 — Input is game-defined actions bound as data

**Issue:** #82


## Summary

Each game names its own actions; the engine maps devices to them from data the
player can rebind. Replaces ROADMAP 1.5, which would have moved engine input
into a strategy crate.

## Exit criteria

- The engine maps keys, mouse buttons, mouse delta and gamepad to a
  game-supplied action type.
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

- Gamepad support adds a dependency; ENG-F13 must give it a verdict first.

## Items

| Item |
|---|
