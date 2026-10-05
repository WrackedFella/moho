# Mined blocks stay mined after quitting and reloading

**Status:** not started (reproduce first)
**Feature:** [SG-F2](_feature.md)

## Summary

Mined blocks have been reported reverting after save and reload. The symptom
has not been reproduced, and the original hypothesis is ruled out (Notes).
This card reproduces the report, then fixes the path that loses edits. If no
path loses edits, it closes as not reproducible, with the passing tests kept as
regression guards.

## Deliverables

- A recorded reproduction (steps and which session-end path), or a statement
  that each path in the acceptance criteria persists edits.
- One automated test for each path below. A fix only for paths that fail.

## Acceptance criteria

```gherkin
Scenario Outline: Mined blocks survive the end of a session
  Given a loaded world
  And the player has mined a block
  And <movement>
  When the session ends by <exit>
  And the player loads the save
  Then the mined block is absent

  Examples:
    | movement                                            | exit               |
    | the player stays near the block                     | closing the window |
    | the player stays near the block                     | Exit in the menu   |
    | the player walks out of range and back              | closing the window |
    | the player walks out of range and stays away        | closing the window |
```

## Tech spec

Written after reproduction: the failing path decides the design.

## Notes

- **Ruled out (2026-10):** "mining never sets the chunk's modified flag".
  Clearing a block marks its chunk modified, so a mined chunk is saved on
  eviction.
- **Candidate gap, found by reading the code:** a session is saved only on
  window close or menu Exit. A kill, crash or terminal Ctrl+C leaves the
  `scene.bin` written at world generation (or the last clean exit). On load,
  that file's blocks fill their columns, and chunk files saved on eviction for
  those columns are never read. If this is the reported symptom, the fix needs a
  save policy (periodic, or on edit) and a scenario for it. Both are product
  calls for the Business Analyst before any design.
- The fix depends on the save-format migration
  ([ENG-F2-01](../../engine/ENG-F2-dependency-upgrades/ENG-F2-01-bincode-migration.md)) only if it changes the save format.
