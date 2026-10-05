# Player can spawn inside terrain

**Status:** not started (confirm the trigger first)
**Feature:** SG-F2

## Summary

The player pawn has been seen spawning underground or clipped into terrain.
It then falls through the world to the kill plane, respawns, and repeats
until player input moves it somewhere that works. The only way out otherwise
is mining yourself free. It is not yet known whether this happens on a new
world, on loading a save, or both.

## Deliverables

- A recorded reproduction: steps, and whether it happens on new-world
  spawn, on load, or both (see Verification).
- A failing test for each confirmed path, then the root-cause fix.
- If only the load path reproduces, the card says so and the generate-path
  suspects below are dropped.

## Acceptance criteria

```gherkin
Scenario: The pawn spawns on terrain higher than its spawn height
  Given the terrain height at the spawn column is above the pawn's spawn height
  When the player pawn spawns
  Then the pawn is above the ground
  And the pawn stays on the ground after 10 seconds without input

Scenario: A loaded world has collidable terrain at the player's position
  Given a saved world with terrain under the player's saved position
  When the player loads the save
  Then the pawn stands on that terrain
  And the pawn does not reach the kill plane
```

## Verification

Manual, to settle which path fails. Run each several times, with different
seeds for the first:

1. New world: create a world, and watch whether the pawn lands on the ground
   or sinks and respawns repeatedly. Note the seed.
2. Load: save and quit, relaunch, load that save, and watch the same.
3. Record the result for each, and which one is the "multiple times" case.

## Notes

Headless probes (2026-10, `App::headless` from ENG-F1-08):

- New world, default terrain, 24 seeds: the surface at the origin is 63 for
  every seed. The pawn spawns about 3 above it and never respawns. The first
  scenario did **not** reproduce on this path.
- Load: the voxel grid is rebuilt from saved blocks, but no chunk is meshed,
  so there are no terrain colliders. The pawn falls through to the kill
  plane and respawns repeatedly (3 times in 10 s). The second scenario
  reproduces. The streamer skips columns already in the grid, so they stay
  unmeshed until the player moves to unloaded terrain.
- Whether the load path is the reported bug is unconfirmed. It matches the
  symptoms but not "spawns below the ground", so the trigger may differ.

Original suspects for the generate path, still unverified: spawn placed at
the surface instead of surface plus the character's half height, colliders
not built before `add_character`, and the spawn scan missing overhangs.

Code: `GenerationProcessor::setup_physics_for_world` (generate),
`App::setup_physics_for_loaded_world` and `scene_loader::load_scene` (load),
`FrameProcessor` (collider lifecycle and kill-plane respawn), the character
capsule in `moho_physics`.
