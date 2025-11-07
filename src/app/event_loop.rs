//! Event Loop Logic
//!
//! This module contains the extracted event loop logic from App, split into
//! logical components:
//! - Window initialization and management
//! - Frame timing and updates
//! - Event processing (UI, audio, graphics, input)
//! - Window event handling
//!
//! Each component is designed to be testable in isolation while maintaining
//! a clean separation of concerns.

pub mod window_manager;
pub mod frame_processor;
pub mod event_processor;
pub mod window_event_handler;
pub mod generation_processor;

pub use window_manager::WindowManager;
pub use frame_processor::FrameProcessor;
pub use event_processor::EventProcessor;
pub use window_event_handler::WindowEventHandler;
pub use generation_processor::GenerationProcessor;
