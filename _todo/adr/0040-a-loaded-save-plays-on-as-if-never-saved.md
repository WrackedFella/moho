# 0040 — A loaded save plays on as if it had never been saved

**Status:** Accepted (2026-11-24); extends [0011](0011-simulation-stays-network-ready.md) and [0016](0016-game-lines-own-their-save-kinds.md)

## Context

[ADR-0011](0011-simulation-stays-network-ready.md) rule 2 makes simulation state plain and
serialisable, but nothing requires a save to hold all of it. [ADR-0021](0021-game-ai-lives-in-the-lines.md)
rule 7 says AI state saves with the world; no rule covers the rest. The strategy line asks
for one ([#884](https://github.com/WrackedFella/moho/issues/884)): load a save, play on, and
get the same result as never having saved.

Some simulation state is owned by the engine, outside any line's types:

- `moho_physics` characters: position, velocity, grounded, held jump, the stance leg in
  progress, lean, and effects queued for the next step ([ADR-0022](0022-character-changes-are-tick-input.md)).
- `moho_physics` loose objects: pose, velocities, and the id sequence.
- `moho_app`'s loop: the tick index and `GameClock`. The loop always starts at tick 0.

`SimRng` streams are already held by value in the drawing system's state and round-trip
through postcard.

A probe on 2026-11-24 (rapier 0.36 with `enhanced-determinism`, one binary, 600 ticks, save
at ticks 20, 90, 200 and 450) measured how far a restored world drifts from one that never
saved:

| Restore | Characters | Dynamic bodies |
|---|---|---|
| Every rapier set cloned (a full rapier snapshot) | bit-equal | bit-equal |
| Records, geometry rebuilt, query structures refreshed before the first tick | bit-equal | differ from the first tick: up to 3 mm in a settling pile; a falling pile is chaotic (3.3 m in one run) |
| Records, no refresh | up to 12 cm | — |
| Position and velocity only (`SetVelocity`) | up to 12 cm | — |

Rebuilt bodies differ because rapier keeps solver state that is in no record: contact
warm-start impulses, island and sleep bookkeeping, and arena order. A rapier snapshot avoids
that but has costs. Its bytes are rapier's internal layout, so they change at each rapier
upgrade, which breaks the [ADR-0006](0006-save-format-contract.md) promise to read N−1 from
v1.0. It also stores the static world-mesh colliders, which are derived state, and it ties
saves to rapier, while [ADR-0027](0027-characters-slide-on-our-own-collide-and-slide.md)
keeps a backend change open.

## Decision

1. **Transparency.** A save loaded at tick k and run on produces, at every later tick, the
   same simulation state as a run that never saved, on the same build and machine. Dynamic
   rigid bodies are the one exception (rule 6).
2. **What is saved.** Everything the tick reads or writes is saved. That includes pending
   orders, queued character effects, `SimRng` streams, id sequences, the tick index and the
   clock. Paused commands are already applied when the save is taken
   ([ADR-0018](0018-paused-commands-apply-at-the-tick-boundary.md)). Derived state (meshes,
   colliders, navmesh, routes, caches, lighting) is rebuilt on load from saved state alone.
   Each field that is skipped is marked as derived where it is skipped. Presentation is never
   simulation state ([ADR-0011](0011-simulation-stays-network-ready.md) rule 4). A line may
   keep some presentation outside the state root, as a preference.
3. **One state root per line.** Each line has one serialisable value that holds all of its
   simulation state and is written whole into its world save, under a kind with pinned bytes
   ([ADR-0016](0016-game-lines-own-their-save-kinds.md)). Engine-owned state enters that root
   as engine records (rule 4). No simulation state is saved by any other path.
4. **Engine-owned state is saved as engine records.** Each engine store that holds
   simulation state hands it out as plain serde records keyed by stable ids. It restores
   itself from those records plus the derived inputs the line supplies again, such as world
   meshes, hit volumes and settings. A restored store is ready for its first tick: it
   rebuilds its query structures, and the restore does not count as a tick. Engine handles
   are never saved; the line keys a record by its own id. A change to a record's shape fails
   each line's pinned-bytes test, so the line bumps its version.

   | State | Owner | Saved as |
   |---|---|---|
   | Characters: pose, velocity, grounded, held jump, stance leg, lean, queued effects | `moho_physics` | one character record each, keyed by the line's id |
   | Loose objects: id, spawn parameters, pose, velocities; the next id | `moho_physics` | loose records in id order, plus the next id |
   | rapier solver state: contacts, warm start, islands, broad phase | rapier | not saved (rule 6) |
   | World-mesh colliders, navmesh | `moho_physics`, `moho_level` | not saved: derived from the line's geometry |
   | `SimRng` streams | the system that draws from them | by value inside the line's root |
   | Tick index and `GameClock` | `moho_app` loop | a loop record, restored before the first tick |
   | Pause, speed, sub-tick remainder | `moho_app` loop | not saved: loop state ([ADR-0018](0018-paused-commands-apply-at-the-tick-boundary.md)) |
   | Effects, audio, camera, UI | presentation | not saved |

5. **Saves are taken at a tick boundary**: after a tick and any paused commands at that
   boundary, before the next tick. Autosave follows the same rule.
6. **Dynamic rigid bodies resume; they do not replay.** At the load tick, a restored dynamic
   body has exactly its saved pose and velocities. After that, its motion is not promised to
   be bit-equal, because rapier's solver state is not saved. Characters are kinematic and run
   on our own slide ([ADR-0027](0027-characters-slide-on-our-own-collide-and-slide.md)), so
   rule 1 covers them. State under rule 1 that a dynamic body touches, such as a character
   blocked by a loose object, is exact only until the first such contact. Exact bodies wait
   for a consumer: lockstep join-in-progress from the netcode ADR, or a replay that starts from
   a save. That consumer brings a new ADR, which weighs a rapier snapshot against the costs
   above.
7. **Guards.**
   - Pinned bytes for each line's root and for each engine record ([ADR-0016](0016-game-lines-own-their-save-kinds.md)).
   - A save-transparency test per line: run N ticks, save and load at tick k, and compare
     every later tick with a run that never saved. A card that adds simulation state extends
     the test's scenario. The engine tests its own records the same way.
   - A review rule: a change that adds simulation state puts it in the root or in an engine
     record, or names it as derived and says what rebuilds it. The design critic cites this
     rule, and the PR template asks about it.
   - Each line's harness stays in that line until a second line saves mid-play or ENG-M3 row
     3.5 (the two-sim proof) needs one. The driver then moves to `moho_app`, beside
     `HeadlessLoop`, together with the `Game` state-hash seam that ENG-F45 deferred.

## Consequences

- ADR-0011 stands. This ADR turns its rule 2 from "can be serialised" into "is saved".
  ADR-0021 rule 7 becomes one case of rule 2.
- The engine owes records and restores for characters, loose objects and the loop
  (ENG-F50). Until then, a line can bring a character back only by spawning it and queueing
  `SetVelocity`, which drifted by up to 12 cm in the probe.
- The strategy line adopts the rule in SG-F4-06 (#440). The FPS line adopts it with its
  first mid-play save; its loose objects (magazines) fall under rule 6.
- Transparency is promised on one build and machine (L0). A save loaded on another build or
  OS plays on correctly but is not promised to be bit-equal; ENG-F45 measures the
  cross-platform level.
- Cost: every new store needs a record, a pinned fixture and a line in the test scenario.
  Loads re-derive geometry, colliders and navmesh, which takes load time.
