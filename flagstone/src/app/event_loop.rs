//! Event Loop Logic
//!
//! This module contains the per-tick logic of App, split into
//! logical components:
//! - Game state updates and per-frame lighting
//! - Event processing (UI, graphics, world, input, debug)
//! - World generation polling
//!
//! Each component is designed to be testable in isolation while maintaining
//! a clean separation of concerns.

pub mod event_processor;
pub mod frame_processor;
pub mod generation_processor;

pub use event_processor::EventProcessor;
pub use frame_processor::FrameProcessor;
pub use generation_processor::GenerationProcessor;
