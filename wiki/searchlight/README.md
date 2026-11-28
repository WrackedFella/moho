# Searchlight

The FPS line: a survival-focused looter shooter with realistic gunplay. This section holds its game design, order of
work and architecture, kept apart from the [strategy line](../flagstone/README.md) and the shared
[engine](../engine/README.md).

| Page | Holds |
|---|---|
| [Game design document](game-design-document.md) | What the player experiences: pillars, systems, phases, open questions |
| [Roadmap](roadmap.md) | Milestones FPS-M1 to FPS-M7, entry gates, exit goals, engine requests |
| [Architecture](architecture.md) | The `searchlight` arena, shot path, simulation and game adapter |

## Determinism call sites

Simulation transcendentals go through `moho_core::sim_math` ([determinism](../architecture/determinism.md)).
Each line migrates its sites under its own card and adds the files to `scripts/sim-math.txt`.
Today's non-test platform calls in `searchlight_rules` and `searchlight`:

| Site | Calls | Status |
|---|---|---|
| `searchlight_rules/src/accuracy.rs` `disc_offset` | `tan`, `cos`, `sin` | sim: shot spread |
| `searchlight/src/game.rs` `tick` | `hypot` | check: player speed passed to the head bob |
| `searchlight/src/game.rs` `gap` | `tan` | presentation: crosshair |
| `searchlight/src/game.rs` `frame` | `atan2` | presentation: view yaw |
| `searchlight/src/head_bob.rs` `Bob::offset` | `sin` | presentation: head bob |
| `searchlight/src/hud.rs` `hit_edge` | `sin`, `cos` | presentation: HUD |

Not here: card and feature text, status and specs (GitHub issues and the
[board](https://github.com/users/WrackedFella/projects/1)); unfiled drafts ([`_todo/searchlight/`](../../_todo/searchlight/));
cross-line dependencies ([`wiki/roadmap.md`](../roadmap.md)).
