//! RTS HUD overlay — visible during isometric/RTS camera mode.
//!
//! Stub for future implementation. Currently renders only the time of day.
//! Will grow to include mini-map, selection info, resource counters, etc.

use super::overlay_manager::{HudData, Overlay};

/// Isometric / RTS gameplay HUD.
#[derive(Debug)]
pub struct RtsHud {
    visible: bool,
}

impl RtsHud {
    pub fn new() -> Self {
        Self { visible: true }
    }
}

impl Default for RtsHud {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay for RtsHud {
    fn name(&self) -> &'static str {
        "rts_hud"
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn render(&mut self, ui: &mut egui::Ui, data: &HudData) {
        if !self.visible || data.is_fps_mode {
            return;
        }

        render_time_of_day(ui, data.time_of_day);
    }
}

/// Time-of-day badge in the top-right corner.
fn render_time_of_day(ui: &mut egui::Ui, time: f32) {
    let hours = time.floor() as u32 % 24;
    let minutes = ((time - time.floor()) * 60.0).floor() as u32;
    let label = format!("{hours:02}:{minutes:02}");

    egui::Area::new("rts_time_display".into())
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
    use crate::overlays::test_support::{rect_of, second_frame, texts};

    #[test]
    fn render_time_of_day_shows_clock_in_top_right() {
        let ctx = egui::Context::default();

        let output = second_frame(&ctx, |ui| render_time_of_day(ui, 7.25));
        let bounds = rect_of(&output, "07:15");

        assert_eq!(texts(&output), ["07:15"]);
        assert!(bounds.center().x > 400.0, "not on the right: {bounds:?}");
        assert!(
            bounds.max.x <= 800.0 - 12.0 + 0.5,
            "past margin: {bounds:?}"
        );
        assert!(bounds.min.y < 100.0, "not at top: {bounds:?}");
    }
}
