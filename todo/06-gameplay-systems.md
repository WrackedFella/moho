# Phase 6: Gameplay Loops

**Goal:** Mechanics that rely on the systems built above (RTS, AI, Saves).

## Tasks

### 0. Point Light Shadow Maps (Backlog — from Phase 2)
- **Context:** Dynamic point lights currently illuminate geometry without occlusion. Directional (sun/moon) shadows already work for all actors and terrain.
- **Goal:** Implement per-point-light shadow maps (cube shadow maps or dual-paraboloid) so actors/terrain occlude local lights.
- **Prerequisite:** Shadow exclusion flag (`cast_shadows: bool`) on actor types to suppress debug gizmo spheres from the shadow pass.
- **Estimate:** 5 SP

### 1. Persistence Update (Saves)
- **Review:** Current `save.rs` likely only dumps raw voxel arrays.
- **Update:**
    - Support storing Entity/Actor positions.
    - Support storing modified `MicroChunks` (from Phase 5).
    - Versioning support for save files.

### 2. RTS Command Logic
- **Selection:**
    - Box selection (Screen space rect -> Frustum cull against actors).
    - Raycast selection (Single actor).
- **Commands:**
    - Right-click "Move To" (Requires NavMesh/Pathfinding).
    - "Patrol", "Hold Position".

### 3. Simple AI
- **State Machine:**
    - Idle / Wander / Chase / Flee.
- **Sensing:**
    - Simple vision cone or distance check to Player.
