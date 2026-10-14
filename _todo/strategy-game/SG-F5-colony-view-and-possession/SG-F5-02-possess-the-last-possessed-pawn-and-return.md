# Possess the last-possessed pawn and return to the colony view

**Feature:** [SG-F5](https://github.com/WrackedFella/moho/issues/309)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

From strategy mode a key takes the player into first person as the pawn they last possessed (the player pawn before any possession), and the same key returns to strategy mode. Selecting a pawn first arrives with click-select.

## Deliverables

- A possess key and a return key (the same key toggles).
- A mode indicator in the HUD and a cue on each switch (placeholder clip acceptable).

## Acceptance criteria

```gherkin
Scenario: Possessing with nothing selected
  Given the player is in strategy mode and has not possessed a pawn before
  When the player presses the possess key
  Then the view switches to first person from the player pawn

Scenario: Possessing the last-possessed pawn
  Given the player possessed a pawn and returned to strategy mode
  When the player presses the possess key
  Then the view switches to first person from that same pawn

Scenario: Returning to the colony view
  Given the player is possessing a pawn
  When the player presses the possess key
  Then the view switches to strategy mode

Scenario: The HUD shows the current mode
  Given the player switches mode
  When the view changes
  Then the HUD names the current mode
```

## Verification

Manual: switch both ways repeatedly; no camera jump; cue plays; the pawn does not move while the player is in strategy mode.
