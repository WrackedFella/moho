# Character movement tests run on a stepped world and fail when the character leaves the floor

**Feature:** [ENG-F22](_feature.md)
**Status:** Draft
**Gate class:** glue
**Labels:** line:engine

## Summary

`test_character_horizontal_movement` never steps the physics world, so the broad phase never learns the floor and the capsule falls to y ≈ -19.5 while "moving"; it passes only because it checks x alone. It and the new character tests step after each move, as the game loop does, and pin the rest height, so a character that tunnels through the floor fails them. The same tests kill the `moho_physics` mutants #177 (ENG-F22-10) left alive.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- Every `moho_physics::world` test that calls `move_character` with a world collider present steps the world after each move.

## Acceptance criteria

All tests are in `moho_physics::world`, on a floor at y = 0 unless the row says otherwise, with each frame being `move_character(…, 1/60)` then `step(1/60)`.

| Test | Pins | Red against |
|---|---|---|
| `test_character_horizontal_movement` (rewritten) | after 60 still frames, 30 frames of +X at 4 m/s move the capsule 2.0 ± 0.05 m in x, keep its centre within 1.1..1.3 and `is_grounded` every frame | `move_character` ignores world colliders (the old non-stepping loop); horizontal input halved |
| `landing_character_stops_vertical_velocity` | spawned at y = 3, after 60 frames `is_grounded` and vertical velocity is exactly 0 | grounded velocity reset deleted |
| `airborne_character_falls_by_integrated_gravity` | no floor, spawned at y = 10, after 10 frames y = 10 − 18·(1/60)²·55 = 9.725 ± 1e-4 | `vertical_velocity * dt` → `/ dt` or `+ dt` |
| `dynamic_cuboid_rests_on_its_half_height` | `add_dynamic_cuboid` at y = 3 with half-extents (0.5, 0.25, 0.5), after 180 steps its body y is within 0.2..0.3 | half-extents passed in the wrong order |

## Tech spec

**Design:** tests only, in the `moho_physics::world` test module; no production change. Reuse the module's `floor_mesh` helper (its 10 m half-width is enough for a 2 m walk) and drop the hand-built quad in the horizontal test.

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. For the rewritten row, also show the old non-stepping loop fails the new assertions (the capsule ends near y = -19.5). A row that fails on unchanged code is a defect: stop that row and report it on this issue.

Values were checked on `dev` at refinement: the capsule rests at y ≈ 1.173 and moves 1.9996 m; the cuboid rests at y ≈ 0.250; every named wrong implementation fails its row. Deleting the grounded reset also fails the horizontal row (the capsule drags at 0.2 m), which is fine.

**Other tests checked for the same flaw:** `test_character_controller_falls_to_floor` already steps (fixed by #177). `set_character_position_teleports_and_stops_falling` moves without stepping but has no collider, so the broad phase has nothing to miss; it stays as is. No binary or `moho_game` test calls `move_character`.

**Out of scope**
- `move_character` with no character: #187 (ENG-F22-20).
- `moho_audio` survivors from #177's mutation run (`AudioSettings` SFX, UI and voice setters): not physics; a sibling card if wanted.

**Test map:** the table above.

**Gate class:** glue (`moho_physics` is an adapter, outside the domain-logic paths).

**Risks:** the windows assume rapier's current integrator and the controller's snap and offset settings; if those change, the tolerance moves, not the test.

## Notes

- Same test module as #187 (ENG-F22-20), which makes `move_character` return `Option`. These tests ignore the return value, so either order works; whichever lands second rebases.
