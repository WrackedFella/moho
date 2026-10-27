# FPS-F15 — A fired round flies, falls and bounces before it hits

**Status:** Draft
**Labels:** feature, line:fps

## End state
Every round in Searchlight is a simulated object: it flies, drops, slows, and hits for what it has left, and the realism stacks in four toggled tiers up to breath and heart rate ([GDD](https://github.com/WrackedFella/moho/blob/dev/wiki/searchlight/game-design-document.md) §4.14, §9 Q7, Q8). This feature is tier 1, *Flight*: FPS-M1.3 ([roadmap](https://github.com/WrackedFella/moho/blob/dev/wiki/searchlight/roadmap.md) §5.1).

## Summary
The player's and the enemies' rounds stop arriving on the tick they are fired. A round takes time to reach its target, falls under gravity, loses speed to drag, hits for less the slower it arrives, and can bounce once off a world surface hit at a glancing angle. Enemies lead a moving player, more accurately the higher their grade. A *Simple* setting restores today's hitscan for A/B playtests, and a gravity scale makes drop readable at arena range.
**Moves toward the end state by:** fixing the round's data shape (muzzle velocity, drag, damage) before FPS-M2 fixes its item data files, and giving tier 2 (materials, penetration, FPS-M3) a flying round to extend.

## Exit criteria
- A round reaches a target after a flight time that grows with distance and shrinks with its muzzle velocity; nothing is hit on the tick it is fired.
- A round aimed level at a far target lands lower than the aim point, and lower still with the gravity scale raised.
- The same round hits a far target for less than a near one.
- A round hitting a wall at a glancing angle bounces once and can strike a target beyond; a square hit stops it.
- An enemy with no aim error hits a player running across its view at a steady speed; lower grades miss a moving player more often.
- *Simple* restores hitscan for every shooter: same-tick hits, no drop, no falloff, no bounce, no lead.
- A replay with the same seed and commands reproduces every impact, its tick and its point.
- Justin playtests Flight against Simple in the arena (gravity scale as needed, the proving ground if FPS-F10 has landed) and answers the milestone question: does a round in flight make distance a decision?

## Scope
- In: flying rounds for the player and enemies; muzzle velocity, drag and damage on the round's definition; damage by arrival speed; one ricochet off world surfaces; enemy lead with a grade dial; near misses counted where the round actually passes; the Simple setting and the gravity scale as tester controls; a replay test.
- Out: penetration and surface materials (tier 2), wind and zeroing (tier 3), breath and heart rate (tier 4); new weapons or rounds (FPS-M2, FPS-M3); a menu row for Simple (difficulty design, §9 Q8); a drawn tracer from the engine (ENG-F40 #636).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does damage live? | On the round. The weapon's damage field goes; the round carries damage, muzzle velocity and drag | GDD §4.2 round data. Leaving it on the weapon means a mixed magazine can't fire rounds that hit differently, and FPS-M2's data would be reworked |
| Can a weapon change a round's flight? | No, not in this feature. Every weapon fires a round at the round's own muzzle velocity | Barrel modifiers need more than one weapon; recorded as deferred. Design must not preclude a later per-weapon modifier |
| What ends a round's flight? | A hit that stops it, falling below a minimum speed, or leaving the level. The weapon's range field goes | A fixed range cuts a round off mid-air and fights drop; long maps (§4.13) need rounds that fly as far as their speed allows |
| How does damage scale with speed? | In proportion: full damage at muzzle velocity, half at half speed (X-Ray's rule) | One readable rule, no falloff table. Body-part multipliers still apply after it |
| What can a bounced round hit? | Anything a fresh round can, the shooter included, with the speed it kept | Cheapest consistent rule; a self-hit from a bounce is a fair hardcore outcome. Bounces off bodies are cut (GDD §4.14) |
| Where does Simple apply? | The whole sim, as a host setting read from slot 0 like god mode; tester control now, menu row later | One sim, one rule set per session; per-player realism would break host-resolved hits in co-op |
| What does the gravity scale touch? | Rounds only, never characters or loose objects; tester control, default 1 | A world gravity change would change movement and FPS-F9 objects |
| Determinism | Ricochet dice draw from their own seeded stream | Adding dice must not change enemy spread in existing replays |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Penetration, surface materials, blunt damage | Tier 2 | FPS-M3 |
| Wind, per-round drag curves, scope zeroing | Tier 3; needs a scope | FPS-M3.1 |
| Breath and heart rate sway | Tier 4; needs fatigue | FPS-M5 (§9 Q32) |
| Per-weapon muzzle velocity (barrel length) | One weapon today | FPS-M2 adds a second weapon on the same caliber |
| Simple as a menu or difficulty row | Difficulty toggles are not designed | §9 Q8, before Phase 5 |
| A bullet-speed tester knob | Muzzle velocity is content data and can be edited | A playtest asks for live tuning |
| Ricochet sound by surface | No materials yet; positional audio is ENG-F16 #228 | Tier 2 |
| Pellets | No shotgun yet; a pellet is a round when it comes | FPS-M3 tube-fed shotgun |
| Rewinding shots for co-op | Netcode ADR | after Phase 5 (§9 Q2) |

## Items
| Item | Behavior | Depends on |
|---|---|---|
| FPS-F15-01 | The player's rounds fly: travel time, drop, drag, hits on arrival, near misses where they pass, gravity scale, Simple restores hitscan, replay matches | none |
| FPS-F15-02 | A round hits for less the slower it arrives; damage moves to the round | 01 |
| FPS-F15-03 | Enemies fire flying rounds and lead the player by grade | 01 |
| FPS-F15-04 | A round bounces once off a glancing world surface | 01 |
| FPS-F15-05 (pending decision) | Rounds in flight are drawn with a stopgap until the engine's tracer exists | 01 |

02, 03 and 04 are independent of each other.

## Notes
- Must land before FPS-M2 fixes its round data files (roadmap §5.1). Runs beside FPS-M2.
- Engine need, not designed here: a tracer the game moves each frame from a position it owns (ENG-F40 #636 slice 1 spawns fixed-lifetime effects). Added to the engine-requests running list.
- TL: where the flight maths lives (platform-free rules crate, mutation-gated, with the ray cast injected, or beside the store).
- TL: whether the ricochet cone can be built without trig in the tick loop, so replays match across platforms.
- TL: hit volumes must be current when rounds step after movement; check the order holds for every shooter.
- TL: arena sightlines are short; at 900 m/s travel time is 2 to 4 ticks. Confirm the tick cost with every enemy firing.
