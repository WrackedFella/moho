//! Top-down chunk minimap overlay for debugging chunk streaming gaps.
//!
//! Shows a grid of loaded (green) vs unloaded (dark) chunk columns centred on
//! the player. Toggle with the `chunk_debug` overlay name.

use super::overlay_manager::{HudData, Overlay};

const CELL_PX: f32 = 6.0;   // pixels per chunk cell
const VIEW_RADIUS: i32 = 14; // chunks visible in each direction

#[derive(Debug)]
pub struct ChunkDebugOverlay {
    visible: bool,
}

impl ChunkDebugOverlay {
    pub fn new() -> Self {
        Self { visible: false }
    }
}

impl Overlay for ChunkDebugOverlay {
    fn name(&self) -> &str {
        "chunk_debug"
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn render(&mut self, ctx: &egui::Context, data: &HudData) {
        let pcx = data.chunk_position[0];
        let pcz = data.chunk_position[2];

        let diameter = (VIEW_RADIUS * 2 + 1) as f32;
        let map_size = egui::vec2(diameter * CELL_PX, diameter * CELL_PX);

        egui::Area::new("chunk_debug_map".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(map_size, egui::Sense::hover());
                let painter = ui.painter_at(rect);

                // Background
                painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));

                let loaded: std::collections::HashSet<(i32, i32)> = data
                    .loaded_chunk_xz
                    .iter()
                    .map(|&[x, z]| (x, z))
                    .collect();

                for dz in -VIEW_RADIUS..=VIEW_RADIUS {
                    for dx in -VIEW_RADIUS..=VIEW_RADIUS {
                        let cx = pcx + dx;
                        let cz = pcz + dz;

                        let px = (dx + VIEW_RADIUS) as f32 * CELL_PX;
                        let py = (dz + VIEW_RADIUS) as f32 * CELL_PX;
                        let cell = egui::Rect::from_min_size(
                            rect.min + egui::vec2(px, py),
                            egui::vec2(CELL_PX - 1.0, CELL_PX - 1.0),
                        );

                        let fill = if dx == 0 && dz == 0 {
                            egui::Color32::from_rgb(255, 220, 0) // player: yellow
                        } else if loaded.contains(&(cx, cz)) {
                            egui::Color32::from_rgb(60, 180, 60) // loaded: green
                        } else {
                            egui::Color32::from_rgb(40, 40, 50) // unloaded: dark
                        };
                        painter.rect_filled(cell, 0.0, fill);
                    }
                }

                // Label
                painter.text(
                    rect.left_bottom() + egui::vec2(2.0, -2.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("chunks ({},{})", pcx, pcz),
                    egui::FontId::monospace(9.0),
                    egui::Color32::from_gray(180),
                );
            });
    }
}
