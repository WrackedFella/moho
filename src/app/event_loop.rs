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

pub mod event_processor;
pub mod frame_processor;
pub mod generation_processor;
pub mod window_event_handler;
pub mod window_manager;

pub use event_processor::EventProcessor;
pub use frame_processor::FrameProcessor;
pub use generation_processor::GenerationProcessor;
pub use window_event_handler::WindowEventHandler;
pub use window_manager::WindowManager;
