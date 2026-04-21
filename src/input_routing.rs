//! Input routing system for the Moho engine.
//!
//! Provides a priority-based input routing system that dispatches
//! input events to different layers based on the current game state.
//! Higher-priority layers receive input first and can consume events
//! to prevent lower-priority layers from receiving them.
//!
//! # Status
//!
//! Layer tracking via [`InputRouter::update_for_state`] is integrated.
//! Full event dispatch through [`InputRouter::dispatch`] is not yet
//! wired in — input currently flows through `InputDispatcher`.

use crate::game_state::GameState;
use std::collections::HashMap;
use winit::event::WindowEvent;

/// Input handling layers with associated priorities.
///
/// Layers are ordered by priority - higher priority layers receive input first.
/// Each layer represents a different input handling context (menus, console, game, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum InputLayer {
    /// Modal dialogs (highest priority - blocks all other input)
    Modal = 100,

    /// Debug console overlay
    Console = 50,

    /// Main menu and settings screens
    Menu = 40,

    /// In-game UI/HUD elements
    GameUI = 30,

    /// Core game world input (movement, camera, interactions)
    Game = 10,
}

impl InputLayer {
    /// Get the priority value for this layer.
    ///
    /// Higher numbers = higher priority = receives input first.
    pub fn priority(self) -> u8 {
        self as u8
    }
}

/// Type alias for input handler functions.
///
/// Handler functions receive a WindowEvent and return true if they consumed
/// the event (preventing further propagation) or false if the event should
/// continue to lower priority layers.
#[allow(dead_code)] // TODO: integrate with InputDispatcher
pub type InputHandler = Box<dyn Fn(&WindowEvent) -> bool + Send + Sync>;

/// Priority-based input router that dispatches events to registered layers.
///
/// The router tracks which input layers are active based on the current
/// [`GameState`]. Full event dispatch is not yet integrated — see module docs.
pub struct InputRouter {
    /// Currently active input layers (determined by GameState)
    active_layers: Vec<InputLayer>,

    /// Registered handlers for each layer
    handlers: HashMap<InputLayer, InputHandler>,
}

impl InputRouter {
    /// Create a new input router with no active layers or handlers.
    pub fn new() -> Self {
        Self {
            active_layers: Vec::new(),
            handlers: HashMap::new(),
        }
    }

    /// Register an input handler for a specific layer.
    ///
    /// If a handler already exists for this layer, it will be replaced.
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn register_handler<F>(&mut self, layer: InputLayer, handler: F)
    where
        F: Fn(&WindowEvent) -> bool + Send + Sync + 'static,
    {
        self.handlers.insert(layer, Box::new(handler));
    }

    /// Update the active input layers based on the current game state.
    ///
    /// This should be called whenever the game state changes to ensure
    /// input is routed to the appropriate handlers.
    ///
    /// # Arguments
    ///
    /// * `state` - The new game state to configure layers for
    pub fn update_for_state(&mut self, state: GameState) {
        self.active_layers.clear();

        match state {
            GameState::Menu => {
                // Only menu layer active
                self.active_layers.push(InputLayer::Menu);
            }
            GameState::Playing => {
                // Game input active, optionally GameUI for HUD
                self.active_layers.push(InputLayer::GameUI);
                self.active_layers.push(InputLayer::Game);
            }
            GameState::ConsoleOpen => {
                // Console gets all input, game world doesn't receive input
                self.active_layers.push(InputLayer::Console);
            }
            GameState::Paused => {
                // Pause menu/UI active
                self.active_layers.push(InputLayer::Menu);
            }
        }

        // Sort by priority (highest first) to ensure correct dispatch order
        self.active_layers.sort_by(|a, b| b.cmp(a));
    }

    /// Dispatch a window event to active input layers in priority order.
    ///
    /// If a handler returns `true` (consumed), dispatch stops.
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn dispatch(&self, event: &WindowEvent) -> bool {
        for layer in &self.active_layers {
            if let Some(handler) = self.handlers.get(layer)
                && handler(event)
            {
                // Event consumed, stop propagation
                return true;
            }
        }

        // Event not consumed by any layer
        false
    }

    /// Get the currently active input layers (highest priority first).
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn active_layers(&self) -> &[InputLayer] {
        &self.active_layers
    }

    /// Check if a specific layer is currently active.
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn is_layer_active(&self, layer: InputLayer) -> bool {
        self.active_layers.contains(&layer)
    }

    /// Clear all registered handlers.
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn clear_handlers(&mut self) {
        self.handlers.clear();
    }

    /// Get the number of active layers.
    #[allow(dead_code)] // TODO: integrate with InputDispatcher
    pub fn active_layer_count(&self) -> usize {
        self.active_layers.len()
    }
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_layer_priority_ordering() {
        assert!(InputLayer::Modal > InputLayer::Console);
        assert!(InputLayer::Console > InputLayer::Menu);
        assert!(InputLayer::Menu > InputLayer::GameUI);
        assert!(InputLayer::GameUI > InputLayer::Game);
    }

    #[test]
    fn test_active_layers_for_menu_state() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Menu);

        assert_eq!(router.active_layer_count(), 1);
        assert!(router.is_layer_active(InputLayer::Menu));
        assert!(!router.is_layer_active(InputLayer::Game));
    }

    #[test]
    fn test_active_layers_for_playing_state() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Playing);

        assert_eq!(router.active_layer_count(), 2);
        assert!(router.is_layer_active(InputLayer::Game));
        assert!(router.is_layer_active(InputLayer::GameUI));
        assert!(!router.is_layer_active(InputLayer::Console));
    }

    #[test]
    fn test_active_layers_for_console_state() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::ConsoleOpen);

        assert_eq!(router.active_layer_count(), 1);
        assert!(router.is_layer_active(InputLayer::Console));
        assert!(!router.is_layer_active(InputLayer::Game));
    }

    #[test]
    fn test_active_layers_for_paused_state() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Paused);

        assert_eq!(router.active_layer_count(), 1);
        assert!(router.is_layer_active(InputLayer::Menu));
    }

    #[test]
    fn test_dispatch_with_no_handlers() {
        let router = InputRouter::new();
        let event = WindowEvent::Focused(true);

        // Should return false (not consumed) when no handlers registered
        assert!(!router.dispatch(&event));
    }

    #[test]
    fn test_dispatch_with_consuming_handler() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::ConsoleOpen);

        let consumed = Arc::new(Mutex::new(false));
        let consumed_clone = consumed.clone();

        router.register_handler(InputLayer::Console, move |_event| {
            *consumed_clone.lock().unwrap() = true;
            true // Consume the event
        });

        let event = WindowEvent::Focused(true);
        assert!(router.dispatch(&event));
        assert!(*consumed.lock().unwrap());
    }

    #[test]
    fn test_dispatch_priority_order() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Playing);

        let order = Arc::new(Mutex::new(Vec::new()));

        // Register GameUI handler (priority 30)
        let order_ui = order.clone();
        router.register_handler(InputLayer::GameUI, move |_event| {
            order_ui.lock().unwrap().push("GameUI");
            false // Don't consume
        });

        // Register Game handler (priority 10)
        let order_game = order.clone();
        router.register_handler(InputLayer::Game, move |_event| {
            order_game.lock().unwrap().push("Game");
            false // Don't consume
        });

        let event = WindowEvent::Focused(true);
        router.dispatch(&event);

        let call_order = order.lock().unwrap();
        assert_eq!(call_order.as_slice(), &["GameUI", "Game"]);
    }

    #[test]
    fn test_dispatch_stops_on_consume() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Playing);

        let order = Arc::new(Mutex::new(Vec::new()));

        // GameUI consumes event
        let order_ui = order.clone();
        router.register_handler(InputLayer::GameUI, move |_event| {
            order_ui.lock().unwrap().push("GameUI");
            true // Consume
        });

        // Game should not be called
        let order_game = order.clone();
        router.register_handler(InputLayer::Game, move |_event| {
            order_game.lock().unwrap().push("Game");
            false
        });

        let event = WindowEvent::Focused(true);
        assert!(router.dispatch(&event));

        let call_order = order.lock().unwrap();
        assert_eq!(call_order.as_slice(), &["GameUI"]); // Game was not called
    }

    #[test]
    fn test_clear_handlers() {
        let mut router = InputRouter::new();
        router.register_handler(InputLayer::Game, |_| false);
        router.register_handler(InputLayer::Console, |_| false);

        router.clear_handlers();

        let event = WindowEvent::Focused(true);
        router.update_for_state(GameState::ConsoleOpen);
        assert!(!router.dispatch(&event)); // No handlers to consume
    }

    #[test]
    fn test_handler_replacement() {
        let mut router = InputRouter::new();
        router.update_for_state(GameState::Playing);

        let first_called = Arc::new(Mutex::new(false));
        let first_called_clone = first_called.clone();

        // Register first handler
        router.register_handler(InputLayer::Game, move |_| {
            *first_called_clone.lock().unwrap() = true;
            false
        });

        let second_called = Arc::new(Mutex::new(false));
        let second_called_clone = second_called.clone();

        // Replace with second handler
        router.register_handler(InputLayer::Game, move |_| {
            *second_called_clone.lock().unwrap() = true;
            false
        });

        let event = WindowEvent::Focused(true);
        router.dispatch(&event);

        // Only second handler should have been called
        assert!(!*first_called.lock().unwrap());
        assert!(*second_called.lock().unwrap());
    }
}
