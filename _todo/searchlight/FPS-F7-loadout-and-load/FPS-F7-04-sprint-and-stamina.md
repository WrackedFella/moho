# The player sprints until stamina runs out, and load takes sprint away

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
Sprint exists, limited by a stamina pool that drains while sprinting and recovers
otherwise. Load at the no-sprint threshold caps sprint through the stat layer.
Sprint and stamina need no items; only the load scenario depends on FPS-F7-03.

## Deliverables
- A sprint key (hold); sprinting moves faster than walking, forward only.
- A stamina pool and recovery rate as stats with base values in data.
- Stamina drains while sprinting and recovers while not; at zero, sprint ends until
  stamina has recovered past a threshold in data.
- The no-sprint load threshold caps sprint.

## Acceptance criteria
```gherkin
Scenario: Sprinting is faster than walking
  Given the player has full stamina
  When the player sprints forward
  Then the player moves faster than walking

Scenario: Sprint ends when stamina runs out
  Given the player is sprinting
  When stamina reaches zero
  Then the player walks
  And sprint does not start again until stamina recovers past its threshold

Scenario: Stamina recovers when not sprinting
  Given the player's stamina is half full
  When the player walks for the recovery time
  Then stamina is full

Scenario: Sprint is forward only
  Given the player has full stamina
  When the player sprints while moving backward or sideways
  Then the player moves at walking speed
  And stamina does not drain

Scenario: Load at the no-sprint threshold takes sprint away
  Given the player's load is at the no-sprint threshold
  When the player presses sprint
  Then the player moves at the slowed speed
  And the character screen lists load as capping sprint

Scenario: Sprint is refused while leaning
  Given the player is leaning
  When the player presses sprint
  Then the player keeps the leaning movement speed
  And stamina does not drain
```

## Verification
- Sprint between cover in a wave: stamina runs out at a point that forces a decision,
  not a stutter. Record the pool, drain and recovery tried on this issue.

## Notes
- No stamina bar on the HUD (GDD §3.3 limited HUD); breathing or other cues are a
  later presentation pass.
- Sprint while crouched or prone belongs to stance (FPS-F4). The leaning rule comes from FPS-F4; its scenario applies once lean has landed, whichever card lands second carries it.
