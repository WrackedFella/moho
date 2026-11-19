# 0033 — A fired round is sim data, stepped once a tick as a cast segment

**Status:** Accepted; amends [0015](0015-fps-rules-are-a-platform-free-crate.md) §3

## Context

Searchlight shots were hitscan: a ray resolved on the tick of the pull. Rounds that take
time to arrive and fall over distance (FPS-F15, [#754](https://github.com/WrackedFella/moho/issues/754))
need a round that exists between ticks, and enemies (#756) and ricochet (#757) reuse it.

## Decision

1. A round in flight is simulation state in `searchlight`: its `Flight` (position, velocity),
   its `Ballistics` (muzzle velocity, drag), its shooter and where the shooter stood. The
   maths (`Flight::step`, `GravityScale`, `FlightBounds`) lives in `searchlight_rules::ballistics`,
   platform-free, so it gets mutation testing and later shooters reuse it unchanged.
2. A step is `v' = v * max(0, 1 - drag * dt) - Y * gravity * dt`, `p' = p + v' * dt`, using
   only correctly rounded IEEE operations, so a replay on one platform is exact.
3. Each tick every round steps once and the segment `position → new position` is cast through
   the world with the existing shot resolver. A strike ends the round; a part strike lands the
   hit on that tick. Near misses count on the segment, once per round per enemy.
4. Tick order: enemies perceive and move, the player moves, the world steps, lean, rounds
   step, weapon actions (a pull adds a round that first steps next tick), enemies fire, dead
   enemies leave. While the player is dead nothing steps; a restart starts with no rounds.
5. A round ends when it strikes anything, falls to or below `MIN_SPEED`, or leaves the flight
   bounds (the level's mesh box plus a margin).
6. Rounds in flight are not saved.
7. The gravity scale moves rounds only. *Simple* (a tester switch) restores same-tick hitscan
   along a ray as long as the flight bounds' diagonal.

## Consequences

- §3 of 0015 ("every shooter resolves on the tick of the shot") holds for the player only
  until #756; enemy shots stay one ray until then.
- Hits arrive one or more ticks after the pull; tests that read a same-tick hit tick on until
  it lands.
- Cost is one ray per round in flight per tick plus one point-to-segment test per enemy;
  measure once every enemy fires rounds.
