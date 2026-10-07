# ENG-F15 — Games query physics and run many bodies


## Summary

Hit-scan and body-part hits need ray queries and several characters. Consumer: FPS GDD v0.2 §10 (pending on `dev`); strategy is a likely second consumer.

## Exit criteria

- A ray query returns hit point, normal, collider and a game-supplied tag.
- Characters are keyed by handle; at least two run at once (test).
- Kinematic volumes follow a character's transform each tick (test: a ray hits the "head" volume).

## Scope

- Out: projectile ballistics.

## Items

| Item |
|---|
