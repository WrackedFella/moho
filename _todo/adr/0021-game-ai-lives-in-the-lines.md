# 0021 — Game AI lives in the lines; the engine answers queries

**Status:** Proposed (applies [0011](0011-simulation-stays-network-ready.md) to AI)

## Context

Both lines start real AI in M2: the FPS enemy brain (an enum state machine in
`searchlight`, percept in, intent out) and the strategy pawn job loop
([SG-F4](https://github.com/WrackedFella/moho/issues/310)). The FPS design adds a
world-level planner, the Director, in its last phase. The two lines want different
AI: tactical combat agents in the FPS, job scheduling plus routing in strategy. No
framework has two consumers.

What both lines must share is how AI touches the simulation. An AI that reads the
rendered image, the audio mix or an unseeded generator, or that changes state
outside the tick, breaks [ADR-0011](0011-simulation-stays-network-ready.md): replays
diverge, a headless sim behaves differently from a windowed one, and co-op
multiplayer can't keep peers in step.

## Decision

1. **Decision logic belongs to its line.** State machines, behaviour trees, utility
   scoring, planners and the Director live in the game line that uses them. The
   engine ships no AI framework.
2. **The engine answers queries.** It offers navigation
   ([ADR-0019](0019-navigation-is-an-engine-seam.md)), filtered physics
   rays, and the sim-side light query when [ENG-F35](https://github.com/WrackedFella/moho/issues/509) lands. A query is engine-owned
   when both lines need it or when its definition must match an engine system (the
   light model must match the renderer's).
3. **AI runs inside the fixed tick.** Each tick it reads simulation state and
   produces its actions within that tick ([ADR-0009](0009-simulation-time-is-one-fixed-tick.md)).
   It keeps no state outside the simulation's typed stores
   ([ADR-0004](0004-entity-storage-without-a-general-ecs.md)).
4. **AI acts through the player's commands.** An AI-driven actor issues the same
   command type a player-driven one does (in the FPS, a `CharacterCommand` and a
   shot through the one shot path of [ADR-0015](0015-fps-rules-are-a-platform-free-crate.md);
   in strategy, the pawn actions a possessed pawn uses). The simulation applies both
   through one path and grants AI no extra verbs.
5. **AI perceives only through the simulation.** Its inputs are built from sim
   sources: physics rays, noise or stimulus events the simulation emits, the strategy
   voxel light grid, and the ENG-F35 light query. It never reads the renderer, GI,
   shadow maps, `moho_audio` output, UI state or the frame clock.
6. **AI randomness is seeded.** Every random choice draws from a named
   `moho_core::rng::SimRng` stream held in simulation state, never from a thread or
   OS generator.
7. **AI state is plain data with stable ids.** It is serialisable and saves and
   reloads with the world ([ADR-0011](0011-simulation-stays-network-ready.md) rules 2 and 3).

| Moves into the engine | When |
|---|---|
| Perception (sight cones, hearing, factions) | Strategy pawns fight, giving it a second consumer |
| Noise or stimulus events | Both lines emit or consume them |
| Cover-point queries | Strategy needs cover |
| Shared graph search | The strategy voxel router and engine navigation both exist |
| AI and navigation debug draw | First AI or navigation debugging pain on either line |
| Per-tick AI budget or time slicing | Measured cost at 50 pawns or at a Director handover |

Until a row's trigger fires, the capability stays in the line that has it. Moving
one into the engine is a new seam, so it needs its own ADR
([ADR-0012](0012-engine-crate-map-for-m2.md)).

## Consequences

- AI commands are derived from state, not recorded. A replay or a lockstep peer
  needs only the initial state and the players' commands; the AI reproduces itself.
- Co-op stays open under either common netcode model, and this ADR picks neither
  (the netcode ADR does, per [ADR-0011](0011-simulation-stays-network-ready.md)).
  Host-authoritative, the usual co-op shooter model: only the host runs AI, and
  clients receive AI actors' state like any other actor's, so AI needs no
  network code of its own. Lockstep, the usual RTS model: every peer runs the
  same AI from the same state and seeds.
- A headless sim plays the same AI as a windowed one at any quality setting, so AI
  behaviour is testable in plain unit and determinism tests.
- AI can do nothing a player can't. A cheat-style action (spawn, teleport, see
  through walls) is a game rule visible to both, never a private AI path.
- On FPS maps the AI reads light through [ENG-F35](https://github.com/WrackedFella/moho/issues/509):
  direct light by ray, so shadow edges stay crisp, plus coarse CPU bounce from
  static and slow lights (the sun, fixed lamps), so a room lit only by bounce
  reads as lit. The bounce term may differ from the rendered GI in detail, never
  in kind. Bounce from dynamic lights (flashlights, muzzle flashes) arrives in a
  later ENG-F35 slice; until then it is invisible to the AI.
- Existing code that breaks a rule is fixed when its owning feature next reworks it,
  not in a sweep. Known case: `searchlight`'s sim seeds a private `SplitMix64`
  instead of a `SimRng` stream. It is seeded and replays, so only the stream rule
  (6) is unmet; it moves to `SimRng` when the brain is next reworked.
- Cost: each line writes its own decision code, and a line that wants a framework
  later builds or picks it for itself.
