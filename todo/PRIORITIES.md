# Development Priorities

**Last Updated:** October 26, 2025  
**Current Branch:** ui-refactor  
**Status:** Planning Phase

---

## Priority Order

### 1. **Event Bus/Manager** 🔴 HIGHEST PRIORITY
**Status:** In Progress (Implementation Plan Complete)  
**Estimate:** 5 SP  
**Blockers:** None  
**Blocks:** HUD overlay (debug console), multiplayer foundation

**Goals:**
- Refactor existing code to standardize and centralize event-driven patterns
- Replace ad-hoc crossbeam channels with unified event architecture
- Build foundation for multiplayer event synchronization
- Enable clean separation of concerns across systems

**Dependencies:** None (foundational)

**Implementation Plan:** See `EVENT_BUS.md`

**Related Files:**
- `src/main.rs` - Current event handling via channels
- `moho_ui/src/adapter.rs` - UI event system
- `moho_audio/src/audio_events.rs` - Audio event patterns

---

### 2. **HUD Overlay** 🟡 HIGH PRIORITY
**Status:** Not Started (blocked by event bus)  
**Estimate:** 2 SP  
**Blockers:** Phase 3 overlay infrastructure  
**Blocks:** Debug console, gameplay HUD

**Sub-Components:**

#### 2a. Debug Console Prototype
- Command input/output
- Command history
- Initial commands: `save`, `help`, `clear`
- Feature-gated behind config flag

#### 2b. Fake HUD Elements
- Minimap placeholder
- Health bar placeholder
- Score/stats placeholder
- Test overlay rendering and z-ordering

#### 2c. Real Debug Elements
- Player coordinates (world space)
- Camera orientation (yaw/pitch)
- Frame rate/performance stats
- Current biome/chunk info

**Dependencies:**
- Event bus (for command feedback)
- Phase 3 overlay infrastructure (from `OVERLAY_SUPPORT.md`)

---

### 3. **Basic Physics** 🟡 HIGH PRIORITY
**Status:** Not Started  
**Estimate:** 3 SP  
**Blockers:** Physics library selection  
**Blocks:** Player movement, collision detection, falling objects

**Components:**

#### 3a. Physics Integration
- Evaluate physics libraries (rapier, bevy_rapier, physx-rs)
- Integrate chosen library into `moho_core`
- Create physics system wrapper

#### 3b. Player Physics Pawn
- Replace camera-only player with physics-enabled pawn
- Collision mesh for player
- Movement controller with physics
- Jump mechanics

#### 3c. Voxel Physics
- Generate collision meshes from voxel data
- Static voxel world collision
- Chunk-based collision updates

#### 3d. Basic Gravity
- Gravity simulation
- Ground detection
- Fall damage (if applicable)

**Dependencies:**
- Voxel system (`moho_core`)
- Camera/controller system (`moho_sim`)

**Related Files:**
- `moho_core/src/voxel.rs`
- `moho_sim/src/simulation.rs`
- `moho_core/src/controller.rs`

---

### 4. **Graphics Additions** 🟢 MEDIUM PRIORITY
**Status:** Not Started  
**Estimate:** 5 SP  
**Blockers:** Rendering pipeline understanding  
**Blocks:** Advanced visual effects

**Components:**

#### 4a. World Lighting
- Moveable directional light (sun)
- Time-of-day system
- Dynamic light position/color
- Shadow casting from sun

#### 4b. Rendering Settings
- Anti-aliasing options (MSAA, FXAA, TAA)
- Shadow quality settings
- Shadow distance/resolution controls
- Settings UI integration

**Dependencies:**
- `moho_renderer` rendering pipeline
- Settings system (`moho_ui/src/prefs.rs`)
- Shader system (`shaders/`)

**Related Files:**
- `moho_renderer/src/lib.rs`
- `shaders/fragment.wgsl`
- `moho_ui/src/screens/settings.rs`

---

### 5. **Complex World Generation** 🟢 MEDIUM PRIORITY
**Status:** Not Started  
**Estimate:** 8 SP  
**Blockers:** None (iterative improvements)  
**Blocks:** Biome diversity, caves exploration

**Components:**

#### 5a. Terrain Smoothing
- Interpolation algorithms
- Smooth terrain transitions
- Reduce blockiness

#### 5b. Cave Systems
- 3D noise-based cave generation
- Cave entrances/exits
- Stalactites/stalagmites

#### 5c. Material Diversity
- Multiple voxel types (stone, dirt, sand, grass, etc.)
- Material-based generation (ore veins, etc.)
- Texture mapping per material

#### 5d. Liquids/Water (Advanced - Revisit)
- Water voxel type
- Water physics (flow, settling)
- Water rendering (transparency, reflections)
- **Note:** Complex feature, deprioritize initially

**Dependencies:**
- `moho_core/src/scene_builders.rs`
- Voxel system
- Noise generation libraries

**Related Files:**
- `moho_core/src/scene_builders.rs`
- `moho_core/src/voxel.rs`

---

### 6. **Peer-to-Peer Multiplayer** 🔵 LONG-TERM GOAL
**Status:** Planning (see `MULTIPLAYER_PLAN.md`)  
**Estimate:** 13 SP  
**Blockers:** Event bus, deterministic simulation  
**Blocks:** Online gameplay

**Major Components:**
- Network protocol design
- P2P connection management
- State synchronization
- Event replication
- Conflict resolution
- Client prediction
- Server reconciliation

**Critical Dependencies:**
- **Event bus** (50% of multiplayer foundation)
- Deterministic simulation (`moho_sim`)
- Input recording/playback
- State serialization

**Related Documents:**
- `MULTIPLAYER_PLAN.md`
- `moho_sim/` (deterministic simulation layer)

---

## Dependency Graph

```
Event Bus (Priority 1)
    ↓
    ├─→ HUD Overlay (Priority 2)
    │       ↓
    │       └─→ Debug Console
    │
    ├─→ Multiplayer (Priority 6) ─┐
    │                              │
    └─→ (All future features)      │
                                   │
Basic Physics (Priority 3) ────────┤
                                   │
Graphics (Priority 4) ─────────────┤
                                   │
World Gen (Priority 5) ────────────┘
```

---

## Implementation Strategy

### Recommended Order (Optimized for Efficiency)

The most efficient implementation path based on dependencies and ROI:

```
Phase 1-2: Event Bus (Priority 1)
    ↓
Phase 3: HUD Overlay Infrastructure (Priority 2a - overlay system only)
    ↓
Phase 4-6: Basic Physics (Priority 3)
    ↓
Phase 7: HUD Overlay Complete (Priority 2b/2c - actual overlays)
    ↓
Phase 8-11: Graphics (Priority 4) ← Can parallelize with World Gen
    ↓
Phase 12-17: World Gen (Priority 5) ← Can parallelize with Graphics
    ↓
Phase 18-29: Multiplayer (Priority 6)
```

**Rationale for Reordering:**
1. **Event Bus first** - Force multiplier for everything else
2. **HUD overlay infrastructure early** - Helps with debugging physics/graphics
3. **Physics before HUD content** - Need real data to display in HUD
4. **Graphics + World Gen in parallel** - Independent systems, can work simultaneously
5. **Multiplayer last** - Requires all other systems to be solid

### Phase 1: Foundation (recommended)
1. **Event Bus Implementation** ← START HERE
   - Core event bus system
   - Event type definitions
   - Integration with existing systems
   - Testing and validation

### Phase 2: Immediate Features (recommended)
2. **HUD Overlay Infrastructure Only**
   - Phase 3 overlay infrastructure (from OVERLAY_SUPPORT.md)
   - Overlay rendering system
   - Z-ordering support
   - Input blocking logic
   - **Note:** Actual overlays (debug console, HUD elements) come after physics

3. **Basic Physics** (Start in parallel with overlay infrastructure)
   - Physics library integration
   - Player physics pawn
   - Voxel collision
   - Gravity system

### Phase 3: Core Gameplay + HUD Content (recommended)
4. **Complete HUD Overlays**
   - Debug console (now has physics data to display)
   - Real debug info (coordinates, velocity, collision state)
   - Performance stats
   - Fake HUD elements for testing

### Phase 4: Visual Polish (recommended)
5. **Graphics Enhancements** (Can work in parallel with World Gen)
   - World lighting system
   - Rendering settings
   - Shadow system
   - Settings UI

### Phase 5: Content Expansion (recommended)
6. **Advanced World Generation** (Can work in parallel with Graphics)
   - Terrain smoothing
   - Cave systems
   - Material diversity
   - (Water/liquids - optional)

### Phase 6: Multiplayer (recommended)
7. **Peer-to-Peer Multiplayer**
   - Network architecture
   - P2P connections
   - State synchronization
   - Testing and refinement

---

## Critical Path Analysis

**To achieve multiplayer (end goal) most efficiently:**

**Recommended Path (Optimized):**
```
Event Bus (Phase 1-2)
    ↓
HUD Overlay Infrastructure (Phase 3)
    ↓
Physics (Phase 4-6) → Creates real data for HUD
    ↓
Complete HUD Overlays (Phase 7) → Now displays real physics data
    ↓
Graphics + World Gen (Phase 8-17) → Parallel development
    ↓
Multiplayer (Phase 18-29) → All systems mature
```

**Why This Order is Better:**

1. **Event Bus First** - Mandatory foundation
2. **Overlay Infrastructure Early** - Debugging aid for all following work
3. **Physics Before HUD Content** - HUD needs real data to be useful
   - Coordinates without physics = meaningless
   - Velocity/collision data requires physics
   - Debug console is more useful with physics commands
4. **Graphics/World Gen Parallel** - Independent systems, maximize efficiency
5. **Multiplayer Last** - Needs stable, tested foundation

**Time Savings (informational):**
- Building HUD with physics data vs dummy data: small saved effort (avoids rework)
- Parallel Graphics+World Gen: reduced calendar time via parallel work
- Total optimization: reduced schedule pressure (informational)

---

## Current Status: Event Bus Planning

**Decision Point:** Build "Battle-Ready" event bus now vs "Minimal" event bus

**Recommendation:** Battle-Ready Event Bus
- **Estimate:** 3 SP
- **Benefit:** Foundation for all future features
- **ROI:** Saves significant future effort (estimated ~13 SP)
- **Multiplayer:** Provides 50% of MP foundation

**Next Steps:**
1. Create detailed event bus implementation plan
2. Define core event types
3. Implement event bus foundation
4. Refactor existing systems
5. Test and validate

---

## Notes

- All estimates assume single developer, full-time equivalent
- Estimates may vary based on complexity and scope creep
- Multiplayer is longest/most complex feature
- Event bus is force multiplier for all other features
- Physics and world gen can be developed in parallel after event bus
- HUD overlay enhances development experience for all subsequent work

---

## Success Criteria

### Event Bus ✓
- [ ] Unified event system operational
- [ ] All existing channels migrated
- [ ] Type-safe event publishing/subscribing
- [ ] Performance metrics acceptable
- [ ] Documentation complete

### HUD Overlay ✓
- [ ] Debug console functional with basic commands
- [ ] Debug info displayed (coordinates, FPS, etc.)
- [ ] Overlay rendering with proper z-ordering
- [ ] No interference with gameplay

### Basic Physics ✓
- [ ] Player has physics-enabled pawn
- [ ] Collision with voxel terrain works
- [ ] Gravity simulation functional
- [ ] Movement feels responsive

### Graphics ✓
- [ ] Dynamic lighting system operational
- [ ] Shadows rendering correctly
- [ ] AA/graphics settings functional
- [ ] Performance impact acceptable

### World Gen ✓
- [ ] Smoother terrain generation
- [ ] Cave systems generate properly
- [ ] Multiple material types
- [ ] Interesting/varied worlds

### Multiplayer ✓
- [ ] P2P connections stable
- [ ] State synchronization working
- [ ] Low latency gameplay
- [ ] 2-4 players supported
- [ ] No major desync issues

---

**End of Priorities Document**
