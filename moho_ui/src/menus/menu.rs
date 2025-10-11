use egui::{Align2, Vec2};
use std::path::PathBuf;

/// Actions a Menu may return when interacted with.
#[derive(Debug)]
pub enum MenuAction {
    None,
    LoadScene(PathBuf),
    Exit,
    ShowMenu(String),
    Close,
}

/// Basic specification for menu positioning and simple style overrides.
#[derive(Clone, Debug)]
pub struct MenuSpec {
    pub anchor: Align2,
    pub offset: Vec2,
    pub modal: bool,
}

impl Default for MenuSpec {
    fn default() -> Self {
        Self {
            anchor: Align2::LEFT_TOP,
            offset: egui::vec2(8.0, 8.0),
            modal: false,
        }
    }
}

/// Menu trait: each menu is responsible for drawing itself and returning
/// a `MenuAction` that the adapter can translate into `UiEvent`s.
pub trait Menu: Send {
    fn name(&self) -> &str;
    fn spec(&self) -> &MenuSpec;
    /// Draw the menu and return a MenuAction. Also return optional
    /// (load_rect, exit_rect) pair of response rects that the adapter can
    /// use for fallback hit-testing. If the menu does not expose such
    /// rects, return None.
    fn ui(&mut self, ctx: &egui::Context) -> (MenuAction, Option<(egui::Rect, egui::Rect)>);
    fn on_show(&mut self) {}
    fn on_hide(&mut self) {}
}
