# 0023 — Pawn kinds share components; flagstone adopts hecs at the second kind

**Status:** Proposed (2026-10-23); extends [0004](0004-entity-storage-without-a-general-ecs.md) for the flagstone line

## Context

The flagstone colony will hold more than one kind of pawn: colonists first, then
animals and monsters. Colonists will also take roles (jobs) that decide which work
they pick up. Kinds and roles cut across each other: an animal and a colonist both
move, route and can be possessed or hurt; a miner and a builder are both colonists
with different job filters. That is the shape [ADR-0004](0004-entity-storage-without-a-general-ecs.md)
names as the trigger for an ECS: many entity kinds that share components, and
queries across them.

The trigger has not fired yet. [SG-F4](https://github.com/WrackedFella/moho/issues/310)
(pawns dig what I mark) has one kind and one job type, and its specs
([#436](https://github.com/WrackedFella/moho/issues/436), [#437](https://github.com/WrackedFella/moho/issues/437))
use typed stores keyed by a stable `PawnId`, job state held per pawn, and routing as
pure functions. This ADR fixes when the switch happens and which seams keep it
mechanical, so SG-F4 and the cards after it don't build something the switch has to
undo.

Options for modelling kinds:

| Option | Fits two kinds | Fits kinds × roles × capabilities | Cross-kind queries | Save |
|---|---|---|---|---|
| `enum PawnKind` field on one pawn struct | Yes | No: every kind carries every field, or the struct splits per kind | A match in every system | Plain data |
| A trait per kind (`trait Animal`) behind `Box<dyn …>` | Yes | No: capabilities don't nest as a hierarchy | Downcasts | Needs a type registry |
| Typed stores keyed by `PawnId`, one store per concern | Yes | Yes, by hand: each new concern is a new store and its own join | Hand-written joins | Plain data |
| `hecs` world, one component per concern | Yes | Yes: a kind is a component set | Queries | Plain data by `PawnId`, not `hecs::Entity` |

## Decision

1. **A pawn kind is a set of components, not a type.** Colonist, animal and monster
   are labels for which components a pawn carries (movement, inventory, job queue,
   role, faction, behaviour). No kind subtypes another, and no system matches on a
   kind enum to decide what a pawn can do; it asks whether the pawn has the component.
2. **One controller for every kind.** Every kind is driven through the same
   `Controller`/`Pawn` actions, whether by the player or by AI
   ([ADR-0021](0021-game-ai-lives-in-the-lines.md) rule 4). A kind gains a verb only
   by gaining the component that grants it, and the player gets it too when possessing
   that pawn.
3. **A role is data on a pawn.** A role is a component that filters which jobs a pawn
   accepts and in what order. Job types stay one list shared by all roles; a role never
   owns its own job loop.
4. **The switch to `hecs` happens with the second pawn kind.** The first card that adds
   a pawn kind other than colonists, or a second component set that systems must query
   across, carries the migration of pawns from typed stores into a `hecs::World` owned by
   `flagstone_game`. Roles alone do not trigger it: one kind with a role component fits
   typed stores. `hecs` stays the default ([ADR-0004](0004-entity-storage-without-a-general-ecs.md));
   choosing anything else needs a new ADR.
5. **Until then, keep the seams `hecs`-shaped.** Pawns are addressed by a stable
   `PawnId`, never by index or reference. Per-pawn state lives in stores keyed by
   `PawnId`, one store per concern, not in a growing pawn struct. Systems are functions
   over those stores that run inside the tick. Pawn data is plain, serialisable values.
6. **The ECS is a flagstone concern.** `hecs` becomes a dependency of `flagstone_game`
   only. Engine crates keep taking domain-shaped data and never name `hecs` types
   ([ADR-0001](0001-render-api-boundary.md), [ADR-0004](0004-entity-storage-without-a-general-ecs.md)).
   Terrain chunks stay in `ChunkStore`; they are not entities. Whether buildings
   ([SG-F3](https://github.com/WrackedFella/moho/issues/312)) join the world is the
   SG-F3 Tech Lead's call, decided by whether they share components with pawns.
7. **Order and identity stay deterministic.** `hecs::Entity` ids are internal: saves,
   commands and events name pawns by `PawnId` ([ADR-0011](0011-simulation-stays-network-ready.md),
   [ADR-0016](0016-game-lines-own-their-save-kinds.md)). Any system whose result depends
   on visit order (job claims, one pawn per mark) iterates in `PawnId` order, not archetype
   order.

## Consequences

- SG-F4 ships on typed stores as specced; nothing in #436 or #437 changes.
- The migration card is mechanical: each `PawnId`-keyed store becomes a component, each
  hand-written join becomes a query, and save data keeps its shape. Its cost lands on the
  feature that brings the second kind, so that feature's estimate includes it.
- A new pawn kind costs a component set and the systems that are new to it, not a copy of
  the colonist code. A creature without a job queue simply isn't visited by the job loop.
- Possession of non-colonist kinds is a design choice per kind (which components the
  first-person verbs need), not an architecture change.
- The FPS line is not bound by this ADR. If searchlight enemies want an ECS, they apply
  ADR-0004 on their own; the engine still ships none.
- Cost: until the switch, adding a per-pawn concern means a new store and its joins by
  hand; after it, `hecs` and its archetype iteration rules are a dependency the line owns.
