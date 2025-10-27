use crate::prefs::Prefs;
use egui::{Align2, Vec2};
use std::path::PathBuf;

use moho_core::scene_builders::WorldSpec;

/// Actions a Menu may return when interacted with.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    None,
    LoadScene(PathBuf),
    /// Request that the adapter open the New World menu (start menu -> open dialog)
    NewWorld,
    /// User confirmed generation from the New World menu with parameters
    GenerateWorld(WorldSpec),
    Exit,
    ShowMenu(String),
    Close,
    SettingsSaved(Prefs),
}

/// A single logical menu item exposed by a Menu implementation. `rect`
/// is optional and used by the adapter for fallback hit-testing when
/// egui's rendering is not available. `enabled` controls whether the
/// adapter should consider the item clickable in fallback logic.
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub action: MenuAction,
    pub rect: Option<egui::Rect>,
    pub enabled: bool,
    /// Whether this item was clicked during the current ui() invocation.
    /// Menu implementations should set this to true when the user
    /// interacts with the widget so the adapter can dispatch the
    /// action immediately (without relying solely on fallback hit-tests).
    pub clicked: bool,
}

impl PartialEq for MenuItem {
    fn eq(&self, other: &Self) -> bool {
        // Compare action by Debug string (MenuAction may not implement Eq)
        format!("{:?}", self.action) == format!("{:?}", other.action)
            && self.enabled == other.enabled
            && self.clicked == other.clicked
            // Compare rects approximately by their Debug string
            && format!("{:?}", self.rect) == format!("{:?}", other.rect)
    }
}

/// Basic specification for screen positioning and simple style overrides.
#[derive(Clone, Debug)]
pub struct ScreenSpec {
    pub anchor: Align2,
    pub offset: Vec2,
    pub modal: bool,
}

impl Default for ScreenSpec {
    fn default() -> Self {
        Self {
            anchor: Align2::LEFT_TOP,
            offset: egui::vec2(8.0, 8.0),
            modal: false,
        }
    }
}

/// Base trait for all UI components (screens, overlays, modals, etc.)
///
/// This trait provides the foundation for the UI system's component hierarchy.
/// All interactive UI elements should implement this trait or one of its
/// specialized variants (Screen, Overlay, etc.)
pub trait UiComponent: Send {
    /// Unique identifier for this component
    fn name(&self) -> &str;

    /// Render the component and return any actions triggered during rendering
    fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem>;

    /// Called when the component is shown
    fn on_show(&mut self) {}

    /// Called when the component is hidden
    fn on_hide(&mut self) {}

    /// Allow downcasting to concrete component types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Read-only downcast helper
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Screen trait: represents full-screen UI elements (menus, forms, settings, etc.)
///
/// Screens are modal by default, blocking the game and taking exclusive focus.
/// They use the entire viewport and typically follow a consistent layout pattern.
pub trait Screen: UiComponent {
    /// Screen-specific configuration (positioning, modal behavior, etc.)
    fn spec(&self) -> &ScreenSpec;

    /// Whether this screen captures raw window input events.
    ///
    /// Return `true` if the screen needs to receive raw WindowEvent messages
    /// for custom input handling (e.g., keybind listening in settings).
    fn captures_raw_input(&self) -> bool {
        false
    }

    /// Handle raw window input events when `captures_raw_input()` returns true.
    ///
    /// This method is only called when the screen has indicated it wants raw input.
    /// Return `true` if the event was consumed and should not be processed further.
    ///
    /// # Arguments
    /// * `event` - The raw winit WindowEvent to handle
    ///
    /// # Returns
    /// `true` if the event was consumed, `false` otherwise
    fn handle_raw_input(&mut self, _event: &winit::event::WindowEvent) -> bool {
        false
    }

    /// Check if this screen wants to show a modal dialog.
    ///
    /// Return `Some(modal)` if a modal should be displayed, `None` otherwise.
    /// The adapter will call this after rendering and display the modal if provided.
    fn take_pending_modal(&mut self) -> Option<Box<dyn crate::modal::Modal>> {
        None
    }

    /// Handle modal confirmation (user clicked OK/Confirm).
    ///
    /// Called when a modal shown by this screen is confirmed by the user.
    fn on_modal_confirm(&mut self) {}

    /// Handle modal cancellation (user clicked Cancel or closed the modal).
    ///
    /// Called when a modal shown by this screen is cancelled by the user.
    fn on_modal_cancel(&mut self) {}
}

/// Backward compatibility alias for existing code.
///
/// TODO: Remove this alias in a future refactor once all code uses Screen directly.
/// This allows gradual migration from Menu to Screen without breaking existing code.
pub trait Menu: Screen {}

// Blanket implementation: any Screen is automatically a Menu for backward compatibility
impl<T: Screen> Menu for T {}
