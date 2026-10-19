# SG-F6 — New worlds look intentional and ore is findable

**Status:** Draft
**Labels:** feature, line:strategy
**Note:** No exit criteria until the user ranks the world-generation problems.

## End state

Each world looks designed, offers somewhere sensible to start, and gives the player a reason to dig. See the [vision](../../../wiki/flagstone/vision.md).

## Summary

World generation looks unpolished. Known: smoothing, caves and level-of-detail transitions need work. The rare holes at angled surface intersections and spawning in terrain are tracked in [SG-F2](https://github.com/WrackedFella/moho/issues/308).
**Moves toward the end state by:** the colony loop is only as interesting as what there is to dig.

## Exit criteria

To be written after the user ranks the top three problems. Candidates:

```gherkin
Scenario: The start area is flat and safe
  Given a new world
  When it is generated
  Then the start area is level and clear of overhangs

Scenario: Ore can be found
  Given a new world
  When the player digs near the start
  Then ore is reachable within a distance the user sets
```

## Scope

- In: to be narrowed by the ranking (smoothing, caves, level of detail, ore placement, start area).
- Out: new biomes; structures.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Are caves in scope? | Open | Caves change what digging means and what the underground view must show |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Everything not ranked | No ranking yet | When the user ranks |

## Items

| Item |
|---|

## Notes

- Ore sits under the top ~4 blocks, so surface scraping yields nothing (finding from the former SG-F1-02).
