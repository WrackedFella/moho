//! RTS HUD overlay — visible during isometric/RTS camera mode.
//!
//! Draws the time of day and the view-height slider, and reports the player's pick
//! through `take_intents`.

use super::overlay_manager::{HudData, HudIntents, Overlay, ViewHeightHud};

const SLIDER_LENGTH: f32 = 200.0;

/// Isometric / RTS gameplay HUD.
#[derive(Debug)]
pub struct RtsHud {
    visible: bool,
    /// The step the player picked, until `take_intents` hands it over.
    chosen: Option<i32>,
}

impl RtsHud {
    pub fn new() -> Self {
        Self {
            visible: true,
            chosen: None,
        }
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

        render_mode_label(ui);
        render_time_of_day(ui, data.time_of_day);
        if let Some(view_height) = data.view_height {
            self.chosen = render_view_height(ui, view_height).or(self.chosen);
        }
    }

    fn take_intents(&mut self, out: &mut HudIntents) {
        out.view_level = self.chosen.take().or(out.view_level);
    }
}

/// Vertical view-height slider on the right edge, below the minimap and the clock.
/// Returns the step the player moved it to this frame, if any.
fn render_view_height(ui: &mut egui::Ui, view: ViewHeightHud) -> Option<i32> {
    egui::Area::new("rts_view_height".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 200.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 120))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    let text = if view.level >= view.top {
                        "All".to_owned()
                    } else {
                        view.level.to_string()
                    };
                    ui.label(
                        egui::RichText::new(text).color(egui::Color32::from_rgb(220, 220, 200)),
                    );
                    ui.spacing_mut().slider_width = SLIDER_LENGTH;
                    let mut level = view.level;
                    ui.add(
                        egui::Slider::new(&mut level, view.floor + 1..=view.top)
                            .vertical()
                            .integer()
                            .show_value(false),
                    )
                    .changed()
                    .then_some(level)
                })
                .inner
        })
        .inner
}

fn render_mode_label(ui: &mut egui::Ui) {
    egui::Area::new("rts_mode_label".into())
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(12.0, 8.0))
        .show(ui.ctx(), |ui| {
            ui.label(
                egui::RichText::new("Strategy")
                    .font(egui::FontId::proportional(16.0))
                    .color(egui::Color32::from_rgb(220, 220, 200)),
            );
        });
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
    use crate::overlays::overlay_manager::{HudIntents, ViewHeightHud};
    use crate::overlays::test_support::{
        input, rect_of, run_ui, second_frame, small_rect_centers, texts,
    };

    const FLOOR: i32 = 0;
    const TOP: i32 = 20;
    /// The handle is the only drawn rectangle with both sides under this.
    const HANDLE_MAX_SIDE: f32 = 20.0;
    /// The 12 px screen gap plus the 6 px frame margin the handle sits inside.
    const RIGHT_GAP: f32 = 18.0;

    fn strategy_data(level: i32) -> HudData {
        HudData {
            is_fps_mode: false,
            view_height: Some(ViewHeightHud {
                floor: FLOOR,
                top: TOP,
                level,
            }),
            ..Default::default()
        }
    }

    fn frame(
        ctx: &egui::Context,
        hud: &mut RtsHud,
        data: &HudData,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        run_ui(ctx, input(events), |ui| hud.render(ui, data))
    }

    /// The slider handle's centre once the HUD has settled; `None` when no slider is drawn.
    fn handle_of(data: &HudData) -> Option<egui::Pos2> {
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        frame(&ctx, &mut hud, data, Vec::new());
        small_rect_centers(&frame(&ctx, &mut hud, data, Vec::new()), HANDLE_MAX_SIDE)
            .first()
            .copied()
    }

    fn handle_at(level: i32) -> egui::Pos2 {
        handle_of(&strategy_data(level)).expect("the slider draws its handle")
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Presses the handle of a slider at `level`, drags to `to` and releases; returns the
    /// HUD, its context and the intents the drag left.
    fn drag_from_level(level: i32, to: egui::Pos2) -> (egui::Context, RtsHud, HudIntents) {
        let from = handle_at(level);
        let data = strategy_data(level);
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        frame(&ctx, &mut hud, &data, Vec::new());
        frame(&ctx, &mut hud, &data, vec![egui::Event::PointerMoved(from)]);
        frame(&ctx, &mut hud, &data, vec![button(from, true)]);
        frame(&ctx, &mut hud, &data, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut hud, &data, vec![button(to, false)]);
        let mut intents = HudIntents::default();
        hud.take_intents(&mut intents);
        (ctx, hud, intents)
    }

    #[test]
    fn dragging_the_slider_records_a_level() {
        let target = TOP - 8;
        let from = handle_at(TOP);
        let to = egui::pos2(from.x, handle_at(target).y);

        let (_ctx, mut hud, intents) = drag_from_level(TOP, to);

        assert_eq!(intents.view_level, Some(target));
        let mut again = HudIntents::default();
        hud.take_intents(&mut again);
        assert_eq!(
            again,
            HudIntents::default(),
            "the choice is handed over once"
        );
    }

    #[test]
    fn dragging_past_the_bottom_stops_one_layer_above_the_floor() {
        let lowest = handle_at(FLOOR + 1);
        let past_bottom = egui::pos2(lowest.x, lowest.y + 150.0);

        let (_ctx, _hud, intents) = drag_from_level(TOP - 3, past_bottom);

        assert_eq!(intents.view_level, Some(FLOOR + 1));
    }

    #[test]
    fn the_slider_sits_inside_the_screen() {
        let screen = input(Vec::new()).screen_rect.expect("tests set a screen");

        let handle = handle_at(TOP - 3);

        assert!(screen.contains(handle), "{handle:?} outside {screen:?}");
        assert!(
            handle.x <= screen.right() - RIGHT_GAP,
            "{handle:?} is pressed against the right edge of {screen:?}"
        );
    }

    #[test]
    fn a_drag_between_steps_settles_on_a_whole_layer() {
        let step = TOP - 10;
        let upper = handle_at(step);
        let lower = handle_at(step - 1);
        let between = egui::pos2(upper.x, upper.y + 0.4 * (lower.y - upper.y));

        let (ctx, mut hud, intents) = drag_from_level(TOP - 3, between);

        assert_eq!(
            intents.view_level,
            Some(step),
            "settles on the nearer step, not the one below"
        );
        let settled = strategy_data(step);
        frame(&ctx, &mut hud, &settled, Vec::new());
        let output = frame(&ctx, &mut hud, &settled, Vec::new());
        assert!(texts(&output).contains(&step.to_string()));
    }

    #[test]
    fn the_top_step_reads_all() {
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        let top = strategy_data(TOP);
        frame(&ctx, &mut hud, &top, Vec::new());
        let at_top = texts(&frame(&ctx, &mut hud, &top, Vec::new()));

        let below = strategy_data(TOP - 3);
        let ctx = egui::Context::default();
        frame(&ctx, &mut hud, &below, Vec::new());
        let lower = texts(&frame(&ctx, &mut hud, &below, Vec::new()));

        assert!(at_top.contains(&"All".to_string()), "{at_top:?}");
        assert!(!at_top.contains(&TOP.to_string()), "{at_top:?}");
        assert!(lower.contains(&(TOP - 3).to_string()), "{lower:?}");
        assert!(!lower.contains(&"All".to_string()), "{lower:?}");
    }

    #[test]
    fn no_slider_in_first_person() {
        let strategy = strategy_data(TOP - 3);
        let first_person = HudData {
            is_fps_mode: true,
            ..strategy_data(TOP - 3)
        };

        assert!(
            handle_of(&strategy).is_some(),
            "the slider shows in strategy"
        );
        assert!(handle_of(&first_person).is_none());
    }

    #[test]
    fn no_slider_without_a_world() {
        let world = strategy_data(TOP);
        let none = HudData {
            view_height: None,
            ..strategy_data(TOP)
        };

        assert!(handle_of(&world).is_some(), "the slider shows with a world");
        assert!(handle_of(&none).is_none());
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        frame(&ctx, &mut hud, &none, Vec::new());
        let output = frame(&ctx, &mut hud, &none, Vec::new());
        assert!(!texts(&output).contains(&"All".to_string()));
    }

    #[test]
    fn strategy_hud_names_the_mode() {
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        let strategy = strategy_data(TOP);
        frame(&ctx, &mut hud, &strategy, Vec::new());

        let drawn = texts(&frame(&ctx, &mut hud, &strategy, Vec::new()));

        assert!(drawn.contains(&"Strategy".to_string()), "{drawn:?}");
        assert!(!drawn.contains(&"First person".to_string()), "{drawn:?}");
    }

    #[test]
    fn strategy_hud_does_not_name_its_mode_in_first_person() {
        let ctx = egui::Context::default();
        let mut hud = RtsHud::new();
        let first_person = HudData {
            is_fps_mode: true,
            ..strategy_data(TOP)
        };
        frame(&ctx, &mut hud, &first_person, Vec::new());

        let drawn = texts(&frame(&ctx, &mut hud, &first_person, Vec::new()));

        assert!(!drawn.contains(&"Strategy".to_string()), "{drawn:?}");
    }

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
