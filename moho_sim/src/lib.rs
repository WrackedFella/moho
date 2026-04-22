//! Minimal simulation crate used as a headless target for PlayerInput and determinism tests.
//!
//! This crate provides a tiny `PlayerInput` type and a trivial `Simulation` harness
//! to run deterministic ticks. It is intentionally small so tests can be written
//! and run before the full extraction of the application's simulation.

use bincode::{Decode, Encode};

/// A compact discrete player input used for tests and initial wiring.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PlayerInput {
    /// Move by integer delta in grid space
    Move { dx: i32, dy: i32 },
    /// A generic action with a small payload
    Action(u8),
}

/// Very small headless simulation: keeps a single actor position and applies inputs.
#[derive(Debug, Clone, Encode, Decode)]
pub struct Simulation {
    pub x: i32,
    pub y: i32,
    seed: u64,
}

impl Simulation {
    /// Create a new simulation with deterministic seed
    pub fn new(seed: u64) -> Self {
        Self { x: 0, y: 0, seed }
    }

    /// Apply a tick worth of player inputs
    pub fn tick(&mut self, inputs: &[PlayerInput]) {
        for i in inputs {
            match i {
                PlayerInput::Move { dx, dy } => {
                    self.x += dx;
                    self.y += dy;
                }
                PlayerInput::Action(_a) => {
                    // no-op for now; present to test serialization and routing
                }
            }
        }
        // incorporate seed determinism: fold seed bits into position to ensure
        // identical seeds produce identical results when inputs are equal
        self.x = self.x.wrapping_add((self.seed & 0xffff) as i32);
        self.y = self.y.wrapping_add(((self.seed >> 16) & 0xffff) as i32);
    }

    /// Apply a slice of time-stamped inputs.
    pub fn tick_timed(&mut self, timed: &[TimedInput]) {
        let inputs: Vec<PlayerInput> = timed.iter().map(|t| t.input.clone()).collect();
        self.tick(&inputs);
    }

    /// Snapshot the simulation state as bincode bytes.
    pub fn snapshot(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::encode_to_vec(self, bincode::config::standard())
    }

    /// Restore a simulation from bincode bytes produced by [`snapshot`](Self::snapshot).
    pub fn restore(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (sim, _) = bincode::decode_from_slice(bytes, bincode::config::standard())?;
        Ok(sim)
    }
}

// Re-export for convenient use in tests
pub use PlayerInput as PlayerInputType;

mod input_map;
pub use input_map::{ContinuousState, TimedInput, map_to_player_inputs, stamp_inputs};
mod controller_adapter;
pub use controller_adapter::apply_controller_tick;
mod simulation;
pub use simulation::SimulationController;
