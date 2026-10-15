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
    fn name(&self) -> &'static str {
        "fps_hud"
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn render(&mut self, ui: &mut egui::Ui, data: &HudData) {
        if !self.visible || !data.is_fps_mode {
            return;
        }

        render_crosshair(ui);
        render_time_of_day(ui, data.time_of_day);
    }
}

/// Draw a small crosshair at screen center.
fn render_crosshair(ui: &mut egui::Ui) {
    let ctx = ui.ctx();
    let center = ctx.input(egui::InputState::viewport_rect).center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        "crosshair".into(),
    ));

    let size = 10.0;
    let stroke = egui::Stroke::new(
        2.0_f32,
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
fn render_time_of_day(ui: &mut egui::Ui, time: f32) {
    let hours = time.floor() as u32 % 24;
    let minutes = ((time - time.floor()) * 60.0).floor() as u32;
    let label = format!("{hours:02}:{minutes:02}");

    egui::Area::new("fps_time_display".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 8.0))
        .show(ui.ctx(), |ui| {
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
    use crate::overlays::test_support::{line_segments, rect_of, second_frame, texts};

    #[test]
    fn render_draws_crosshair_centered_on_screen() {
        let ctx = egui::Context::default();

        let output = second_frame(&ctx, |ui| FpsHud::new().render(ui, &HudData::default()));
        let lines = line_segments(&output);

        assert!(lines.contains(&[egui::pos2(390.0, 300.0), egui::pos2(410.0, 300.0)]));
        assert!(lines.contains(&[egui::pos2(400.0, 290.0), egui::pos2(400.0, 310.0)]));
    }

    #[test]
    fn render_time_of_day_shows_clock_in_top_right() {
        let ctx = egui::Context::default();

        let output = second_frame(&ctx, |ui| render_time_of_day(ui, 13.5));
        let bounds = rect_of(&output, "13:30");

        assert_eq!(texts(&output), ["13:30"]);
        assert!(bounds.center().x > 400.0, "not on the right: {bounds:?}");
        assert!(
            bounds.max.x <= 800.0 - 12.0 + 0.5,
            "past margin: {bounds:?}"
        );
        assert!(bounds.min.y < 100.0, "not at top: {bounds:?}");
    }
}
