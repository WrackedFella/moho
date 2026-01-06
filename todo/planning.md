# Project Roadmap

This document outlines the high-level phases for upcoming development. Detailed tasks for each phase are broken out into separate files in this directory.

## Phase 1: Maintenance & Hygiene (The "Easy Wins")
*Focus: Clean up the workspace and fix immediate annoyances to improve developer experience.*
- **[Details](./01-maintenance.md)**
- Reverse Console Scroll (UI Logic)
- Clean Shutdown / Resource Cleanup
- Prune Outdated Tests
- Add Local READMEs for Crates

## Phase 2: Debugging & Tools (High Utility)
*Focus: Tools that make future work easier and fixing high-visibility bugs.*
- **[Details](./02-debug-tools.md)**
- **RTS Camera Mode**: Essential for inspecting terrain generation and lighting.
- **HUD Debug Info**: Toggleable overlay (Coords, MatID) for verifying Physics later.
- Fix Spawn Command oddities.
- Fix Lights "Turning Off" (Culling/Bounds issue).

## Phase 3: UI & Settings (Isolated Systems)
*Focus: Polish that doesn't touch the simulation core.*
- **[Details](./03-ui-polish.md)**
- Menu Improvements (Visuals, Audio).
- Settings: Discrete Resolution Locking.
- HUD Visual Framework (Compass, Health/Stamina bars).

## Phase 4: Physics Integration (Rapier3D)
*Focus: The heavy lifting for simulation.*
- **[Details](./04-physics.md)**
- Construct Physics ECS bundle.
- Rapier3D Integration.
- Basic Gravity & Primitive Collisions.
- Character Controller (Capsule vs World).

## Phase 5: Terrain Overhaul (Research Spike)
*Focus: Researching high-fidelity voxel terrain architectures.*
- **[Details](./05-terrain-overhaul.md)**
- Research: Uniform High-Density vs. Adaptive Subdivision (Octrees) vs. Clustered Micro-Grids.
- Prototype: "Micro-Grid" deletion (Deforming blocks into sub-voxels).
- Update Mesher for mixed resolutions.

## Phase 6: Gameplay Loops
*Focus: Mechanics that rely on the systems built above.*
- **[Details](./06-gameplay-systems.md)**
- RTS Unit Selection & Commands.
- Gameplay Save Persistence (Updated format).
- Simple AI.
