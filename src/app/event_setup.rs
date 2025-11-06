//! Event bus initialization and subscriber setup.
//!
//! This module encapsulates the creation of the event bus and setup of event subscribers.
//! The event bus follows a pub-sub pattern where:
//! - Publishers emit events to the bus
//! - Subscribers receive events via channels
//! - Events that need to mutate App state are collected via crossbeam channels
//!
//! # Architecture
//!
//! The event bus has three main subscriber types:
//! 1. **UI Events** (feature-gated) - Menu interactions, console commands, etc.
//! 2. **Audio Events** - Sound effects, music control, volume changes
//! 3. **Graphics Events** - Renderer configuration, material updates, etc.
//!
//! Each subscriber gets its own channel to avoid blocking. Events are processed
//! in the main event loop, allowing them to safely mutate App state.

use crossbeam_channel::{Receiver, unbounded};
use moho_core::EventBus;
use std::sync::Arc;

/// Result of event bus setup containing the bus and all subscriber receivers.
pub struct EventBusSetup {
    /// The event bus itself (shared via Arc for thread-safe publishing)
    pub event_bus: Arc<EventBus>,

    /// Receiver for UI events (only available with ui-egui feature)
    #[cfg(feature = "ui-egui")]
    pub ui_event_rx: Receiver<moho_core::events::UiEvent>,

    /// Receiver for audio events
    pub audio_event_rx: Receiver<moho_core::events::AudioEvent>,

    /// Receiver for graphics events
    pub graphics_event_rx: Receiver<moho_core::events::GraphicsEvent>,
}

/// Initialize the event bus and set up all event subscribers.
///
/// This function:
/// 1. Creates a new EventBus instance
/// 2. Creates unbounded channels for each event type
/// 3. Subscribes to the bus with closures that forward events to channels
/// 4. Returns the bus and all receiver channels
///
/// # Feature Flags
///
/// - `ui-egui`: Enables UI event subscription (menu, console, etc.)
///
/// # Examples
///
/// ```no_run
/// use moho::app::event_setup::setup_event_bus;
///
/// let setup = setup_event_bus();
/// 
/// // Use the event bus for publishing
/// // let event_bus = setup.event_bus;
///
/// // Process events in the main loop
/// // while let Ok(event) = setup.audio_event_rx.try_recv() { ... }
/// ```
pub fn setup_event_bus() -> EventBusSetup {
    // Create the event bus
    let event_bus = Arc::new(EventBus::new());
    log::info!("Event bus initialized");

    // Create channels for event collection
    #[cfg(feature = "ui-egui")]
    let (ui_event_tx, ui_event_rx) = unbounded::<moho_core::events::UiEvent>();
    
    let (audio_event_tx, audio_event_rx) = unbounded::<moho_core::events::AudioEvent>();
    let (graphics_event_tx, graphics_event_rx) = unbounded::<moho_core::events::GraphicsEvent>();

    // Subscribe to UI events (feature-gated)
    #[cfg(feature = "ui-egui")]
    {
        let bus = event_bus.clone();
        bus.subscribe(move |event: &moho_core::events::UiEvent| {
            // Forward events to the channel for processing in main loop
            let _ = ui_event_tx.send(event.clone());
        });
    }

    // Subscribe to Audio events
    {
        let bus = event_bus.clone();
        bus.subscribe(move |event: &moho_core::events::AudioEvent| {
            let _ = audio_event_tx.send(event.clone());
        });
    }

    // Subscribe to Graphics events
    {
        let bus = event_bus.clone();
        bus.subscribe(move |event: &moho_core::events::GraphicsEvent| {
            let _ = graphics_event_tx.send(event.clone());
        });
    }

    EventBusSetup {
        event_bus,
        #[cfg(feature = "ui-egui")]
        ui_event_rx,
        audio_event_rx,
        graphics_event_rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_setup_creates_bus() {
        let setup = setup_event_bus();
        
        // Verify the bus exists and is usable
        assert!(Arc::strong_count(&setup.event_bus) >= 1);
    }

    #[test]
    fn test_audio_event_subscription() {
        let setup = setup_event_bus();
        
        // Publish an audio event
        setup.event_bus.publish(moho_core::events::AudioEvent::PlaySound {
            path: "test.wav".to_string(),
            volume: 1.0,
        });

        // Should receive it on the channel
        let received = setup.audio_event_rx.try_recv();
        assert!(received.is_ok(), "Should receive audio event");
    }

    #[test]
    fn test_graphics_event_subscription() {
        let setup = setup_event_bus();
        
        // Publish a graphics event
        setup.event_bus.publish(moho_core::events::GraphicsEvent::TimeOfDayChanged {
            time: 12.0,
            sun_angle: 0.5,
        });

        // Should receive it on the channel
        let received = setup.graphics_event_rx.try_recv();
        assert!(received.is_ok(), "Should receive graphics event");
    }

    #[cfg(feature = "ui-egui")]
    #[test]
    fn test_ui_event_subscription() {
        let setup = setup_event_bus();
        
        // Publish a UI event
        setup.event_bus.publish(moho_core::events::UiEvent::MenuShown {
            name: "TestMenu".to_string(),
        });

        // Should receive it on the channel
        let received = setup.ui_event_rx.try_recv();
        assert!(received.is_ok(), "Should receive UI event");
    }

    #[test]
    fn test_channels_are_initially_empty() {
        let setup = setup_event_bus();
        
        // Channels should start empty
        assert!(setup.audio_event_rx.is_empty());
        assert!(setup.graphics_event_rx.is_empty());
        
        #[cfg(feature = "ui-egui")]
        assert!(setup.ui_event_rx.is_empty());
    }
}
