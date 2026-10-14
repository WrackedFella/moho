# Saved settings survive a later console command

**Feature:** ENG-F18
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary
After the player saves bindings or window settings in Settings, a later console command that saves prefs (shadow or ambient-occlusion settings) writes back the old values and loses the player's changes. Found under #179. Prefs end with one owner, so ENG-F18's shell does not inherit the defect.

## Deliverables
- Settings saves and console prefs commands both keep the other's changes, in the running game and on disk.
- One in-memory owner of prefs in the running game.

## Acceptance criteria
```gherkin
Scenario Outline: A console prefs command keeps what Settings saved
  Given the player changed <setting> in Settings and saved
  When the player runs a console command that changes a shadow setting
  Then the prefs file holds both the new <setting> and the new shadow setting
  Examples:
    | setting       |
    | a key binding |
    | the window mode |

Scenario: Settings saved after a console command keep the console change
  Given the player changed a shadow setting from the console
  When the player changes a key binding in Settings and saves
  Then the prefs file holds both changes

Scenario: Both changes are in effect after a restart
  Given the player saved a key binding in Settings, then changed a shadow setting from the console
  When the game restarts
  Then the new binding works and the new shadow setting is applied
```

## Notes
- Lands before any ENG-F18 card that moves settings to the engine line.
- The bus carries notifications, not state ([ADR-0011](../../adr/0011-simulation-stays-network-ready.md)): prefs do not ride on the settings-saved event.
