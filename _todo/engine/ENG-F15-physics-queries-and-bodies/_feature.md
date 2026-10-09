# ENG-F15 — Games query physics and run many bodies

**Issue:** [#227](https://github.com/WrackedFella/moho/issues/227)
**Status:** Backlog
**Labels:** feature, line:engine

## Summary

Hit-scan and body-part hits need ray queries and several characters. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- A ray query returns hit point, normal, collider and a game-supplied tag.
- Characters are keyed by handle; at least two run at once (test).
- Kinematic volumes follow a character's transform each tick (test: a ray hits the "head" volume).

## Scope

- Out: projectile ballistics.

## Items

| Item |
|---|
