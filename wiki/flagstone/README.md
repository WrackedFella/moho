# Flagstone

The strategy line: a colony base-builder and mining game on editable voxel terrain. This section holds its design
intent, game design and order of work, kept apart from the [FPS line](../searchlight/game-design-document.md) and the
shared [engine](../architecture/overview.md). Code, crates and binary use the name `flagstone`; issues and the board
say "strategy".

| Page | Holds |
|---|---|
| [Vision](vision.md) | What stays true across features: the game, design principles, feature arc |
| [Game design document](gdd.md) | What the player experiences: pillars, modes, loop, systems, deferred ideas, open questions |
| [Roadmap](roadmap.md) | Milestones STRAT-M1 to STRAT-M4, feature order, engine dependencies, strategy decisions |
| [World generation research](worldgen-research.md) | Terrain representation, overworld, slices, caves and ore seams compared; the path SG-F6 proposes |

## Determinism call sites

Simulation transcendentals go through `moho_core::sim_math` ([determinism](../architecture/determinism.md)).
Each line migrates its sites under its own card and adds the files to `scripts/sim-math.txt`.
Today's non-test platform calls in `flagstone_game`, `flagstone_voxel` and `flagstone`
(`flagstone_voxel` has none):

| Site | Calls | Status |
|---|---|---|
| `flagstone_game/src/jobs.rs` `horizontal_distance` | `hypot` | sim: pawn job distance |
| `flagstone_game/src/jobs.rs` `steer` | `atan2` | sim: pawn walk heading |
| `flagstone_game/src/controller.rs` `PlayerController::look_at` | `atan2`, `asin` | check: sets the controller's yaw and pitch |
| `flagstone_game/src/controller.rs` `PlayerController::apply_input` | `sin`, `cos`, `exp` | check: moves the first-person and RTS camera targets |
| `flagstone/src/app/event_loop/event_processor.rs` `handle_input_event` (wheel) | `sin`, `cos` | check: moves the first-person position along the look direction |
| `flagstone_game/src/controller.rs` `controller_to_camera` | `sin`, `cos` | presentation: view matrices |
| `flagstone_game/src/actors.rs` `Sphere::unit_sphere_indexed` (sphere sampling) | `sin`, `cos` | presentation: mesh vertices |

Not here: card and feature text, status and specs (GitHub issues and the [board](https://github.com/users/WrackedFella/moho/issues));
unfiled drafts ([`_todo/flagstone/`](../../_todo/flagstone/)); the cross-line [roadmap](../roadmap.md).
