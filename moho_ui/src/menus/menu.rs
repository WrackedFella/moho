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

/// Menu trait: each menu is responsible for drawing itself and returning
/// a list of `MenuItem`s describing clickable items painted by the menu.
/// The adapter consumes that list for fallback hit-testing and to map
/// actions to `UiEvent`s. Implementations should fill `rect` for any
/// button-like widgets to enable fallback behavior.
pub trait Menu: Send {
    fn name(&self) -> &str;
    fn spec(&self) -> &ScreenSpec;
    /// Draw the menu and return a vector of MenuItem entries. The menu is
    /// responsible for painting the UI; each MenuItem should include an
    /// optional `rect` corresponding to the painted widget so the adapter
    /// can perform fallback hit tests when necessary.
    fn ui(&mut self, ctx: &egui::Context) -> Vec<MenuItem>;
    fn on_show(&mut self) {}
    fn on_hide(&mut self) {}

    /// Allow downcasting to concrete menu types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    /// Read-only downcast helper
    fn as_any(&self) -> &dyn std::any::Any;
}
