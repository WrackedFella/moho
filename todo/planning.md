# Project Roadmap

This document outlines the high-level phases for upcoming development. Detailed tasks for each phase are broken out into separate files in this directory.

## Phase 1: Maintenance & Hygiene (The "Easy Wins")
*Focus: Clean up the workspace and fix immediate annoyances to improve developer experience.*
- **[Details](./01-maintenance.md)** ✅ COMPLETE

## Phase 2: Debugging & Tools (High Utility)
*Focus: Tools that make future work easier and fixing high-visibility bugs.*
- **[Details](./02-debug-tools.md)** ⏳ IN PROGRESS
- **Completed:** FPS Controls (1 SP) ✅ + RTS Camera (2-3 SP) ✅
- **Current:** HUD Debug Info (1-2 SP) ⏳
- **Remaining:** Fix Lights (2-3 SP) ⏳
- **Sequenced approach:** FPS Controls ✅ → RTS Camera ✅ → HUD Debug → Fix Lights
- **Rationale:** Better controls and cameras make light debugging significantly easier.

## Phase 3: UI & Settings (Isolated Systems)
*Focus: Polish that doesn't touch the simulation core.*
- **[Details](./03-ui-polish.md)**

## Phase 4: Physics Integration (Rapier3D)
*Focus: The heavy lifting for simulation.*
- **[Details](./04-physics.md)**

## Phase 5: Terrain Overhaul (Research Spike)
*Focus: Researching high-fidelity voxel terrain architectures.*
- **[Details](./05-terrain-overhaul.md)**

## Phase 6: Gameplay Loops
*Focus: Mechanics that rely on the systems built above.*
- **[Details](./06-gameplay-systems.md)**

---

## Tech Debt Foundation (Completed)
- **[Details](./07-tech-debt-sprint.md)** ✅ COMPLETE
- Completed 13 SP of foundational cleanups before resuming feature work.
