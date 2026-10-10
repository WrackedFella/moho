# ENG-F15 — A ray finds the surface or body part it hits

**Status:** Draft (revision of #227, which was filed from an unapproved draft; on approval this text and title replace the issue's)
**Labels:** feature, line:engine

## End state

Games ask physics what a line through the world hits (shots, line of sight, aiming)
and get back the world surface or the body part of the character it struck, for any
number of characters ([FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §4.7,
§10 item 2).

## Summary

Physics today runs one character and has no ray query. When this ships, a game runs
several characters keyed by handle, attaches named hit volumes to each, and casts rays
that report the first thing hit: point, normal, distance and the game's own tag.
Consumers: P.2 hitscan on body parts and AI line of sight (GDD §7 Phase 1, §9 Q7);
the strategy game moves its one player onto a handle.
**Moves toward the end state by:** fixing the physics-query seam (ADR-0012, seam 4) and
the character handles ENG-F21's controller builds on.

## Exit criteria

- At least two characters run at once, each keyed by its own handle (test).
- A ray query returns hit point, normal, distance and a game-supplied tag, and can
  ignore one character (test).
- Hit volumes follow their character each tick; a ray at the head hits the "head"
  volume (test).
- The strategy game's player behaves as before (existing tests; manual check).

## Scope

- In: many characters by handle, ray queries against world meshes and hit volumes,
  hit volumes fixed relative to a character's position and facing.
- Out: projectiles and ballistics; shape casts and overlap queries; volumes that follow
  an animated pose; rigid-body dynamics changes.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| What do rays see on a character? | Its hit volumes only, never its movement capsule | A capsule hit has no body part; shots and line of sight both want parts |
| Do hit volumes block movement? | No, they are query-only | Movement stays on the capsule; volumes can overlap it freely |
| Who gives hits a meaning? | The game: it tags volumes and world meshes; the engine returns the tag | Body parts are FPS rules (GDD §4.7) |
| How do volumes move? | Fixed offset from the character's position and facing, updated each tick | Pose-driven volumes need animation, which doesn't exist |
| Projectiles | Out; hitscan only (GDD §9 Q7) | Physics projectiles are a long-term FPS goal |
| Can a later weapon hit several things along one ray (penetration)? | Must stay possible: one hit's data (point, normal, distance, tag) is shaped so a later "every hit along the ray, nearest first" query returns a list of the same thing | Penetration would otherwise change the seam ENG-F5 freezes; adding a second query is additive |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| "Run many bodies" beyond characters (old title) | No consumer needs more dynamic bodies | A game spawns many props |
| Shape casts and overlap queries (grenades, melee) | No Phase 1 weapon needs them | FPS adds grenades |
| Volumes on an animated pose | No animation yet | Skeletal or segmented animation lands |
| Penetration through walls and targets (all hits along a ray) | Ballistics depth, GDD "Later"; the first-hit query keeps it open | FPS asks |

## Items

| Item |
|---|
| ENG-F15-01 Several characters run at once, each by its own handle |
| ENG-F15-02 A ray returns the first surface it hits with the game's tag |
| ENG-F15-03 Hit volumes follow a character and rays report the part hit |

## Notes

- Order: 01 → 02 → 03 (one physics module; 03 needs both). ENG-F21-01 needs 01.
- ADR-0012: drop `moho_physics`'s unused `moho_core` dependency in the first card that touches the crate.
