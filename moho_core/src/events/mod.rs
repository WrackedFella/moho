//! Event bus system for application-wide event distribution
//!
//! This module provides a type-safe, thread-safe event bus for
//! decoupling systems and enabling event-driven architecture.
//!
//! # Overview
//!
//! The event bus allows different parts of the application to communicate
//! without tight coupling. Systems can publish events and subscribe to
//! events they're interested in, creating a clean separation of concerns.
//!
//! # Features
//!
//! - **Type-safe**: Events are strongly typed at compile time
//! - **Thread-safe**: Safe to use across multiple threads
//! - **Priority-based**: Handlers execute in priority order
//! - **Deferred processing**: Option to defer events until end of frame
//! - **Event history**: Debug buffer for event replay
//! - **Performance metrics**: Built-in performance tracking
//!
//! # Example
//!
//! ```no_run
//! use moho_core::events::{EventBus, Event};
//! use std::any::Any;
//!
//! // Define an event
//! #[derive(Clone, Debug)]
//! struct PlayerMoved {
//!     position: glam::Vec3,
//! }
//!
//! impl Event for PlayerMoved {
//!     fn as_any(&self) -> &dyn Any {
//!         self
//!     }
//! }
//!
//! // Create event bus
//! let bus = EventBus::new();
//!
//! // Subscribe to events
//! bus.subscribe(|event: &PlayerMoved| {
//!     println!("Player moved to: {:?}", event.position);
//! });
//!
//! // Publish events
//! bus.publish(PlayerMoved {
//!     position: glam::Vec3::new(1.0, 2.0, 3.0),
//! });
//! ```

mod bus;
mod handler;
mod metrics;
mod types;

pub use bus::EventBus;
pub use metrics::{EventMetrics, MetricsTracker};
pub use types::Event;

// Re-export handler types for advanced usage
pub use handler::{Handler, HandlerFn, HandlerList};

// Re-export all event types
pub use types::{
    // Graphics events
    AntiAliasingMode,
    // Audio events
    AudioEvent,
    // World events
    BiomeType,
    // Debug events
    ConsoleLevel,
    DebugEvent,
    // Game events
    GameEvent,
    GraphicsEvent,
    GraphicsSetting,
    // Input events
    InputEvent,
    MaterialType,
    // Network events
    NetworkEvent,
    OreType,
    // Physics events
    PhysicsEvent,
    RenderMode,
    ShadowQuality,
    // System events
    SystemEvent,
    // UI events
    UiEvent,
    WorldEvent,
};

#[cfg(test)]
mod tests;
