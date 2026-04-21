# Technical Debt Backlog

This directory tracks structural and quality issues identified in the April 2026
codebase audit. Items are ordered by priority. Work on these before the terrain
overhaul (exp/terrain) and gameplay systems, so new work builds on solid ground.

---

## Near-term (do before terrain overhaul)

| # | Item | Why now |
|---|---|---|
| [TD-01](01-seed-determinism.md) | Fix terrain seed determinism | Correctness bug, one-afternoon fix, blocks trust in world generation |
| [TD-02](02-save-light-system.md) | Fix broken save → LightSystem path | Active bug on every load; will be compounded by terrain work |
| [TD-03](03-unsafe-soundness.md) | Audit unsafe Send/Sync and Box::leak | Soundness hole; violates own code conventions |
| [TD-04](04-input-consolidation.md) | Consolidate InputDispatcher + InputRouter | Two parallel systems, one unused; confusing priority conventions |

## Medium-term (do alongside terrain overhaul)

| # | Item | Why |
|---|---|---|
| [TD-05](05-moho-core-trim.md) | Trim moho_core — remove legacy, extract domain crates | Gates the "reusable crates" goal; ray-tracer residue is dead weight |
| [TD-06](06-app-god-object.md) | Dissolve App god object | Gameplay features will pile state onto App; fix before that starts |
| [TD-07](07-serialization-story.md) | Pick one serialization strategy | Two approaches in parallel; bincode vs serde_json decision |
| [TD-08](08-event-bus-issues.md) | Event bus performance and convention issues | Central nervous system; triple-lock dispatch, misleading metrics API |
| [TD-13](13-test-coverage-gaps.md) | Add high-value tests | Save/load round-trip, seed determinism guard, mesh index safety, BlockModifier invariants |

## Lower priority (do when convenient)

| # | Item | Why |
|---|---|---|
| [TD-09](09-renderer-backend-trait.md) | Clean up RendererBackend trait | No-op methods, redundant module nesting, no actual second backend |
| [TD-10](10-cargo-and-dead-code.md) | Cargo hygiene and dead code | Small items; each is a 5-minute fix |
| [TD-11](11-moho-sim-clarity.md) | Clarify moho_sim — two simulations in one crate | Misleading crate description; `SimulationController` is not headless/integer |
| [TD-12](12-docs-drift.md) | Fix docs drift | Broken links, wrong feature flags in Quick Start, stale status |

---

## Not tracked here

The terrain overhaul architecture (paletted chunks, 3D density field, LOD, streaming)
is documented in [todo/terrain-gen-updates.md](../todo/terrain-gen-updates.md) and is
the primary feature roadmap item, not a debt item.
