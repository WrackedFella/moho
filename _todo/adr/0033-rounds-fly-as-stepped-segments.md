# 0033 — A fired round is sim data, stepped once a tick as a cast segment

**Status:** Accepted; amends [0015](0015-fps-rules-are-a-platform-free-crate.md) §3

## Context

[0015](0015-fps-rules-are-a-platform-free-crate.md) §3 resolves every shot through one path
and answers it on the tick it is fired (hitscan). Rounds with muzzle velocity, drag and
gravity (FPS-F15) need time to arrive, so a shot is no longer a same-tick answer.

## Decision

1. A fired round is a `RoundInFlight` in the sim: position, velocity, its `Ballistics`, who
   fired it and where they stood. Rounds in flight are sim state and are cleared by a
   restart; they are not saved.
2. `searchlight_rules::ballistics` holds the integrator, platform-free:
   `v' = v * max(0, 1 - drag * dt) - Y * gravity * dt`, `p' = p + v' * dt`. It uses only
   correctly rounded IEEE operations, so a replay is exact on one platform.
3. Each tick a round steps once and its segment `position → new position` is cast through
   the same resolver every shot uses (`resolve_shot`). A strike, a speed at or below
   `MIN_SPEED`, or a position outside the flight bounds ends the round.
4. Tick order: rounds in flight step after the world and lean, before weapon actions, so a
   round fired this tick first moves on the next. A near miss counts on the segment the round
   flew, once per round per enemy.
5. The gravity scale applies to rounds only, never the world. *Simple* brings back hitscan
   for the player's shots.
6. Enemy shots stay hitscan until the enemy-lead work; §3's "every shooter" holds only for
   the player until then.

## Consequences

- Cost is one ray a tick per round in flight plus one point-to-segment test per enemy.
- Enemies and ricochet reuse the integrator unchanged.
- Across platforms replays still depend on `first_person_view`'s `sin`/`cos` and rapier's
  casts, as hitscan did.
