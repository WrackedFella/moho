# 0014 — Characters move by velocity; physics only resolves contact

**Status:** Proposed

## Context

Both games want Source-engine-style physics eventually (Justin, 2026-10-10): player
movement with momentum (acceleration, friction, air control, speed kept along walls
and slopes) and physically interactable objects, including damage scaled to the
momentum of an impact. Neither is needed for M1, but the M1 controller
([ENG-F21-01](https://github.com/WrackedFella/moho/issues/271)) fixes the shape of
character movement for both lines.

Today a character moves by `speed × direction × dt` each tick with no horizontal
velocity carried between ticks, and `PhysicsWorld::move_character` integrates
gravity itself. Momentum can't be added to that shape later without rewriting the
controller and every game that tunes it.

Quake's `pmove`, Source's `CGameMovement`, Unreal's `CharacterMovementComponent` and
Godot's `CharacterBody3D` all split the same way: a movement layer owns the
character's velocity and rules, and the physics engine only sweeps a shape and
reports what it touched.

## Decision

1. **A character carries a velocity.** Velocity is part of the character's
   simulation state and persists between ticks.
2. **The movement step owns the rules.** Gravity, jumping, ground friction, and
   ground and air acceleration live in one movement step, driven by the per-tick
   command ([ADR-0011](0011-simulation-stays-network-ready.md)) and by movement
   settings the game supplies as data. `PhysicsWorld` provides collide-and-slide and
   queries; it holds no movement rules.
3. **Velocity is clipped against what the slide hits.** Speed into a surface is
   removed; speed along it is kept.
4. **The step reports impacts; the game decides what they mean.** Each impact names
   the surface, its normal and the speed lost into it. Damage, fall damage and sounds
   are game rules built on that report.
5. **The step is a function of (character state, command, settings, collision
   queries).** It reads nothing else and writes only the character's state, so it can
   be re-run on the same inputs.

## Consequences

- Momentum, air strafing, knockback (adding velocity) and momentum-scaled impact
  damage need no change to the controller's shape, only settings and game rules.
- A game's current "instant" feel is a choice of settings (high acceleration and
  friction), so the strategy game keeps its behaviour.
- Client-side prediction stays possible later: re-running rule 5's function on
  unacknowledged commands is how Source predicts movement. No netcode is chosen here.
- Physical objects (props) are a later engine feature; they reuse the velocity and
  impact report from this decision, and objects striking a character are reported
  the same way. Tracked in [ENG-F21](https://github.com/WrackedFella/moho/issues/123)'s
  Deferred table.
- Cost: movement settings grow from two values (speed, jump) to about eight, and tests
  that assumed instant speed state their settings or start at speed.
