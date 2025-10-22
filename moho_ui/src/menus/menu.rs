use crate::prefs::Prefs;
use egui::{Align2, Vec2};
use std::path::PathBuf;

/// Actions a Menu may return when interacted with.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    None,
    LoadScene(PathBuf),
    NewWorld,
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

/// Hit-test helper for menu fallback logic. Given an optional press and
/// release position and a slice of `MenuItem`s, return the first matching
/// MenuAction if any item contains both positions (press+release) or just
/// release when press is None.
///
/// The `press` and `release` coordinates may be provided in either egui
/// logical points or physical pixels. If the coordinates are physical
/// pixels, set `coords_are_physical` to true and provide the current
/// `pixels_per_point` (ppp) from egui in `pixels_per_point` so the helper
/// can normalize coordinates into egui logical points by dividing by `ppp`.
/// If `coords_are_physical` is false the coordinates are assumed to already
/// be in egui logical points and no normalization is performed.
pub fn hit_test_menu_items(
    press: Option<(f32, f32)>,
    release: (f32, f32),
    items: &[MenuItem],
    pixels_per_point: Option<f32>,
    coords_are_physical: bool,
    // Margin in logical points to expand each item's rect for hit-testing.
    hit_margin: f32,
) -> Option<MenuAction> {
    // Helper to normalize incoming coordinates into egui logical points
    // when the caller provides physical pixel coords and a known ppp.
    let norm = |(x, y): (f32, f32)| {
        if coords_are_physical {
            if let Some(ppp) = pixels_per_point {
                (x / ppp, y / ppp)
            } else {
                // No pixels_per_point available; assume coords are already logical
                (x, y)
            }
        } else {
            (x, y)
        }
    };
    // Compute normalized margin (logical points). If the incoming
    // coords are physical pixels, convert margin using pixels_per_point
    // so the margin value is interpreted as logical points by callers.
    let norm_margin = if coords_are_physical {
        if let Some(ppp) = pixels_per_point {
            hit_margin / ppp
        } else {
            hit_margin
        }
    } else {
        hit_margin
    };

    // When press is available prefer press+release containment.
    if let Some((px, py)) = press {
        let (pxn, pyn) = norm((px, py));
        let (rxn, ryn) = norm(release);
        for item in items {
            if !item.enabled {
                continue;
            }
            if let Some(r) = item.rect {
                let r_exp = egui::Rect::from_min_max(
                    egui::pos2(r.min.x - norm_margin, r.min.y - norm_margin),
                    egui::pos2(r.max.x + norm_margin, r.max.y + norm_margin),
                );
                if r_exp.contains(egui::pos2(pxn, pyn)) && r_exp.contains(egui::pos2(rxn, ryn)) {
                    return Some(item.action.clone());
                }
            }
        }
        None
    } else {
        let (rxn, ryn) = norm(release);
        for item in items {
            if !item.enabled {
                continue;
            }
            if let Some(r) = item.rect {
                let r_exp = egui::Rect::from_min_max(
                    egui::pos2(r.min.x - norm_margin, r.min.y - norm_margin),
                    egui::pos2(r.max.x + norm_margin, r.max.y + norm_margin),
                );
                if r_exp.contains(egui::pos2(rxn, ryn)) {
                    return Some(item.action.clone());
                }
            }
        }
        None
    }
}

/// Test-friendly helper to construct an egui::Rect from primitive coordinates.
/// This avoids having integration tests depend directly on the `egui` crate.
pub fn rect_from_min_max(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
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
/// a list of `MenuItem`s describing clickable items painted by the menu.
/// The adapter consumes that list for fallback hit-testing and to map
/// actions to `UiEvent`s. Implementations should fill `rect` for any
/// button-like widgets to enable fallback behavior.
pub trait Menu: Send {
    fn name(&self) -> &str;
    fn spec(&self) -> &MenuSpec;
    /// Draw the menu and return a vector of MenuItem entries. The menu is
    /// responsible for painting the UI; each MenuItem should include an
    /// optional `rect` corresponding to the painted widget so the adapter
    /// can perform fallback hit tests when necessary.
    fn ui(&mut self, ctx: &egui::Context) -> Vec<MenuItem>;
    fn on_show(&mut self) {}
    fn on_hide(&mut self) {}

    /// Allow downcasting to concrete menu types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
