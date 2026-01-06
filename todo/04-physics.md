# Phase 4: Physics Integration

**Goal:** Integrate `rapier3d` for simulation logic.

## Tasks

### 1. Integration
- Add `bevy_rapier3d` (or raw `rapier3d` if we aren't using Bevy ECS) to `Cargo.toml`.
- Initialize Physics World resource in `moho_sim` / `moho_core`.
- Create a `PhysicsBundle` (RigidBody, Collider) for ECS entities.

### 2. Terrain Colliders
- **Challenge:** Generating efficient colliders for voxel terrain.
- **Approach:**
    - *Simple:* Create a static trimesh collider from the chunk mesh.
    - *Complex:* Decompose into box primitives (avoid for now).

### 3. Basic Interactions (Gravity)
- Enable gravity.
- Spawn test spheres/cubes that fall and stack.
- Verify Player Actor does not fall through the world.

### 4. Character Controller
- Replace current "flying camera" logic with a kinematic character controller.
- Capsule collider for the player.
- Logic: Walk, Jump, Slope handling (provided by Rapier).
