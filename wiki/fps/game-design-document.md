# FPS — game design document

**Status:** draft. Writing this page is
[FPS-F1](../../_todo/fps-game/FPS-F1-game-design-document/_feature.md)'s deliverable.
Sections marked *TBD* are open.

## Pillars

*TBD:* three to five statements that decide arguments about scope.

## Genre and setting

*TBD.*

## Core loop

*TBD:* the shape of movement and combat, and what a session looks like.

## Scope boundaries

What this game is not:

- *TBD.*

## Maps and levels

Known constraints:

- Maps are not voxels. They are pre-made, baked files loaded without generation,
  or generated at load time as non-voxel geometry. Say which, per mode or level,
  because it decides the pipeline.
- The engine takes world geometry as meshes keyed by a handle
  ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)). A map format,
  loader or generator is FPS-line work.

*TBD:* the content plan and the first playable level.

## Engine fit

Available from the engine: wgpu rendering (cascaded shadows, SSAO, skybox), a rapier3d
character controller and triangle-mesh colliders, audio, input mapping, the event bus,
and a fixed 60 Hz tick ([ADR-0009](../../_todo/adr/0009-simulation-time-is-one-fixed-tick.md)).

Not available yet: weapons and damage, enemy AI, a map pipeline, and FPS menus.
Anything generic that the FPS needs moves into the engine. The FPS line never depends on
strategy crates ([ADR-0005](../../_todo/adr/0005-crate-lines-and-dependency-direction.md)).

## Engine requests

What the game needs from the engine, stated as capabilities rather than designs:

- *TBD.*
