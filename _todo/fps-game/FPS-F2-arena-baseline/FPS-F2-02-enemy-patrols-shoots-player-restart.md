# An enemy patrols, shoots the player, and the player can die and restart

**Feature:** FPS-F2
**Status:** Draft
**Gate class:** glue
**Labels:** line:fps
**Note:** Spec for review. Issue #299 is unchanged until this is approved; the cloud thread cannot set board fields. Depends on FPS-F2-01 (#298, both parts merged): it builds on that card's draft spec, which is not merged yet. The scenarios below restate #299's under the main-HP model (Justin, 2026-10-14); `# Added` ones need Justin's yes.

## Summary

A human enemy walks a waypoint loop, notices the player, turns, and shoots with hitscan. The player now has a body that takes hits, can die, and can restart. The enemy can be killed. First card where the player is in danger.

## Deliverables

- One enemy type (box segments, visual-only gun) spawns at the enemy spawn marker and patrols the waypoint loop.
- It notices the player inside its view cone and range with a clear line, turns to face, and fires with a reaction delay and a cooldown.
- The player has the same hit volumes and health model as a dummy; hits show on screen.
- Killing an enemy removes it. Player death freezes the arena and shows a restart prompt; restart is a fresh start.

## Acceptance criteria

```gherkin
Scenario: An enemy patrols its waypoints while it has not seen the player
  Given an enemy with a patrol route and no line of sight to the player
  When time passes
  Then the enemy moves along its route

Scenario: An enemy shoots the player on sight
  Given an enemy with line of sight to the player
  When time passes
  Then the enemy fires at the player

Scenario: An enemy cannot shoot through arena geometry
  Given an enemy with a wall between it and the player
  When time passes
  Then the enemy does not damage the player

Scenario Outline: Enemy shots drain the player's health and the part's status
  When an enemy shot hits the player's <part>
  Then the player's health drops
  And <status>

  Examples:
    | part     | status                                  |
    | head     | only the head's status HP drops         |
    | torso    | no status HP drops (the torso has none) |
    | left leg | only the left leg's status HP drops     |

Scenario: A head with no status HP left kills the player
  Given a player with plenty of health left
  When enemy shots empty the head's status HP
  Then the player is dead

Scenario: Killing an enemy
  Given an enemy at full health
  When the player empties its head's status HP
  Then the enemy is dead
  And it stops moving and firing

Scenario: Dying offers a restart
  Given the player is dead
  When the player chooses restart
  Then the arena is reset
  And the player is at the spawn with a full rifle loadout

# Added
Scenario: A player with no health left is dead, wherever it was hit
  When enemy shots drain the player's health through the torso
  Then the player is dead

# Added
Scenario: An enemy does not notice a player behind it
  Given an enemy with a clear line to a player outside its view cone
  When time passes
  Then the enemy keeps patrolling and does not fire

# Added
Scenario: An enemy that loses sight of the player resumes its patrol
  Given an enemy firing at the player
  When the player steps behind a wall
  Then the enemy stops firing and patrols again

# Added
Scenario: The arena stands still while the player is dead
  Given the player is dead
  When time passes
  Then enemies neither move nor fire
```

## Tech spec

### Design

All in `moho_fps` ([ADR-0015](../../adr/0015-fps-rules-are-a-platform-free-crate.md) decision 3: enemies reuse the shot path, so no new ADR). `moho_fps_rules` is untouched: the player and enemies use the same `Body`. 01's module names are taken from its draft spec; the implementer reconciles any drift.

- **`brain`** (new, pure: `glam` only, no physics). `Brain::{Patrol { next }, Engage { seen_ticks, cooldown }}`; `think(&mut self, route: &[Vec3], percept: &Percept, tuning: &EnemyTuning, dt) -> Intent`. `Percept { position, yaw, target: Option<Vec3> }`, where `target` is the player's aim point, set only when the sight ray reached a player part. `Intent { forward, yaw_delta, fire }`. Patrol walks to the next waypoint at a speed fraction, turns by the short way at `turn_rate`, advances and wraps on arrival. A target inside the view cone starts `Engage`; an engaged enemy ignores the cone, turns to face, and fires once `reaction_ticks` have passed, it faces within `fire_cone`, and the cooldown is 0. No target goes back to `Patrol` with the route position kept. Card 03 adds cover and chase states here; the seam is the enum, nothing more.
- **`sim`** (01's file). Dummies and enemies become one `Target` record (character, six hit volumes, `Body`); an enemy also holds a `Brain`. The player character gets the same six hit volumes (tag `SurfaceTag::Player(part)`) and a `Body`. One shared `body_volumes()` builds all of them. `Feedback` gains `EnemyFired`, `PlayerHit { part, from }`, `EnemyDied`, `PlayerDied`. Tick order, extending 01's:
  1. Player dead: only `restart` is read; nothing else advances.
  2. Percepts: per enemy, one `resolve_shot`-style ray from its eye to the player's aim point (`EnemyTuning.aim_at`, default torso), range `sight_range`, ignoring itself. `target` is set only if the first surface is a player part, so one ray gives both line of sight and the wall check.
  3. `Brain::think`, then `move_character` for player and enemies, then `PhysicsWorld::step`.
  4. 01's player weapon actions. An enemy the player killed here is removed and does not fire this tick.
  5. Enemy fire, in index order, skipped once the player is dead: aim point plus a spread from a seeded SplitMix64 held in the sim (`aim_error`, `seed`), then `resolve_shot`. A strike on a player part calls `Body::hit(part, damage)`. A strike on another enemy is absorbed. A strike on the world does nothing.
  6. A dead enemy's character is removed from the world (`remove_character` drops its hit volumes), so shots pass through. Player death emits `PlayerDied`.
  Restart: the sim keeps `Arc<Arena>` and its tuning. `restart` while dead rebuilds the whole sim, RNG seed included, through the same constructor as a fresh start. There is one code path for "start", so nothing can be left stale. Waves and score (card 04) will sit above this.
- **`tag`:** adds `SurfaceTag::Player(PartIndex)` and round-trips it. 01's `Part { dummy, .. }` field may be named `target` from the start (optional rename, suggested for 01's draft).
- **`arena`:** the patrol route is every `waypoint_NN` in numeric order as one loop; an enemy starts toward its nearest waypoint. Missing waypoints with an enemy spawn is an `ArenaError`. Per-enemy routes would need marker extras and a generator change; they wait for card 04, which is the first to spawn several enemies.
- **`input`:** `FpsAction::Restart` (Enter), `FpsCommand.restart` on the press edge.
- **`game`/`hud`:** the title status line gains `HP n/max`. Dead: a translucent full-screen rect plus title text "Dead - press Enter to restart". A damage edge bar on the screen side the shot came from (front/right/behind/left from the player's yaw), fading over a few frames; enemy gun segment flashes on `EnemyFired`; same rifle sound for enemy fire. The window title is set through the `Arc<Window>` kept from `Game::init`, since `FrameContext` carries no window (01's design needs the same).

Tuning is data, not literals. Defaults: `sight_range` 30 m, view cone 60° half-angle, `reaction_ticks` 30, `cooldown_ticks` 12, damage 15, `aim_error` 0.03 rad, `turn_rate` 3 rad/s, patrol speed 0.5 of max, arrive radius 0.5 m, player profile as 01's. Tests set their own values and never read the defaults.

### Out of scope

- Cover, chase, taking cover when shot, searching (card 03). Waves, score, spawning more than the arena's enemies (card 04).
- Enemy ammo, magazines, reloads, jams: enemies fire on a cooldown, unlimited.
- Navigation, stuck detection, per-enemy routes, hearing, enemy-to-enemy awareness, friendly fire.
- Tracers, positional audio, enemy art, animation, death effects or corpses, text menus, a health bar, bleed.
- Edits to `moho_fps_rules`, `moho_physics`, `moho_level`, `moho_app`, `moho_input`, `moho_ui`, `Cargo.toml` members or `scripts/layering.txt`. The arena asset is only read; a missing marker is reported, not patched here.

### Footprint

Part A (pure brain):
- `moho_fps/src/brain.rs`, `src/lib.rs` (module line): `Brain`, `Percept`, `Intent`, `EnemyTuning`, and their tests

Part B (integration; compiles against A's signatures above, so run after A unless they are stubbed):
- `moho_fps/src/{sim,shot,tag,arena,input,game,hud}.rs`: enemies, player body, fire, death, restart, feedback, and tests

### Test map

Part A, `brain` (glue; tests with code):

| Scenario | Test |
|---|---|
| 1 patrol | `brain::tests::an_unseen_enemy_walks_toward_its_next_waypoint` |
| 1 loop | `brain::tests::arriving_advances_the_route_and_wraps` |
| edge | `brain::tests::turning_takes_the_short_way_round` (proptest: wrap, bounded by turn rate) |
| 2 sight | `brain::tests::an_enemy_waits_out_its_reaction_time_then_fires_on_cooldown` |
| 2 aim | `brain::tests::an_enemy_does_not_fire_until_it_faces_the_player` |
| 9 cone | `brain::tests::a_target_outside_the_view_cone_is_not_noticed` (both sides of the edge) |
| edge | `brain::tests::an_engaged_enemy_keeps_a_target_outside_the_cone` |
| 10 lost | `brain::tests::losing_sight_returns_to_patrol_and_keeps_the_route_position` |

Part B, `moho_fps` (glue):

| Scenario | Test |
|---|---|
| 1 e2e | `sim::tests::an_enemy_walks_a_full_lap_of_the_shipped_arena` |
| edge | `arena::tests::the_shipped_arena_has_an_enemy_spawn_and_a_waypoint_loop`, `arena::tests::an_enemy_spawn_without_waypoints_is_an_error_naming_it` |
| 2 e2e | `sim::tests::an_enemy_with_sight_fires_at_the_player` |
| 3 wall | `sim::tests::an_enemy_behind_a_wall_does_not_damage_the_player` |
| shared path | `shot::tests::a_shot_at_the_player_reports_the_part` (head, torso, left leg), `tag::tests::surface_tags_round_trip` (now with `Player`) |
| 4 hit | `sim::tests::an_enemy_hit_drains_the_players_health_and_the_struck_part` (head, torso, left leg, via `aim_at`) |
| 5 head | `sim::tests::enemy_shots_to_the_head_kill_a_player_with_health_left` |
| 8 health | `sim::tests::enemy_shots_to_the_torso_kill_when_health_is_gone` |
| edge | `sim::tests::spread_is_deterministic_for_a_seed` |
| edge | `sim::tests::a_shot_that_clips_another_enemy_is_absorbed` |
| 6 kill | `sim::tests::a_killed_enemy_stops_moving_and_firing`, `sim::tests::shots_pass_through_where_a_dead_enemy_stood` |
| edge | `sim::tests::an_enemy_killed_this_tick_does_not_fire` |
| 9 cone e2e | `sim::tests::an_enemy_does_not_fire_at_a_player_behind_it` |
| 10 lost e2e | `sim::tests::an_enemy_stops_firing_when_the_player_ducks_behind_a_wall` |
| 11 frozen | `sim::tests::a_dead_player_freezes_the_arena`, `sim::tests::no_enemy_fires_once_the_player_is_dead` |
| 7 restart | `sim::tests::restart_after_death_matches_a_fresh_start`, `sim::tests::restart_while_alive_does_nothing` |
| input | `input::tests::enter_is_bound_to_restart` |
| feedback | `hud::tests::the_hit_edge_points_toward_the_shooter` (front, right, behind, left), `hud::tests::the_status_line_shows_health`, `hud::tests::the_death_overlay_shows_only_when_dead` |

### Gate class

`glue`: all code is in `moho_fps`, which is not in the domain-logic paths. The brain's tests are written first anyway and `just mutants` covers it. If Justin wants the brain attacked by `devflow:test-critic` before implementation, Part A would need `moho_fps/src/brain.rs` added to CLAUDE.md's domain paths; the recommendation is no.

### Risks

- Straight-line patrol with no navmesh: an enemy can get stuck on a corner. The full-lap test on the shipped arena is the check; if it fails, fix the waypoint placement in the arena generator in a follow-up, not a nav system.
- The shipped arena must hold an enemy spawn and a waypoint loop. 01's draft promises both; if 01 ships fewer, this card's arena test fails and the generator gets the markers.
- Tuning constants are mutation targets. Each default is data; boundary tests (cone edge, reaction tick, cooldown, arrive radius) pin them.
- Enemy and player capsules block each other, so an enemy can be stopped by the player standing on its route.
- `sim.rs` is shared with 01, so this card runs after it. Per-tick cost is one ray per enemy plus one per shot.
- Readability of enemy fire (playtest) rests on the edge bar, facing and gun flash; positional audio is the real fix and is deferred.
