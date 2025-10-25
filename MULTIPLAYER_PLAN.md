# ARCHIVE: MULTIPLAYER_PLAN

Archived 2025-10-24. Multiplayer planning notes have been archived. Multiplayer is out of scope for the current voxel rendering cleanup; keep this in VCS history if needed.

### Strengths for Multiplayer
- ✅ **ECS-based architecture** (Legion) provides clean entity/component separation
- ✅ **Modular design** with separate `engine_core`, `engine_renderer`, and `moho_ui` crates
- ✅ **Scene serialization** already implemented with bincode
- ✅ **Clean separation** between game logic and rendering
- ✅ **Abstracted input system** ready for network adaptation

### Current Limitations
- ❌ **Tightly coupled application loop** - Input → simulation → render in single thread
- ❌ **Single player assumption** - One hardcoded `PlayerController`
- ❌ **Direct input processing** - No discrete command abstraction
- ❌ **No network layer** - No communication infrastructure

## Foundation Changes Required

### 1. Game Logic Separation
**Problem**: Game simulation tightly coupled with presentation in `main.rs`

**Solution**: Extract pure game logic into separate module
```rust
pub struct GameSimulation {
    world: World,
    scene: Scene,
    players: HashMap<PlayerId, PlayerController>,
}

impl GameSimulation {
    pub fn tick(&mut self, inputs: &[PlayerInput], dt: f32) -> GameStateUpdate {
        // Pure game logic - no rendering dependencies
    }
}
```

### 2. Input Command Abstraction  
**Problem**: Continuous input directly modifies game state

**Solution**: Discrete input commands that can be serialized and networked
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInput {
    pub player_id: PlayerId,
    pub sequence: u32,
    pub timestamp: f32,
    pub actions: Vec<InputAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InputAction {
    Move { forward: f32, right: f32, up: f32 },
    Look { yaw_delta: f32, pitch_delta: f32 },
    Interact { target: Option<EntityId> },
    // Future: Shoot, Build, etc.
}
```

### 3. Multi-Player Support
**Problem**: Single hardcoded player controller

**Solution**: Player ID concept with multiple controllers
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u32);

#[derive(Clone, Copy, Debug)]
pub struct NetworkedEntity {
    pub owner: PlayerId,
    pub last_update: f32,
}
```

### 4. Incremental State Updates
**Problem**: Scene assumes full state access every frame

**Solution**: Delta updates for network efficiency
```rust
pub enum GameStateUpdate {
    EntitySpawned { id: EntityId, components: EntityData },
    EntityUpdated { id: EntityId, changes: ComponentChanges },
    EntityDestroyed { id: EntityId },
    PlayerJoined { player_id: PlayerId, spawn_point: Vec3 },
    PlayerLeft { player_id: PlayerId },
}
```

## Refactoring Plan

### Phase 1: Extract Game Simulation (1-2 weeks)
**Goal**: Separate game logic from presentation layer

1. **Create `game_simulation` crate** - New workspace member for pure game logic
2. **Move ECS world management** - Extract world/scene handling from `main.rs`
3. **Create `GameSimulation::tick()`** - Pure function: inputs → state updates
4. **Update `main.rs`** - Becomes thin presentation layer calling simulation
5. **Verify functionality** - Ensure single-player behavior unchanged

**Files Modified**:
- `Cargo.toml` - Add workspace member
- `src/main.rs` - Extract game logic
- `game_simulation/src/lib.rs` - New crate with game logic

### Phase 2: Input Abstraction (1 week)
**Goal**: Convert continuous input to discrete commands

1. **Create `PlayerInput` types** - Command-based input instead of continuous
2. **Update input collection** - Accumulate commands per frame
3. **Add player ID infrastructure** - Single player uses ID 0
4. **Test input recording** - Validate command abstraction works

**Files Modified**:
- `engine_core/src/controller.rs` - Add player ID support
- `game_simulation/src/input.rs` - New input command types
- `src/main.rs` - Use command-based input

### Phase 3: Multi-Player Foundation (1 week)  
**Goal**: Support multiple players in single instance

1. **Multiple `PlayerController`s** - Store as ECS components with player IDs
2. **Update camera system** - Follow local player (player 0)
3. **Player lifecycle management** - Spawn/despawn players dynamically
4. **Test with simulated players** - Add keyboard "player 2" or simple bot

**Files Modified**:
- `engine_core/src/actors.rs` - Add player entities
- `engine_core/src/camera.rs` - Multi-player camera handling
- `game_simulation/src/player.rs` - Player management

### Phase 4: State Serialization (1 week)
**Goal**: Prepare for network state synchronization

1. **Extend scene serialization** - Include all dynamic game state
2. **Implement delta updates** - Track frame-to-frame changes
3. **Add state checkpointing** - Save/restore for testing
4. **Validate determinism** - Same inputs = same outputs

**Files Modified**:
- `engine_renderer/src/scene.rs` - Extended serialization
- `game_simulation/src/state.rs` - Delta update system

## Recommended Workspace Structure

```
moho/
├── Cargo.toml                 # Workspace root (add game_simulation member)
├── engine_core/               # Core game types (unchanged)
├── engine_renderer/           # Rendering system (unchanged)  
├── moho_ui/                  # UI system (unchanged)
├── game_simulation/          # NEW: Pure game logic
│   ├── src/
│   │   ├── lib.rs            # GameSimulation struct
│   │   ├── input.rs          # PlayerInput, InputAction types
│   │   ├── player.rs         # Multi-player support
│   │   ├── state.rs          # State updates and serialization
│   │   └── world.rs          # ECS world management
│   └── Cargo.toml
└── src/
    └── main.rs               # Thin presentation layer
```

## Future Networking Implementation

When ready to implement actual multiplayer (estimated 4-6 weeks):

### Dependencies to Add
```toml
# Add to game_simulation crate when implementing networking
tokio = { version = "1.0", features = ["net", "rt-multi-thread"] }
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
```

### Implementation Steps
1. **Week 1**: Basic TCP networking with `tokio`
2. **Week 2**: Host selection and client connection handling  
3. **Week 3**: Input forwarding from clients to host
4. **Week 4**: State broadcasting from host to clients
5. **Week 5**: Lag compensation and interpolation
6. **Week 6**: Error handling, reconnection, polish

## Alternative Architectures Considered

### True Peer-to-Peer
- **Pros**: No single point of failure, fair latency
- **Cons**: Complex synchronization, requires deterministic simulation
- **Verdict**: More complex for minimal benefit given use case

### Hybrid Client-Server
- **Pros**: Flexible authoritative vs client-side decisions  
- **Cons**: Complex to design, risk of desynchronization
- **Verdict**: Overkill without anti-cheat requirements

## Estimated Implementation Timeline

### Foundation Only (Recommended First)
- **Phase 1-4**: 4-5 weeks total
- **Benefit**: Clean architecture, easier testing, reduced tech debt
- **Risk**: Low - improves code quality regardless of multiplayer

### Complete Multiplayer Implementation  
- **Foundation + Networking**: 8-11 weeks total
- **Production Polish**: Additional 2-4 weeks
- **Advanced Features**: Additional 2-4 weeks (prediction, migration, etc.)

## Success Metrics

### Foundation Refactoring
- ✅ Single-player functionality unchanged
- ✅ Game logic cleanly separated from presentation
- ✅ Input system supports discrete commands
- ✅ Multiple players can exist in single game instance
- ✅ Game state can be serialized/deserialized

### Multiplayer Implementation
- ✅ 2+ players can connect and play together
- ✅ Host migration or graceful disconnection handling
- ✅ Acceptable latency for target game types
- ✅ Stable connections for 30+ minute sessions
- ✅ Easy setup for end users

---

## Notes

This plan prioritizes simplicity and incremental progress over complex enterprise solutions. The foundation refactoring provides value immediately through better code organization and testing, while enabling future multiplayer implementation without major architectural changes.

The host-client model is recommended for its simplicity and suitability for the target use case (small friend groups, cooperative gameplay, no cheating concerns).