# Phase 2: Debugging & Tools

**Goal:** Tools that make future work easier and fixing high-visibility bugs.

**Status:** In Progress

## Tasks (Sequenced for Dependency Order)

### 1. FPS Control Updates ⏳
- **Goal:** Clean up existing FPS controls to behave in a more standard way.
- **Priority:** HIGH — Blocking other camera tasks.
- **Key Points:**
    - Camera/player should move in the direction the camera is facing.
    - "Up" and "Down" controls do not currently work. 
    - Default CTRL to down (currently shift).
    - Shift increases movement speed (run/sprint)
- **Estimate:** 1 SP

### 2. RTS Camera Mode (Debug View) 
- **Goal:** Implement a "birds-eye" free camera to easily inspect terrain generation and lighting.
- **Depends on:** Solid FPS controls implemented.
- **Controls:** WASD for planar movement, Scroll for zoom/height.
- **Toggle:** Keybind to switch between First-Person and RTS/Debug Camera.
- **Note:** This is purely for visualization right now (no unit selection).
- **Estimate:** 2-3 SP

### 3. HUD Debug Info
- **Goal:** Toggleable overlay to verify world state.
- **Data Points:**
    - Player World Coordinates (x, y, z)
    - Material ID under crosshair (Raycast result)
    - Current Chunk ID
    - FPS / Frame Time
- **Implementation:** Simple text block in `moho_ui`.
- **Estimate:** 1-2 SP

### 4. Fix Lights "Turning Off"
- **Problem:** Point lights cut out when the camera moves (likely AABB/Frustum culling issue).
- **Depends on:** Better camera controls to reproduce issue more easily.
- **Action:** Investigate `moho_renderer` light culling logic.
- **Verify:** Lights should persist even when the source is off-screen if the light radius is still visible.
- **Estimate:** 1-2 SP
