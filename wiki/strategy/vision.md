# Strategy game — vision

The design intent behind the strategy line. Features and their order live in the
[roadmap](../../_todo/ROADMAP.md) and the GitHub issues. This page holds what stays true across features.

## The game

A base-builder and mining game played from above:

- **Strategy mode is the game.** The player runs a colony like RimWorld or Dwarf
  Fortress: marks ground to dig, places buildings, and watches pawns do the work.
- **First person is possession.** The player can take over one pawn and act through
  its eyes, Minecraft-style. While possessed, that pawn's own work pauses; today it is
  the single player pawn, later every pawn is AI-driven when not possessed.
- **Core loop:** mine → build → automate → progress, in the style of Satisfactory or
  Factorio.

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
  An example is SG-F4, pawns that dig what the player marks, which is the first possible ECS consumer
  ([ADR-0004](../../_todo/adr/0004-entity-storage-without-a-general-ecs.md)).

## Feature arc

Features and their order live in the [roadmap](../../_todo/ROADMAP.md); the issues are the record.

1. A saved world comes back as left ([SG-F2](https://github.com/WrackedFella/moho/issues/308)).
2. Strategy view and possession: pan, select, step into a pawn ([SG-F5](https://github.com/WrackedFella/moho/issues/309)).
3. Pawns dig what the player marks ([SG-F4](https://github.com/WrackedFella/moho/issues/310)).
4. Buildings placed from a build menu and constructed by pawns ([SG-F3](https://github.com/WrackedFella/moho/issues/312)).
5. Possession polish: pickup feedback ([SG-F1](https://github.com/WrackedFella/moho/issues/311)).
6. World generation polish ([SG-F6](../../_todo/strategy-game/SG-F6-world-generation/_feature.md), draft).
7. Automation and progression (not yet planned).
