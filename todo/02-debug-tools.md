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

### 4. Fix Lights "Turning Off" ✅
- **Problem:** Multiple light-related issues discovered.
- **Status:** COMPLETE (core issues resolved; one item deferred to backlog)
- **Implemented:**
    - **Frustum culling fix** (`moho_renderer::lights`): Replaced the broken NDC-space point test with a proper Gribb-Hartmann frustum-plane vs. bounding-sphere intersection. Lights whose radius overlaps the frustum are now correctly included regardless of whether their center is off-screen or behind the camera.
    - **Light persistence** (`moho_renderer::scene`): Added `LightDesc` to the scene save format (v3). `encode_to_bytes` and `load_from_bytes` now accept/return light descriptors. Autosave collects lights from the renderer via `all_lights_as_descs()`; on load, lights are re-added to the renderer. Backward compatible: v1/v2 saves load fine with an empty lights list.
    - **Debug light gizmo**: Spawning a light via `spawn light` now also spawns a small sphere (radius 0.15) at the light position using the light's own color as albedo. The gizmo entity is tracked in `App::light_gizmos` (keyed by light ID) for future removal support.
- **Deferred to backlog:**
    - **Point light shadow casting** (`06-gameplay-systems.md`): Dynamic point lights currently illuminate scene geometry without occlusion (no shadow maps). Directional sun/moon shadows already work for all actor geometry. Implementing point light shadow maps (cube shadow maps) is a new feature (~5 SP) logged in the gameplay backlog.
    - **Shadow exclusion flag for actors**: A `cast_shadows` flag on `Sphere`/`Cube` actors (to suppress gizmo sphere shadows) is deferred alongside the shadow pipeline work.
- **Completed:** 3 SP
