# 0041 — Player stats resolve through one modifier layer, recomputed each tick from what is true now

**Status:** Accepted (Justin, 2026-11-24); lands with FPS-F7-03, #606

## Context

GDD §4.12 makes the player's capability the sum of what they wear and what is happening to
their body, and asks one question of the prototype: can one modifier layer carry gear, load,
wounds, stance and survival so the player can always answer "why am I slow"? FPS-F7 (#591)
births the layer with three stats (carry capacity, movement speed, stamina), three verbs (add,
scale, cap) and two sources (equipment, load). Wounds (FPS-M4), stance (FPS-F4), survival and
timed meds declare against it later, so the shape chosen now is what every later source
writes to.

Today each system scales its own number: the sim multiplies walking speed by
`LeanTuning::speed_factor` while a lean is held, and nothing else touches speed. A second and
third private multiplier (load, sprint) would make "why am I slow" unanswerable and the
character screen a second copy of the rules.

[ADR-0040](0040-a-loaded-save-plays-on-as-if-never-saved.md) saves all simulation state and
rebuilds derived state on load. [ADR-0011](0011-simulation-stays-network-ready.md) keeps the
sim host-owned and replayable, and [ADR-0015](0015-fps-rules-are-a-platform-free-crate.md)
keeps rules in a platform-free crate.

## Decision

1. **One layer, in the rules crate.** The stat layer is plain data and pure functions in
   `searchlight_rules`. It names no physics, input or UI type. The sim and the screens are its
   only callers.
2. **A closed set of stats.** Stats are an enum, at most six (§4.12). A stat joins the enum
   with its first real source, never ahead of it. Phase 2 has three: carry capacity (kg),
   movement speed (a factor on the tuning's gait speeds, 1.0 = unmodified) and stamina (the
   pool, in seconds of sprint). Each stat has a baseline from data (the fixed attribute sheet)
   and a floor.
3. **Sources declare contributions; nothing else changes a stat.** A contribution is a source
   label, a stat and one verb: `Add(x)`, `Scale(x)` or `Cap(x)`. Resolution is fixed: baseline,
   plus every add, times every scale (scales multiply, so two −50% sources leave 25%, never
   below zero), then the lowest cap, then the stat's floor. The order of contributions never
   changes the result. A system that reads a stat reads the resolved value; no system keeps a
   private multiplier on a stat. The lean's speed factor becomes a stance contribution.
4. **Contributions are derived, never saved.** Every tick, each source recomputes its
   contributions from current state (what is worn, what is carried, whether a lean is held),
   and the sheet is resolved from them. Only the state a source reads is simulation state
   (ADR-0040 rule 2); contributions and resolved values are derived and rebuilt on load. A
   timed source (meds, later) saves its timer as its own state, not its contribution.
5. **Stats resolve in a fixed order, without cycles.** A source may read only stats that
   resolve before the stat it changes. Carry capacity resolves first (equipment only); load
   reads it and carried weight; movement speed and stamina resolve after. A source that would
   need a cycle is a design error, raised when the source is designed.
6. **"Cannot" is a cap.** A capability that a source takes away is a cap on the stat that
   enables it: load's "cannot move" caps movement speed at 0, and "no sprint" caps the stamina
   pool at 0, so sprint, which needs stamina, cannot start. Current stamina is sim state
   clamped to the resolved pool.
7. **Every resolved value can explain itself.** Resolution can return, beside the value, each
   contribution with its source label in resolution order. The character screen shows that
   list; it never recomputes a stat. Presentation (limp, breathing, icon) reads resolved
   values and the load state, never a source's private number (§4.12 "presentation follows
   the number").
8. **Suppression is not built yet.** A suppressing source (painkillers mask wounds) arrives
   with meds (FPS-M4). The seam it needs is that contributions carry their source label, so a
   suppressor can drop a source's contributions before resolution.

## Consequences

- The character screen and the movement code read one answer, so "why am I slow" is the
  contribution list, and a new source shows up on the screen without screen code.
- Adding a source is local: it maps its state to contributions. Adding a stat is an enum
  variant, a baseline and a floor, and the compiler finds every match that must handle it.
- Save format: no stat state is persisted, so the stat set and verbs can change without a
  save version bump. The state sources read (worn items, carried items, current stamina) is
  saved by the features that own it.
- Determinism: resolution uses only `+`, `*`, `min` and `max` in a fixed order, so it replays
  bit-exact (ADR-0011).
- Cost: contributions are rebuilt every tick for one player, a handful of items. If enemies
  ever resolve stats, the per-tick rebuild is the first thing to cache, keyed by what the
  sources read.
- "No sprint" as a stamina cap means that after the player drops below the threshold,
  stamina refills from empty at its recovery rate instead of being full at once.
- A stat a source wants that is not in the enum (steadiness, recoil control, handling) waits
  for that source's card, which adds it.
- Rejected: each system keeps its own multiplier and the screen re-derives them (two copies
  of the rules, the GDD's question goes unanswered); stored, incrementally updated stat values
  (saved state that can drift from its sources, and every source must undo itself); an
  extra "capability" verb for sprint (a fourth verb against the GDD's three, for one case).
