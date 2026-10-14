# The simulation draws random numbers that replay identically

**Feature:** ENG-F6
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary
A game's simulation draws random numbers from a seeded source held in its state, split into per-system streams, so the same seed and commands replay the same numbers on every platform and after dependency upgrades. Engine request #316; consumer FPS-F2-02 (#299) enemy aim error, which uses an interim generator until this lands.

## Deliverables
- A seeded source a game holds in simulation state, with named per-system streams.
- Restart from a seed, and save and restore of the source's state.
- Known-value tests that pin the sequence on every OS CI runs.

## Acceptance criteria
```gherkin
Scenario: The same seed gives the same numbers
  Given two sources made from the same seed
  When each draws the same sequence of values
  Then both sequences are equal

Scenario: The sequence is pinned
  Given a source made from a fixed seed
  When it draws its first values
  Then they equal the recorded known values

Scenario: Streams do not disturb each other
  Given streams for two systems from one seed
  When one system adds an extra draw
  Then the other system's numbers are unchanged

Scenario: A restored source continues where it was saved
  Given a source that has drawn some values
  When its state is saved and restored
  Then the restored source draws the same next values as the original

Scenario: Different seeds give different numbers
  Given two sources made from different seeds
  When each draws its first values
  Then the sequences differ
```

## Notes
- Replacing the FPS interim generator and strategy world generation's `rand` use are their lines' cards.
