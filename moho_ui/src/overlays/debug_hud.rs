//! Debug HUD overlay — toggled with F3.
//!
//! Displays technical diagnostics: player coordinates, chunk position,
//! camera mode, FPS / frame time, and material under crosshair.

use super::overlay_manager::{HudData, Overlay};

/// Exponential moving average weight for FPS smoothing.
const FPS_SMOOTHING: f32 = 0.05;

/// Debug information overlay rendered in the top-left corner.
#[derive(Debug)]
pub struct DebugHud {
    visible: bool,
    smoothed_fps: f32,
}

impl DebugHud {
    pub fn new() -> Self {
        Self {
            visible: false,
            smoothed_fps: 60.0,
        }
    }
}

impl Default for DebugHud {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay for DebugHud {
    fn name(&self) -> &str {
        "debug"
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn render(&mut self, ctx: &egui::Context, data: &HudData) {
        if !self.visible {
            return;
        }

        // Smooth FPS via exponential moving average
        if data.frame_time_secs > 0.0 {
            let instant_fps = 1.0 / data.frame_time_secs;
            self.smoothed_fps += FPS_SMOOTHING * (instant_fps - self.smoothed_fps);
        }

        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 160))
            .inner_margin(egui::Margin::same(8));

        egui::Area::new("debug_hud".into())
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(8.0, 8.0))
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    let mono = egui::FontId::monospace(13.0);
                    let color = egui::Color32::from_rgb(200, 220, 200);

                    ui.label(
                        egui::RichText::new(format!(
                            "FPS: {:.0}  ({:.1} ms)",
                            self.smoothed_fps,
                            data.frame_time_secs * 1000.0,
                        ))
                        .font(mono.clone())
                        .color(color),
                    );

                    ui.add_space(2.0);

                    ui.label(
                        egui::RichText::new(format!(
                            "Pos: ({:.1}, {:.1}, {:.1})",
                            data.player_position[0],
                            data.player_position[1],
                            data.player_position[2],
                        ))
                        .font(mono.clone())
                        .color(color),
                    );

                    ui.label(
                        egui::RichText::new(format!(
                            "Chunk: ({}, {}, {})",
                            data.chunk_position[0], data.chunk_position[1], data.chunk_position[2],
                        ))
                        .font(mono.clone())
                        .color(color),
                    );

                    ui.label(
                        egui::RichText::new(format!("Camera: {}", data.camera_mode))
                            .font(mono.clone())
                            .color(color),
                    );

                    let material = match data.material_under_crosshair {
                        Some(id) => format!("Material: {id}"),
                        None => "Material: N/A".to_string(),
                    };
                    ui.label(egui::RichText::new(material).font(mono).color(color));
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_hidden() {
        let hud = DebugHud::new();
        assert!(!hud.is_visible());
    }

    #[test]
    fn toggle_visibility() {
        let mut hud = DebugHud::new();
        hud.toggle();
        assert!(hud.is_visible());
        hud.toggle();
        assert!(!hud.is_visible());
    }
}
