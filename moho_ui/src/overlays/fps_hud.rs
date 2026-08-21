//! FPS HUD overlay — visible during first-person gameplay.
//!
//! Renders a centered crosshair and the current time of day.
//! Automatically hides when the camera is not in first-person mode.

use super::overlay_manager::{HudData, Overlay};

/// First-person gameplay HUD.
#[derive(Debug)]
pub struct FpsHud {
    visible: bool,
}

impl FpsHud {
    pub fn new() -> Self {
        Self { visible: true }
    }
}

impl Default for FpsHud {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay for FpsHud {
    fn name(&self) -> &str {
        "fps_hud"
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn render(&mut self, ctx: &egui::Context, data: &HudData) {
        if !self.visible || !data.is_fps_mode {
            return;
        }

        render_crosshair(ctx);
        render_time_of_day(ctx, data.time_of_day);
    }
}

/// Draw a small crosshair at screen center.
fn render_crosshair(ctx: &egui::Context) {
    let center = ctx.input(|i| i.viewport_rect()).center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        "crosshair".into(),
    ));

    let size = 10.0;
    let stroke = egui::Stroke::new(
        2.0,
        egui::Color32::from_rgba_premultiplied(220, 220, 220, 180),
    );

    // Horizontal
    painter.line_segment(
        [
            egui::pos2(center.x - size, center.y),
            egui::pos2(center.x + size, center.y),
        ],
        stroke,
    );
    // Vertical
    painter.line_segment(
        [
            egui::pos2(center.x, center.y - size),
            egui::pos2(center.x, center.y + size),
        ],
        stroke,
    );
}

/// Time-of-day badge in the top-right corner.
fn render_time_of_day(ctx: &egui::Context, time: f32) {
    let hours = time.floor() as u32 % 24;
    let minutes = ((time - time.floor()) * 60.0).floor() as u32;
    let label = format!("{hours:02}:{minutes:02}");

    egui::Area::new("fps_time_display".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 120))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(label)
                            .font(egui::FontId::proportional(16.0))
                            .color(egui::Color32::from_rgb(220, 220, 200)),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_visible() {
        let hud = FpsHud::new();
        assert!(hud.is_visible());
    }
}
