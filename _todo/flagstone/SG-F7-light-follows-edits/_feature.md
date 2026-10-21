# SG-F7 — Light follows what I dig and build

**Status:** Draft
**Labels:** feature, line:strategy
**Note:** Drafted from the ENG-F9-10 findings (#451 comment). M2 label to be decided with the user.

## End state

The voxel world's block light and sky light always match its blocks: digging, building and loading a world relight the affected chunks within a frame budget. See the [vision](../../../wiki/flagstone/vision.md).

## Summary

The lighting rules exist and are tested (ENG-F9-10), but the game never runs them: no world event reaches `LightSystem`, nothing fills light on world load, and sky exposure is never recomputed, so meshes read stale light. Two removal rules are also wrong. This feature fixes the rules, then wires them into flagstone's loop.
**Moves toward the end state by:** emissive blocks (torches, lava, glowing ore) and dark caves become possible without more engine work.

## Exit criteria

```gherkin
Scenario: Digging a shaft lets sky light down
  Given a world with solid ground
  When the player digs a vertical shaft
  Then the shaft's floor is sky-exposed once the light system has processed the edit

Scenario: Sealing a shaft takes the sky light away
  Given an open shaft
  When the player places an opaque block over it
  Then the voxels below are no longer sky-exposed

Scenario: Removing one light never darkens another light's area
  Given two lights whose areas overlap
  When one is removed
  Then every voxel has the level the remaining light alone gives it

Scenario: Opening a wall lets neighbouring light in
  Given a lit room beside a dark one, separated by one opaque block
  When the block is removed
  Then the dark room is lit as if the wall had never been there
```

## Scope

- In: block-light removal correctness, light refill when an opaque block goes, routing world edits into the light system, sky exposure recompute for edited and newly loaded chunks, block-light fill on world load and chunk generation.
- Out: new emissive materials, light colour model, falloff rules, threading the job queue.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Does the game get emissive blocks (torches)? | Open, game design | Without one, only sky light is visible in game; the block-light cards still fix real rules |
| Do edits relight synchronously or through the budgeted job queue? | Job queue (exists) | Synchronous relight of a large edit stalls a frame |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Emissive materials (torch block, glowing ore) | Game design, Strategy project | When the user wants light sources |
| `LightUpdateJob` `Ord`/`Eq` agree on ties | Harmless for `BinaryHeap` | If jobs are ever compared for equality |

## Items

| Item |
|---|
| SG-F7-01 Removing a light leaves every other light's area lit |
| SG-F7-02 Opening a wall lets neighbouring light in |
| SG-F7-03 Digging and building relight the world in game |

## Notes

- 01 and 02 both edit `light_propagation.rs` removal: 02 after 01.
- 03 depends on neither for wiring, but its in-game check is only right after 01 and 02.
