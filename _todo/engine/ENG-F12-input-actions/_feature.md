# ENG-F12 — Input is game-defined actions bound as data

**Issue:** #82

## End state

Both games read player intent as their own named actions, never as keys. The
engine turns keyboard, mouse and gamepad into those actions each tick, from
bindings the player can change, and hands the result to the game's tick as
its command input ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md) rule 1).

## Summary

Each game names its own actions; the engine maps devices to them from data the
player can rebind. Replaces ROADMAP 1.5, which would have moved engine input
into a strategy crate.
**Moves toward the end state by:** giving ENG-F11's tick its input and
ENG-F21's controller its commands, so the FPS prototype gets input without
copying strategy code.

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

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| How are actions identified in data? | By a stable name the game gives each action ("jump") | Prefs and future mods ([ENG-F19](../ENG-F19-data-and-mod-content/_feature.md)) reference actions by stable id; an index or enum discriminant breaks saved bindings when the game reorders actions |
| Digital or analog actions? | Digital actions plus one look delta now; a stick direction binds as a digital action. **Analog movement axes deferred** | Analog axes are a second action kind with its own binding syntax and dead-zone rules; no consumer needs them before FPS P.2. The binding syntax leaves room for them |
| Key combinations (Ctrl+X)? | Not supported; the unused modifier field goes | The game already ignores modifiers; chords need conflict and release rules nobody has asked for |
| Old `key_w=`-style prefs | Not migrated: ignored with a warning, defaults apply | One user, pre-release; a migration path is code kept forever for one file |
| Keys by position or by character? | Position (physical key), as today | WASD stays WASD on AZERTY; labels may show the US name (deferred) |
| A tap shorter than one tick | Registers as pressed on the next tick | Otherwise fast taps are lost at low frame rates and the result depends on frame timing |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Analog axis actions (stick movement, trigger pressure) | No consumer before FPS P.2 | The FPS line files an engine request |
| Action sets / contexts (menu vs. gameplay) | The game decides when it reads actions; one set is enough | A game needs two bindings for one key in different modes |
| Key chords and modifiers | Unused today | A game asks for one |
| Layout-aware key labels (AZERTY shows "Z") | Cosmetic | Localisation work |
| Rebinding gamepad buttons in the settings screen | Out of scope (no new rebinding UI); gamepad bindings are edited in the prefs file | The UI shell (ENG-F18) or a strategy settings card |
| Per-frame look for presentation between ticks | ENG-F21 owns camera smoothing | ENG-F21's spec |

## Items

| Item |
|---|
| #148 [ENG-F12-01](ENG-F12-01-strategy-app-state-on-strategy-line.md): the strategy game's app state lives on the strategy line |
| #149 [ENG-F12-02](ENG-F12-02-bindings-are-data-keyed-by-action.md): bindings are data keyed by action name |
| #150 [ENG-F12-03](ENG-F12-03-actions-arrive-once-per-tick.md): the game reads its actions once per tick |
| #151 [ENG-F12-04](ENG-F12-04-gamepad-drives-actions.md): a gamepad drives the same actions |

Order: 01 is independent. 02 → 03 → 04. 03 and ENG-F11 meet at the tick:
whichever lands second wires the action frame into the tick.

## Notes

- Gamepad: `gilrs`, confirmed by the [ENG-F13 audit](../ENG-F13-dependency-audit/audit.md).
- ENG-F13 call: `phf` is dropped. The key-name and key-label tables become
  `match` functions when this feature reworks key naming; no `phf` remains in
  any manifest when it closes.
- Prefs stay INI (ENG-F13 call: keep `ini`).
