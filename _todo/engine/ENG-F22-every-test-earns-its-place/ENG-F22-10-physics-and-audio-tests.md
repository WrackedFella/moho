# Physics and audio tests fail for wrong implementations

**Feature:** [ENG-F22](_feature.md)
**Issue:** [#177](https://github.com/WrackedFella/moho/issues/177)
**Status:** Backlog
**Gate class:** glue
**Labels:** line:engine

## Summary

The physics tests assert where bodies come to rest, not just an upper bound, and physics teleport and audio caching get the tests their callers rely on. Each test is shown red against the wrong implementation it targets.

## Deliverables

- The tests below exist and each was shown red against its named wrong implementation.
- `moho_audio` gains `tempfile` as a dev-dependency (workspace version).

## Acceptance criteria

| Test | Pins | Red against |
|---|---|---|
| `moho_physics::world` `test_gravity_drops_rigid_body` | after 60 steps of 1/60 s from y=10 under the world's gravity, y is within 0.7..1.2 | gravity -9.81 |
| `moho_physics::world` `test_floor_stops_rigid_body` | the sphere rests with 0.4 < y < 0.7 | floor collider missing |
| `moho_physics::world` `test_character_controller_falls_to_floor` | the capsule centre settles within 1.1..1.3 and `is_grounded` | character tunnels through the floor |
| `moho_physics::world` `set_character_position_teleports_and_stops_falling` | after falling a few steps, `set_character_position(p)` → position `p`, vertical velocity 0 | velocity kept |
| `moho_audio::audio_cache` `cached_audio_survives_file_deletion` | write a temp file, `get_audio` returns its bytes; delete it; `get_audio` returns the same bytes. Same for `get_ui_sound` | cache bypassed |
| `moho_audio::audio_cache` `missing_file_returns_file_not_found` | `get_audio` and `get_ui_sound` on a missing path → `Err(AudioError::FileNotFound(_))` | other error variant |
| `moho_audio::audio_settings` `effective_volume_multiplies_clamped_master_and_category` | `with_master_volume(0.5).with_music_volume(1.5)` → Music 0.5; muted → 0.0 | clamp skipped; mute ignored |

## Tech spec

**Design:** tests only. Audio tests build the cache with `AudioCache::default()`, never `new()` (which reads cwd-relative `assets/`).

**Red proof (applies to every row):** these tests pin behaviour the code already has, so they pass on arrival. Before committing each one, apply the wrong implementation its row names as a temporary local edit, run the test, see it fail, and revert. The PR lists test → wrong implementation → failure line. A row that fails on unchanged code is a defect: stop that row, report it on this issue, and don't change production code beyond the seams named here.

**Out of scope**
- `move_character` with no character: #187 (ENG-F22-20).
- `AudioSource::validate` (no caller): open decision, keep or delete.

**Test map:** the table above.

**Gate class:** glue (`moho_physics` and `moho_audio` are adapters, outside the domain-logic paths).

**Risks:** the physics windows assume rapier's current integrator; if it changes, the tolerance moves, not the test.
