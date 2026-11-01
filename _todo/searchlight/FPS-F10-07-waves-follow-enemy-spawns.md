<!-- board: gate=glue; eligible=no; priority=P2 -->
# Waves run only where a level places enemy spawns; no level has a mode

**Feature:** FPS-F10
**Status:** Draft
**Gate class:** glue
**Labels:** line:fps

## Summary
FPS-F10-01 (#627, PR #789) gave every level a mode, arena or sandbox. Justin dropped the
idea on 2026-10-29: all Searchlight maps are one kind, and waves are a prototype test
feature, not a map mode (a wave attack may return later as a job type). Waves now run on a
level when, and only when, the level file places enemy spawns. The arena keeps its waves;
the proving ground and borrowed maps have no spawns, so they stay quiet.

## Deliverables
- A level file names no mode. The `searchlight_level` marker, the `LevelMode` it selects
  and the arena-versus-sandbox marker rules are gone; one set of marker rules serves every level.
- Waves, score and the wave and score line in the HUD appear on a level that places at
  least one enemy spawn, and on no other.
- The `grade` console command says there are no waves on a level without enemy spawns.
- The level launch, level list, load notices and the next-wave button behave as before.
- The text no longer speaks of an arena mode or a sandbox mode (see Wording below), and
  FPS-F10-03 (#741) is reworded to match.

## Acceptance criteria
```gherkin
Scenario: A level with enemy spawns runs waves
  Given the tester launches Searchlight with level "arena"
  Then wave 1 and a score show
  And enemies spawn

Scenario: A level without enemy spawns stays quiet
  Given the tester launches Searchlight with level "proving_ground"
  When 60 seconds pass
  Then no enemy has spawned
  And no wave or score shows

Scenario: A borrowed map stays quiet
  When the tester launches with a level path whose file places no enemy spawns
  Then the map loads with a player spawn
  And no enemy has spawned

Scenario: A level file with an old mode marker still loads
  Given a level file carries a "searchlight_level" marker
  When the tester launches it
  Then the level loads and the marker has no effect

Scenario: Forcing a grade needs waves
  Given the proving ground has loaded
  When the tester runs the console command "grade" with "rookie"
  Then the console says there are no waves on this level

Scenario: A restart keeps the level's behaviour
  Given the player dies on a level with no enemy spawns
  When the player restarts
  Then the same level reloads with no enemies and no wave or score

Scenario: A restart on a level with spawns starts the waves again
  Given the player dies on the arena
  When the player restarts
  Then wave 1 starts again with the score at zero
```

## Wording
- GDD §3 (arena scaffolding, two bullets): "mode layer" becomes "wave layer"; the "Arena
  as a long-term mode" bullet is replaced by "Wave attacks may return later as a job type; no map has a mode."
- GDD §6 (not a score-chasing game): "Not a score-chasing game: waves are a prototype test feature."
- GDD §7 (disposable scaffolding): "sit in the wave layer (§3)".
- Roadmap, FPS-F10 line: "a test level with distance targets and cover heights, and
  spawn and reset controls" (drops "a sandbox mode").
- Roadmap, FPS-F16 line: the add-on's first slice places spawns, waypoints, cover and
  dummies (drops "level mode") and "launches the game on it" (drops "in sandbox mode").
- `wiki/searchlight/architecture.md`: the mode passages in the module table, the arena
  contract and the waves section (through `/devflow:wiki`).
- Issue edits, not in the PR: FPS-F10-03 (#741) scenario "loads in sandbox mode" becomes
  "the proving ground loads"; check FPS-F16 cards (#791 to #793) and FPS-F10-02 (#629)
  stay free of "mode".

## Notes
- TL: which marker rules stay strict. Today only an arena requires `player_spawn`, a
  `dummy_NN`, a waypoint for every enemy spawn and numeric waypoint suffixes, and only a
  sandbox tolerates a failed navmesh. Proposal: apply the strict waypoint rules only when
  the file places enemy spawns, keep `player_spawn` and `dummy_NN` optional (origin and
  console notice as now), and treat a navmesh failure as a warning plus console line on a
  level with no spawns and as an error on one with spawns.
- TL: a stale `searchlight_level` marker is ignored without a warning (scenario above).
- Out of scope: new wave tuning, the wave-as-job-type idea, and any change to the
  next-wave button's action list.
