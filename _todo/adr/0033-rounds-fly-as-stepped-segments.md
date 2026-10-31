# 0033 — A fired round is sim data stepped once a tick as a cast segment

**Status:** Accepted (Justin, 2026-10-31); lands with the FPS-F15-01 PR, #754

## Context

GDD §4.14 makes every round a simulated object: it flies, falls, slows and later bounces
(FPS-F15, #745) and penetrates (tier 2). [ADR-0015](0015-fps-rules-are-a-platform-free-crate.md)
resolves a shot as one ray on the tick it is fired, and every shooter shares that path. A
round in flight lasts several ticks, must replay exactly ([ADR-0011](0011-simulation-stays-network-ready.md)),
and stays host-owned for co-op (§9 Q2). STALKER X-Ray steps bullets the same way: one ray per
step from the last position to the next.

## Decision

1. **A round in flight is plain sim data:** position, velocity, the round's ballistics, its
   shooter, where the shooter stood when firing, and the enemies it has already near-missed.
   The sim owns the list; rounds step in the order they were fired.
2. **Flight maths is a rule; the cast is the adapter's.** `searchlight_rules` integrates one
   step (semi-implicit Euler, linear drag, gravity) with correctly rounded IEEE-754 operations
   only (`+ - * /`, `sqrt`, `max`): no `exp`, trigonometry or other platform maths, so the step
   adds nothing that differs between platforms. Flight state is plain, serialisable data.
   `searchlight` casts the step's segment through the ADR-0015 path (ray, surface tag, part,
   rule call), ignoring the round's shooter; the first strike ends the step.
3. **A round first steps on the tick after it is fired.** Nothing is hit on the firing tick,
   at any range.
4. **Rounds step after every character has moved and the world has stepped, before any shot of
   this tick is fired.** A step reads this tick's hit volumes.
5. **Round gravity is its own:** standard gravity (9.81 m/s²) times a host-level gravity scale.
   World gravity stays the game's movement tuning and never changes with the scale.
6. **A flight ends** on a strike, at or below a minimum speed, or outside the level's flight
   bounds. A round whose muzzle velocity is not above the minimum speed is refused at load.
   Until levels author a boundary (GDD §5), the bounds are the level geometry's box grown by a
   fixed margin.
7. **Simple is a host level** read from slot 0, like god mode. Under it a shot is today's one
   ray, cast to the edge of the flight bounds; nothing is put in flight.

ADR-0015 §3 now reads: one shot path, stepped. Each step of a round resolves as segment,
surface tag, part, rule call, for every shooter. Until FPS-F15-03 (#756) enemy shots stay one
ray on the firing tick; from then on Simple is the only hitscan, for every shooter.

## Consequences

- Enemy rounds (FPS-F15-03) join the same list and step; nothing new in the order.
- A ricochet (FPS-F15-04) ends a step at the strike and gives the round a new velocity; the
  mirror `v - 2(v·n)n` needs no trigonometry, and its dice get their own stream.
- A tracer (FPS-F15-05) reads the rounds' positions after the tick; the sim exposes them then.
- Penetration (tier 2) continues a step past a strike instead of ending it.
- Point-blank shots land one tick (16.7 ms) later than hitscan did.
- Rounds in flight are not saved; a save (Phase 6) taken mid-flight drops them.
- Replays stay exact on one platform. Across platforms they also depend on the aim's `sin`
  and `cos` and on rapier's casts, as hitscan shots already do; this ADR adds no new source.
- Cost: each round costs one ray a tick while it flies: 2 to 4 ticks to a target in the arena,
  7 to 10 for a miss flying out to the bounds.
