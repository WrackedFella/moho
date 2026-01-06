# Phase 6: Gameplay Loops

**Goal:** Mechanics that rely on the systems built above (RTS, AI, Saves).

## Tasks

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
