/// Gameplay HUD overlay — compass, status bars, and hotbar.
///
/// Renders only in first-person mode. All three sub-components
/// use `egui::Area` so they don't interfere with the main UI panels.
use super::overlay_manager::{HudData, Overlay};

#[derive(Debug)]
pub struct GameplayHud {
    visible: bool,
}

impl GameplayHud {
    pub fn new() -> Self {
        Self { visible: true }
    }
}

impl Default for GameplayHud {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay for GameplayHud {
    fn name(&self) -> &str {
        "gameplay_hud"
    }

    fn render(&mut self, ctx: &egui::Context, data: &HudData) {
        if !self.visible || !data.is_fps_mode {
            return;
        }

        render_compass(ctx, data.camera_yaw);
        render_status_bars(ctx, data.player_health, data.player_stamina);
        render_hotbar(ctx);
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

// ── Compass ──────────────────────────────────────────────────────────────────

/// Cardinal and inter-cardinal direction labels with their angular offsets (radians).
const COMPASS_LABELS: &[(&str, f32)] = &[
    ("N", 0.0),
    ("NE", std::f32::consts::FRAC_PI_4),
    ("E", std::f32::consts::FRAC_PI_2),
    ("SE", 3.0 * std::f32::consts::FRAC_PI_4),
    ("S", std::f32::consts::PI),
    ("SW", -3.0 * std::f32::consts::FRAC_PI_4),
    ("W", -std::f32::consts::FRAC_PI_2),
    ("NW", -std::f32::consts::FRAC_PI_4),
];

/// Labels within this angular range of forward are visible.
const COMPASS_HALF_FOV: f32 = std::f32::consts::FRAC_PI_3; // 60°

fn render_compass(ctx: &egui::Context, camera_yaw: f32) {
    egui::Area::new("gameplay_hud_compass".into())
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 8.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;

                for &(label, angle) in COMPASS_LABELS {
                    // Angular difference: how far this label is from forward direction.
                    // camera_yaw = 0 means facing North (angle 0).
                    let mut diff = angle - camera_yaw;
                    // Wrap to [-π, π]
                    while diff > std::f32::consts::PI {
                        diff -= std::f32::consts::TAU;
                    }
                    while diff < -std::f32::consts::PI {
                        diff += std::f32::consts::TAU;
                    }

                    if diff.abs() > COMPASS_HALF_FOV {
                        continue; // outside visible arc
                    }

                    let is_facing = diff.abs() < std::f32::consts::FRAC_PI_8;
                    let text = if is_facing {
                        egui::RichText::new(label)
                            .strong()
                            .color(egui::Color32::WHITE)
                            .size(14.0)
                    } else {
                        egui::RichText::new(label)
                            .color(egui::Color32::from_gray(200))
                            .size(12.0)
                    };
                    ui.label(text);
                }
            });
        });
}

// ── Status bars ───────────────────────────────────────────────────────────────

const BAR_WIDTH: f32 = 120.0;
const BAR_HEIGHT: f32 = 14.0;
const BAR_ROUNDING: f32 = 3.0;
const BAR_SPACING: f32 = 4.0;

fn render_status_bars(ctx: &egui::Context, health: f32, stamina: f32) {
    egui::Area::new("gameplay_hud_status".into())
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                paint_bar(
                    ui,
                    "HP",
                    health.clamp(0.0, 1.0),
                    egui::Color32::from_rgb(200, 50, 50),
                );
                ui.add_space(BAR_SPACING);
                paint_bar(
                    ui,
                    "SP",
                    stamina.clamp(0.0, 1.0),
                    egui::Color32::from_rgb(50, 180, 80),
                );
            });
        });
}

fn paint_bar(ui: &mut egui::Ui, label: &str, fraction: f32, fill_color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(egui::Color32::WHITE),
        );
        ui.add_space(4.0);

        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(BAR_WIDTH, BAR_HEIGHT), egui::Sense::hover());

        let painter = ui.painter();

        // Background
        painter.rect_filled(
            rect,
            BAR_ROUNDING,
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 160),
        );

        // Fill
        if fraction > 0.0 {
            let fill_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * fraction, rect.height()),
            );
            painter.rect_filled(fill_rect, BAR_ROUNDING, fill_color);
        }

        // Border
        painter.rect_stroke(
            rect,
            BAR_ROUNDING,
            egui::Stroke::new(1.0, egui::Color32::from_gray(160)),
            egui::StrokeKind::Outside,
        );
    });
}

// ── Hotbar ────────────────────────────────────────────────────────────────────

const HOTBAR_SLOTS: usize = 8;
const SLOT_SIZE: f32 = 48.0;
const SLOT_GAP: f32 = 4.0;
const SLOT_ROUNDING: f32 = 6.0;

fn render_hotbar(ctx: &egui::Context) {
    let total_width = HOTBAR_SLOTS as f32 * SLOT_SIZE + (HOTBAR_SLOTS - 1) as f32 * SLOT_GAP;

    egui::Area::new("gameplay_hud_hotbar".into())
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -12.0))
        .show(ctx, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(total_width, SLOT_SIZE), egui::Sense::hover());

            let painter = ui.painter();

            for i in 0..HOTBAR_SLOTS {
                let x = rect.min.x + i as f32 * (SLOT_SIZE + SLOT_GAP);
                let slot_rect = egui::Rect::from_min_size(
                    egui::pos2(x, rect.min.y),
                    egui::vec2(SLOT_SIZE, SLOT_SIZE),
                );

                // Slot background
                painter.rect_filled(
                    slot_rect,
                    SLOT_ROUNDING,
                    egui::Color32::from_rgba_premultiplied(0, 0, 0, 140),
                );

                // Slot border
                painter.rect_stroke(
                    slot_rect,
                    SLOT_ROUNDING,
                    egui::Stroke::new(2.0, egui::Color32::from_gray(160)),
                    egui::StrokeKind::Outside,
                );

                // Slot number (1-based) in bottom-left corner
                let number = format!("{}", i + 1);
                let text_pos = egui::pos2(slot_rect.min.x + 3.0, slot_rect.max.y - 13.0);
                painter.text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    &number,
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_gray(180),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_facing_north_at_yaw_zero() {
        // At yaw=0, the "N" label has diff=0 → is_facing=true
        let yaw = 0.0_f32;
        let (label, angle) = COMPASS_LABELS[0]; // ("N", 0.0)
        let diff = angle - yaw;
        assert_eq!(label, "N");
        assert!(
            diff.abs() < std::f32::consts::FRAC_PI_8,
            "N should be centered at yaw=0"
        );
    }

    #[test]
    fn compass_facing_east_at_yaw_pi_over_2() {
        let yaw = std::f32::consts::FRAC_PI_2;
        // E is at angle π/2, diff = π/2 - π/2 = 0
        let east = COMPASS_LABELS.iter().find(|(l, _)| *l == "E").unwrap();
        let diff = east.1 - yaw;
        assert!(
            diff.abs() < std::f32::consts::FRAC_PI_8,
            "E should be centered at yaw=π/2"
        );
    }

    #[test]
    fn compass_facing_south_at_yaw_pi() {
        let yaw = std::f32::consts::PI;
        let south = COMPASS_LABELS.iter().find(|(l, _)| *l == "S").unwrap();
        let mut diff = south.1 - yaw;
        while diff > std::f32::consts::PI {
            diff -= std::f32::consts::TAU;
        }
        while diff < -std::f32::consts::PI {
            diff += std::f32::consts::TAU;
        }
        assert!(
            diff.abs() < std::f32::consts::FRAC_PI_8,
            "S should be centered at yaw=π"
        );
    }

    #[test]
    fn status_bar_clamp_overdraw() {
        // Values outside [0,1] should be clamped — just verify clamp math here
        assert_eq!(1.5_f32.clamp(0.0, 1.0), 1.0);
        assert_eq!((-0.1_f32).clamp(0.0, 1.0), 0.0);
    }

    #[test]
    fn gameplay_hud_hidden_when_not_fps() {
        let ctx = egui::Context::default();
        let mut hud = GameplayHud::new();
        let data = HudData {
            is_fps_mode: false,
            ..Default::default()
        };

        // Should not panic and should render nothing (no egui output is hard to assert
        // without an egui render harness, but verifying no panic is the minimum).
        let _ = ctx.run(Default::default(), |ctx| {
            hud.render(ctx, &data);
        });
        // Still visible by flag, just filtered by is_fps_mode
        assert!(hud.is_visible());
    }

    #[test]
    fn gameplay_hud_renders_in_fps_mode() {
        let ctx = egui::Context::default();
        let mut hud = GameplayHud::new();
        let data = HudData::default(); // is_fps_mode = true

        let _ = ctx.run(Default::default(), |ctx| {
            hud.render(ctx, &data);
        });
    }
}
