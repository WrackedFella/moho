# Hotbar slot range, config defaults and missing-character moves behave as decided

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#187](https://github.com/WrackedFella/moho/issues/187)
**Status:** Backlog
**Gate class:** domain
**Labels:** line:engine

## Summary

Three engine and game behaviours are settled: selecting a hotbar slot that doesn't exist is ignored, the app config builder takes unset fields from the prefs it is given, and moving a character that doesn't exist reports that instead of returning a zero position.

## Deliverables

- The three behaviours below, each pinned by a test that failed before the change.
- `AppConfig::streaming`, which nothing reads, is deleted.

## Acceptance criteria

```gherkin
Scenario: Selecting a slot past the hotbar is ignored
  Given slot 2 is selected
  When slot 8 is selected
  Then slot 2 is still selected and the equipped tool is unchanged

Scenario: The config builder takes unset fields from supplied prefs
  Given prefs with mouse sensitivity 2.5 and filtering off
  When a config is built with those prefs and no explicit sensitivity
  Then the config's sensitivity is 2.5 and filtering is off

Scenario: Moving a character that doesn't exist reports nothing moved
  Given a physics world with no character
  When the character is moved
  Then the result is None
```

## Tech spec

**Design**
- `Pawn::select_slot` returns early when `slot >= HOTBAR_SLOT_COUNT`.
- `AppConfigBuilder::build` falls back to the supplied prefs' sensitivity and filtering before `Prefs::default()`'s.
- `PhysicsWorld::move_character` returns `Option<Vec3>`; the binary's wrapper already returns `Option`, so it passes the value through.

**Test map**
| Scenario | Test |
|---|---|
| Slot out of range | `moho_game::pawn` `select_slot_out_of_range_is_ignored` |
| Builder derives from prefs | binary `app::config` `builder_with_prefs_derives_unset_fields_from_them` |
| No character | `moho_physics::world` `move_character_without_character_returns_none` |

**Out of scope:** handle-keyed characters (ENG-F15); hotbar size changes.

**Gate class:** domain (`moho_game`'s hotbar rule; the other two are glue and ride along).

**Risks:** `move_character`'s signature change touches the binary's physics controller; ENG-F15 and ENG-F21 will reshape this API, so keep the change minimal.
