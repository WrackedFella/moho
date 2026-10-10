# Several characters run at once, each by its own handle

**Feature:** ENG-F15 (#227)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Physics holds one character today. Each character becomes its own entry keyed by a
handle, with its own vertical speed and grounded state, so a player and several
enemies move at once. Consumers: P.2 (player plus AI enemies); the strategy game moves
its player onto a handle, and its unchanged behaviour proves the change.

## Deliverables

- A game adds, moves, teleports, reads and removes any number of characters by handle.
- Each character falls, lands and reports grounded on its own.
- The strategy game's player uses a handle; no single-character API remains.

## Acceptance criteria

```gherkin
Scenario: Two characters move independently
  Given two characters standing on a floor
  When only the first is moved 1 m along X
  Then the first character is 1 m further along X
  And the second has not moved

Scenario: Each character keeps its own grounded state
  Given one character standing on a floor and one in the air above it
  When the world is stepped once
  Then the first reports grounded
  And the second does not

Scenario: Removing one character leaves the others
  Given two characters
  When the first is removed
  Then moving the second still works
  And moving the first is refused

Scenario: A handle from a removed character does not reach a new one
  Given a character that was removed
  And a new character was added
  When the old handle is moved
  Then the move is refused
  And the new character has not moved

Scenario: The strategy player walks as before
  Given the strategy game in first-person walking mode
  When the existing character movement tests run
  Then they pass unchanged in what they assert
```

## Verification

- Strategy game: walk, jump, fall off the map and respawn; behaviour matches `dev`.
