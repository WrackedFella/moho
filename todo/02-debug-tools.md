# Phase 2: Debugging & Tools

**Goal:** Tools that make future work easier and fixing high-visibility bugs.

## Tasks

### 1. RTS Camera Mode (Debug View)
- **Goal:** Implement a "birds-eye" free camera to easily inspect terrain generation and lighting.
- **Controls:** WASD for planar movement, Scroll for zoom/height.
- **Toggle:** Keybind to switch between First-Person and RTS/Debug Camera.
- **Note:** This is purely for visualization right now (no unit selection).

### 2. HUD Debug Info
- **Goal:** Toggleable overlay to verify world state.
- **Data Points:**
    - Player World Coordinates (x, y, z)
    - Material ID under crosshair (Raycast result)
    - Current Chunk ID
    - FPS / Frame Time
- **Implementation:** Simple text block in `moho_ui`.

### 3. Fix Spawn Command
- **Problem:** Spawning entities results in odd visual behavior or placement errors.
- **Action:** Debug `console_commands.rs` -> `spawn` logic. Ensure entities snap to grid or resolve collision correctly.

### 4. Fix Lights "Turning Off"
- **Problem:** Point lights cut out when the camera moves (likely AABB/Frustum culling issue).
- **Action:** Investigate `moho_renderer` light culling logic.
- **Verify:** Lights should persist even when the source is off-screen if the light radius is still visible.
