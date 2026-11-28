# 0027 — Characters slide on our own collide-and-slide, not rapier's controller

**Status:** Accepted on merge of #700

## Context

[ADR-0022](0022-character-changes-are-tick-input.md) puts the movement rules in the step and
leaves `PhysicsWorld` collide-and-slide and queries. The slide is rapier's
`KinematicCharacterController::move_shape`, which brings movement rules of its own: it counts a
character as grounded on any contact facing even slightly up (`normal.y >= 1e-3`, not
configurable in rapier 0.36), snaps to ground on level and downward moves only, on any slope,
and steps up with an autostep that fails on float noise at the skin offset. Every movement card
since ENG-F21-01 has corrected one of these: the step-up fallback and landing probe (#271),
stick-to-floor and the flat-base ground read (#330, #496), edge roll-off (#649) and steep faces
(#700). Rapier's rigid bodies, broad phase and parry queries have needed no such corrections.

## Decision

1. Character collide-and-slide is `moho_physics` code on parry shape casts through rapier's
   query pipeline: cast, stop a skin short, clip against the hit plane, repeat up to a fixed
   number of planes.
2. Whether a character is grounded is decided only by the ground read and the walkable rule;
   no third-party flag decides it.
3. rapier3d stays the rigid-body, broad-phase and query backend. `moho_physics` does not use
   `KinematicCharacterController`.
4. Movement code reaches collision only through `CharacterQueries`, so a backend change
   touches `moho_physics::world` only.

## Consequences

- The ground, step and walkable rules live in one place; the landing probe, the KCC step-up
  fallback and the `CharacterLength` read-backs go away.
- `moho_physics` owns depenetration and multi-plane corners (Source `TryPlayerMove` and Jolt
  `CharacterVirtual` are the references).
- Characters pushing props stays with the props feature (the `set_position` teleport and
  rapier's `solve_character_collision_impulses`).
- Changing the physics backend (for example to Jolt) is reconsidered only if rapier's solver
  falls short on props or ragdolls.
