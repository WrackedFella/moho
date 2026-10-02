# moho_sim

Deterministic headless simulation system for the Moho engine. Provides snapshot/restore functionality, input mapping, and frame-locked simulation stepping for multiplayer and replay support.

## Purpose

This crate separates game simulation logic from rendering and platform code, enabling:
- **Deterministic execution** - Same inputs + same seed = identical output
- **Snapshot/restore** - Save and restore complete simulation state
- **Multiplayer foundations** - Input replay and state synchronization
- **Headless testing** - Run simulations without GPU/window
- **Frame-locked updates** - Consistent tick rate regardless of rendering FPS

## Core Components

### PlayerInput

Discrete input representation for deterministic simulation:

```rust
use moho_sim::PlayerInput;

// Movement input
let input = PlayerInput::Move { dx: 1, dy: 0 }; // Move right

// Generic action
let input = PlayerInput::Action(42); // Action with payload
```

### Simulation

Minimal simulation harness for testing determinism:

```rust
use moho_sim::Simulation;

let mut sim = Simulation::new(12345); // Deterministic seed

// Apply inputs
sim.tick(&[
    PlayerInput::Move { dx: 1, dy: 0 },
    PlayerInput::Move { dx: 0, dy: 1 },
]);

// Check state
assert_eq!(sim.x, 1);
assert_eq!(sim.y, 1);
```

### SimulationController

Higher-level controller with GameClock integration:

```rust
use moho_sim::SimulationController;
use moho_core::game_clock::GameClock;

let clock = GameClock::new(600.0, 420.0);
let mut controller = SimulationController::new(clock);

// Update each frame
controller.tick(delta_time);

// Access clock state
let time = controller.clock().time_of_day();
```

## Snapshot and Restore

Complete state serialization for save games and multiplayer sync:

```rust
use moho_sim::Simulation;

let mut sim = Simulation::new(12345);
sim.tick(&[PlayerInput::Move { dx: 5, dy: 3 }]);

// Save state
let snapshot = sim.snapshot()?; // Vec<u8>

// Restore state
let restored = Simulation::restore(&snapshot)?;
assert_eq!(restored.x, sim.x);
assert_eq!(restored.y, sim.y);
```

Snapshots use `serde_json` for human-readable debugging. Can switch to `bincode` for production efficiency.

## Input Mapping

Convert continuous controller state to discrete simulation inputs:

```rust
use moho_sim::{ContinuousState, map_to_player_inputs};

let controller_state = ContinuousState {
    move_x: 0.8,  // Right
    move_y: 0.0,
    action_pressed: false,
};

// Map analog input to discrete moves
let inputs = map_to_player_inputs(&controller_state, 0.5); // 0.5 = deadzone

// Produces: PlayerInput::Move { dx: 1, dy: 0 }
```

## Timed Inputs

Associate inputs with frame numbers for replay systems:

```rust
use moho_sim::{TimedInput, stamp_inputs};

let inputs = vec![
    PlayerInput::Move { dx: 1, dy: 0 },
    PlayerInput::Action(5),
];

// Stamp with frame number
let timed = stamp_inputs(&inputs, 42); // Frame 42

// Apply to simulation
sim.tick_timed(&timed);
```

## Determinism

The simulation is fully deterministic when:
1. Same seed is used
2. Same inputs applied in same order
3. No external randomness (time, I/O, floating point non-determinism)

**Determinism Strategy:**
- Seed is incorporated into state updates (`x += seed_bits`)
- All operations use integer math (no floating point)
- No system time or random number generation
- State is completely serializable

**Example:**
```rust
let mut sim1 = Simulation::new(12345);
let mut sim2 = Simulation::new(12345);

let inputs = vec![PlayerInput::Move { dx: 3, dy: 2 }];

sim1.tick(&inputs);
sim2.tick(&inputs);

assert_eq!(sim1.x, sim2.x); // ✅ Deterministic
assert_eq!(sim1.y, sim2.y); // ✅ Deterministic
```

## Controller Adaptation

Bridge between raw input events and simulation:

```rust
use moho_sim::apply_controller_tick;

// Apply controller state changes
apply_controller_tick(&mut controller_state, delta_time);

// Convert to discrete inputs for simulation
let player_inputs = map_to_player_inputs(&controller_state, 0.3);
```

## Testing

The crate includes extensive determinism and serialization tests:

```bash
# Run all tests
cargo test --package moho_sim

# Determinism test
cargo test --test determinism

# Snapshot roundtrip test
cargo test --test snapshot_roundtrip

# Input mapping tests
cargo test --test input_mapping
```

**Test Coverage:**
- Determinism validation (same seed + inputs = same result)
- Snapshot/restore roundtrip
- Input mapping from analog to discrete
- Timed input application
- CRC validation for corruption detection

## Integration with Main Engine

The simulation crate is designed to be integrated with the main engine's ECS and world systems:

```rust
// In main game loop
let inputs = collect_player_inputs(&input_system);
simulation.tick(&inputs);

// Sync rendering to simulation state
let world_state = simulation.export_world_state();
renderer.update_entities(&world_state);
```

## Multiplayer Considerations

This crate provides foundations for networked multiplayer:

**Lockstep Model:**
1. Collect inputs from all players
2. Exchange inputs over network
3. Each client runs identical simulation with all inputs
4. State stays synchronized

**State Synchronization:**
1. Periodic snapshot exchange
2. CRC validation to detect desync
3. Rollback and replay if desync detected

**Replay System:**
1. Record timed inputs
2. Store initial seed
3. Replay inputs from beginning to reproduce exact game

## Thread Safety

- **`Simulation`**: Not thread-safe (single-threaded updates)
- **`PlayerInput`**: `Send + Sync` (can be transferred between threads)
- **Snapshots**: `Send + Sync` (Vec<u8> can be moved between threads)

Design assumes single-threaded simulation updates with multi-threaded input collection.

## Performance

- **Tick Rate**: Handles 60Hz updates with minimal overhead
- **Snapshot Size**: ~100 bytes for simple simulation state
- **Serialization**: <1ms for typical snapshots
- **Input Processing**: <0.1ms per frame for 10 inputs

## Future Plans

- Full ECS integration (Legion/Bevy)
- Physics simulation (collision, velocity, forces)
- Network protocol for input exchange
- Rollback netcode support
- Larger world state (chunk streaming)
- Floating point determinism (if needed for physics)
