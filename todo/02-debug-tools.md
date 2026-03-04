# Phase 2: Debugging & Tools

**Goal:** Tools that make future work easier and fixing high-visibility bugs.

**Status:** In Progress

## Tasks (Sequenced for Dependency Order)

### 1. FPS Control Updates ✅
- **Goal:** Clean up existing FPS controls to behave in a more standard way.
- **Status:** COMPLETE
- **Implemented:**
    - Camera/player moves in the direction the camera is facing (including pitch for forward/back)
    - Up (Space) and Down (Ctrl) controls working
    - Shift for sprint (1.5x speed multiplier)
    - All directional movement follows camera orientation correctly
- **Completed:** 1 SP

### 2. RTS Camera Mode (Debug View) ✅
- **Goal:** Implement a "birds-eye" free camera to easily inspect terrain generation and lighting.
- **Status:** COMPLETE
- **Implemented:**
    - Toggle via Tab key (switches between FirstPerson and Isometric camera modes)
    - WASD for planar movement at fixed height
    - Mouse scroll to zoom in/out (adjusts camera height from 5-50 units)
    - Fixed 45-degree angle view looking down at terrain from offset (10, 10, 10)
    - Smooth zoom along horizontal/vertical distance maintaining aspect ratio
- **Completed:** 2-3 SP

### 3. HUD & Overlay System ✅
- **Goal:** Overlay manager + gameplay HUDs + toggleable debug overlay.
- **Status:** COMPLETE
- **Implemented:**
    - **Overlay system** (`moho_ui::overlays`): generic `Overlay` trait + `OverlayManager` — registers, renders, and toggles named overlay layers with shared `HudData`.
    - **FPS HUD** (`fps_hud`): crosshair at screen center + time-of-day badge (top-right). Auto-hides in Isometric mode.
    - **RTS HUD** (`rts_hud`): stub — time-of-day badge only. Auto-hides in FirstPerson mode. Ready for mini-map, selection info, etc.
    - **Debug HUD** (`debug_hud`): toggled with **F3**. Displays FPS (smoothed), frame time, player position, chunk coordinates, camera mode, material under crosshair (placeholder).
    - Overlays render during Playing, Paused, and ConsoleOpen states.
    - `HudData` updated every frame from the main loop (position, chunk, camera mode, dt, time of day).
    - Adapter visibility gate fixed so overlays render during gameplay even when menus are hidden.
- **Data Points:**
    - Player World Coordinates (x, y, z)
    - Material ID under crosshair (stub — raycast wiring needed)
    - Current Chunk ID
    - FPS / Frame Time
    - Camera Mode (FirstPerson / Isometric)
    - Time of Day
- **Completed:** 2 SP

### 4. Fix Lights "Turning Off"
- **Problem:** Multiple light-related issues discovered:
    1. Point lights cut out when the camera moves (AABB/Frustum culling issue in `moho_renderer`)
    2. Spawned lights do not persist between loads (not saved in save format?)
    3. New actors do not block light/cast shadows/occlude light sources
        - *Note:* This may have been an accepted design compromise during light system implementation. Assessment needed: what is the effort to enable actor shadowing?
- **Depends on:** Better camera controls to reproduce and debug issues (now resolved).
- **Action:** 
    - Investigate and fix frustum culling logic in `moho_renderer::lights`
    - Review save format: determine if lights need explicit persistence or should be derived from actor state
    - Assess actor blocking: check if `VoxelChunk` shadowing is a mesh-only vs actor limitation
- **Verify:** 
    - Lights persist when camera moves (source off-screen but visible radius in-screen)
    - Spawned lights remain after save/load
    - New actors properly interact with light propagation
- **Estimate:** 2-3 SP (larger due to investigation scope)
