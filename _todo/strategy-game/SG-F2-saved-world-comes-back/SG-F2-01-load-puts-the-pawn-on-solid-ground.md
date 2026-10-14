# Loading a save puts the pawn on solid ground

**Feature:** [SG-F2](https://github.com/WrackedFella/moho/issues/308)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

After loading a save the pawn can fall through the world to the kill plane and respawn repeatedly. Loading must leave the pawn standing on terrain.

## Deliverables

- A recorded reproduction on the load path, then a failing test, then the fix.
- New-world spawn is out: it does not reproduce (24 seeds).

## Acceptance criteria

```gherkin
Scenario: Loading a save puts the pawn on solid ground
  Given a saved world with terrain under the player's saved position
  When the player loads the save
  Then the pawn stands on that terrain
  And the pawn does not reach the kill plane

Scenario: The loaded pawn stays put
  Given the player loaded a save
  When 10 seconds pass without input
  Then the pawn is still standing on terrain
```

## Verification

Manual, several times: save, quit, relaunch, load, and watch the pawn for 10 seconds. Do this before verifying SG-F2-02 and SG-F2-03 by hand.

## Notes

- Headless probe: load reproduces, 3 respawns in 10 s. If the fix does not clear the symptom the user reported (spawning below the ground), the trigger differs; record that on the issue.
