# Player can spawn inside terrain

**Status:** not started
**Feature:** [SG-F2](_feature.md)

## Summary

On fresh world gen, the player occasionally spawns underground or clipped
into terrain. Currently the only way out is mining yourself free.

## Deliverables

- Root-cause fix — leading suspects: spawn placed at `surface_y` instead of
  `surface_y + character_half_height`; colliders not fully built before
  `add_character` runs; 9×9 spawn-scan missing multi-block overhangs.

## Notes

Files: `src/main.rs` (`setup_physics_for_world`, `add_character`),
`frame_processor.rs` (chunk collider lifecycle), `moho_physics/src/lib.rs`
(character capsule).
