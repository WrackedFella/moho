# Slow and small mouse movements always turn the view

**Feature:** ENG-F28
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine, M2

## Summary
Mouse look becomes 1:1. Any mouse movement turns the view by an amount proportional to
the counts moved, and the same total movement always gives the same total turn.
Smoothing is opt-in and keeps the total turn intact.

## Deliverables
- No deadzone on mouse look; the stick keeps its deadzone.
- Smoothing is off by default. When on, it averages recent movement without losing any.
- The Settings "Input filtering" toggle is replaced by "Mouse smoothing", which is saved in prefs.

## Acceptance criteria
```gherkin
Scenario Outline: Mouse movement turns the view in proportion, at any speed
  Given smoothing and acceleration are off
  When the mouse moves <counts> counts spread over <ticks> ticks
  Then the view turns by <counts> times the per-count turn

  Examples:
    | counts | ticks |
    | 1      | 1     |
    | 4      | 4     |
    | 300    | 1     |
    | 300    | 60    |

Scenario: Smoothing spreads a movement without losing any of it
  Given mouse smoothing is on
  When the mouse moves 100 counts in one tick and then stops
  Then the view turns less than the full amount on that tick
  And once the mouse has been still for a few ticks the total turn equals 100 counts

Scenario: The stick still ignores a small deflection
  When the right stick rests inside its deadzone
  Then the view does not turn

Scenario: Mouse smoothing survives a restart
  Given the player turns mouse smoothing on in Settings and saves
  When the game restarts
  Then mouse smoothing is on

Scenario: An old input filtering preference is ignored
  Given a prefs file that still holds the old input filtering key
  When the game starts
  Then a warning names the key
  And mouse smoothing is off
```

## Tech spec

## Verification
- With smoothing and acceleration off, move the mouse very slowly while looking at a
  thin vertical edge. The view creeps with the hand and never sticks and then jumps.
