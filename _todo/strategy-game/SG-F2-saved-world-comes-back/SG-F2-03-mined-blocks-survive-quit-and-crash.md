# Mined blocks survive quitting and crashing

**Feature:** [SG-F2](https://github.com/WrackedFella/moho/issues/308)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

Mined blocks have been reported reverting after save and reload. The world also saves only when the session ends cleanly, so a crash or kill loses the whole session. The world now autosaves on an interval, and on exit.

## Deliverables

- A recorded reproduction of the reported revert, or a statement that each exit path already persists edits. Passing paths stay as regression tests.
- Autosave every 2 minutes by default; the interval is configurable.

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
    | movement                                      | exit               |
    | the player stays near the block               | closing the window |
    | the player stays near the block               | Exit in the menu   |
    | the player walks out of range and back        | closing the window |
    | the player walks out of range and stays away  | closing the window |

Scenario: A killed session loses at most one autosave interval
  Given the player mined blocks over several autosave intervals
  When the process is killed
  And the player loads the latest save
  Then every block mined before the last autosave is absent
  And no edit older than one autosave interval is missing

Scenario: The autosave interval is configurable
  Given the autosave interval is set to 30 seconds
  When 30 seconds of play pass after a block is mined
  Then a save containing that edit exists
```

## Verification

Manual: mine, kill the process after an autosave, load, confirm the edit. Confirm an autosave causes no visible hitch.

## Notes

- Ruled out earlier: "mining never marks its chunk modified". Mining does mark it, so a mined chunk is saved on eviction.
- Candidate cause: a session saves only on clean exit, and on load the saved blocks fill their columns so chunk files saved on eviction are never read.
