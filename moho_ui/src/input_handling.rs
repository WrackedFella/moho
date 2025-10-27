//! Input handling utilities for UI components.
//!
//! This module provides fallback hit-testing logic and coordinate conversion
//! utilities for UI interaction when egui's built-in event handling is
//! insufficient or unavailable.

use crate::screens::{MenuAction, MenuItem};

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
