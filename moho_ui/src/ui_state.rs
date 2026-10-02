//! UI state management for screens, overlays, and modals.
//!
//! This module separates UI state management from rendering concerns,
//! making the adapter lighter and the state easier to test and maintain.

use crate::modal::ModalManager;
use crate::overlays::Console;
use crate::overlays::{ChunkDebugOverlay, DebugHud, FpsHud, GameplayHud, OverlayManager, RtsHud};
use crate::screens::Menu;
use std::collections::HashMap;

/// Manages the lifecycle and state of all UI components
pub struct UiStateManager {
    /// Collection of all available screens (menus, forms, etc.)
    screens: HashMap<String, Box<dyn Menu>>,

    /// Currently active screen (if any)
    active_screen: Option<String>,

    /// Modal dialog manager
    pub modal_manager: ModalManager,

    /// Debug console overlay (separate from screens)
    pub console: Console,

    /// Overlay layer manager (FPS HUD, RTS HUD, Debug HUD, etc.)
    pub overlay_manager: OverlayManager,

    /// Whether UI menus are currently visible
    pub visible: bool,
}

impl UiStateManager {
    /// Create a new UI state manager with default screens
    pub fn new() -> Self {
        let mut screens: HashMap<String, Box<dyn Menu>> = HashMap::new();

        // Register default screens
        use crate::screens::{NewWorldMenu, SettingsMenu, StartMenu};

        screens.insert("start".to_string(), Box::new(StartMenu::new()));
        screens.insert("settings".to_string(), Box::new(SettingsMenu::new()));
        screens.insert("new_world".to_string(), Box::new(NewWorldMenu::new()));

        // Register default overlay layers
        let mut overlay_manager = OverlayManager::new();
        overlay_manager.register(Box::new(FpsHud::new()));
        overlay_manager.register(Box::new(RtsHud::new()));
        overlay_manager.register(Box::new(DebugHud::new()));
        overlay_manager.register(Box::new(GameplayHud::new()));
        overlay_manager.register(Box::new(ChunkDebugOverlay::new()));

        Self {
            screens,
            active_screen: Some("start".to_string()),
            modal_manager: ModalManager::new(),
            console: Console::new(),
            overlay_manager,
            visible: true,
        }
    }

    /// Add a new screen to the manager
    pub fn add_screen(&mut self, name: String, screen: Box<dyn Menu>) {
        self.screens.insert(name, screen);
    }

    /// Show a specific screen by name
    pub fn show_screen(&mut self, name: &str) -> bool {
        if !self.screens.contains_key(name) {
            return false;
        }

        // Call on_hide for current screen
        if let Some(current) = &self.active_screen
            && let Some(screen) = self.screens.get_mut(current)
        {
            screen.on_hide();
        }

        // Activate new screen
        self.active_screen = Some(name.to_string());
        if let Some(screen) = self.screens.get_mut(name) {
            screen.on_show();
        }

        self.visible = true;
        true
    }

    /// Hide all screens
    pub fn hide_all(&mut self) {
        if let Some(current) = &self.active_screen
            && let Some(screen) = self.screens.get_mut(current)
        {
            screen.on_hide();
        }
        self.active_screen = None;
        self.visible = false;
    }

    /// Get the currently active screen (mutable)
    pub fn active_screen_mut(&mut self) -> Option<&mut Box<dyn Menu>> {
        if let Some(name) = &self.active_screen {
            self.screens.get_mut(name)
        } else {
            None
        }
    }

    /// Get the currently active screen (immutable)
    pub fn active_screen(&self) -> Option<&dyn Menu> {
        if let Some(name) = &self.active_screen {
            self.screens.get(name).map(|b| b.as_ref())
        } else {
            None
        }
    }

    /// Get the name of the active screen
    pub fn active_screen_name(&self) -> Option<&str> {
        self.active_screen.as_deref()
    }

    /// Check if a specific screen is active
    pub fn is_screen_active(&self, name: &str) -> bool {
        self.active_screen.as_deref() == Some(name)
    }

    /// Get a specific screen by name (mutable)
    pub fn get_screen_mut(&mut self, name: &str) -> Option<&mut Box<dyn Menu>> {
        self.screens.get_mut(name)
    }

    /// Get a specific screen by name (immutable)
    pub fn get_screen(&self, name: &str) -> Option<&dyn Menu> {
        self.screens.get(name).map(|b| b.as_ref())
    }
}

impl Default for UiStateManager {
    fn default() -> Self {
        Self::new()
    }
}
