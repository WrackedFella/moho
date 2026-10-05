# Strategy game — vision

The design intent behind the strategy line. Features and their order live in
[`_todo/strategy-game/`](../../_todo/strategy-game/) and the
[roadmap](../../_todo/ROADMAP.md). This page holds what stays true across features.

## The game

A base-builder and mining game:

- **Core loop:** mine → build → automate → progress, in the style of Satisfactory or
  Factorio.
- **Colony control:** direct units and jobs, as in Dwarf Fortress.
- **Optional direct control:** take over a single unit in first person, Minecraft-style.

## Design principles

- **The setting can be swapped.** The setting (alien corporation, RTS, tech tree,
  combat flavour) is deliberately undecided. It could change per world, for example
  different planets reached through portals. The core systems (mining, building,
  units, automation, progression, combat) must not depend on it.
- **One controller for players and AI.** A single `Controller`/`Pawn` abstraction
  drives player and AI units the same way. AI pawns reuse the player's actions
  (`Pawn::mine` takes plain `glam` types, with no input coupling). Where that would
  overcomplicate a piece, choose the simpler option.
- **Terrain is editable voxels.** The terrain engine belongs to this line, not the
  shared engine ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)).
- **Mining will become sub-grid deformation.** Today a block breaks whole. Later, a
  full voxel becomes a grid of sub-blocks that wear away gradually. Only the
  block-destruction step inside `Pawn::mine` may assume whole blocks. Its signature and
  the deposit contract (`ResourceYield { resource_id, amount }`) must not.
- **Build for a named consumer.** Systems arrive with the feature that needs them.
  An example is SG-F4, the worker pawns, which is the first possible ECS consumer
  ([ADR-0004](../../_todo/adr/0004-entity-storage-without-a-general-ecs.md)).

## Feature arc

1. Core interaction loop: aim, mine, inventory, hotbar, tool gating, pickup feedback
   ([SG-F1](../../_todo/strategy-game/SG-F1-core-interaction-loop/_feature.md)).
2. Buildings placed from a context-sensitive build menu
   ([SG-F3](../../_todo/strategy-game/SG-F3-buildings-and-construction/_feature.md)).
3. Worker pawns that gather on their own: go, act, return
   ([SG-F4](../../_todo/strategy-game/SG-F4-worker-pawns/_feature.md)).
4. Automation and progression (not yet planned).
