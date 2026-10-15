# Sensitivity uses Source's scale, and Y can be inverted

**Feature:** ENG-F28
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine, M2

## Summary
Sensitivity uses Source's units (0.022° per count × sensitivity), so a player's CS or
HL2 value feels the same here. The Settings screen adds an invert-Y toggle.

## Deliverables
- Sensitivity in Source units, default 3.0. A saved value is read on the new scale with no migration.
- An invert mouse Y setting, off by default. It applies to the mouse only.
- Both settings are saved in prefs and apply on Save without a restart.

## Acceptance criteria
```gherkin
Scenario Outline: Sensitivity matches Source
  Given sensitivity is <sensitivity> and acceleration is off
  When the mouse moves <counts> counts horizontally
  Then the view turns <degrees> degrees

  Examples:
    | sensitivity | counts | degrees |
    | 1.0         | 1000   | 22      |
    | 3.0         | 1000   | 66      |
    | 2.5         | 400    | 22      |

Scenario: Default sensitivity is 3.0
  Given no prefs file
  When the game starts
  Then sensitivity is 3.0

Scenario: Inverted Y turns the view the other way
  Given invert mouse Y is on
  When the mouse moves up
  Then the view pitches down

Scenario: Invert Y leaves the stick alone
  Given invert mouse Y is on
  When the right stick is pushed up
  Then the view pitches up

Scenario: Settings apply on Save and survive a restart
  Given the player sets sensitivity to 1.8 and turns invert mouse Y on, then saves
  Then the next mouse movement uses the new values
  And after a restart both values are kept
```

## Tech spec

## Verification
- Enter your usual CS sensitivity and compare a 360° swipe with the same mouse and DPI in a Source game.
