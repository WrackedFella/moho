//! The look behind an open menu screen: the scene dimmed, and a panel under the rows.

use crate::menu::Rect;
use crate::text::ScreenText;

/// Black over the whole window while a menu screen is open.
pub(crate) const DIM: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 0, 0, 153);
/// The panel behind a screen's rows: rgb(16, 16, 24) at alpha 240, premultiplied.
pub(crate) const PANEL: egui::Color32 = egui::Color32::from_rgba_premultiplied(15, 15, 23, 240);
/// Space between the outermost row or text and the panel's edge, in points.
const PANEL_MARGIN: f32 = 16.0;
const PANEL_ROUNDING: f32 = 8.0;

/// Paints the dim over the whole window, then the panel, then `texts` on it. The panel is the
/// union of `rows` and the painted texts' rects, grown by a margin and kept on screen; no rows
/// and no texts paint no panel.
pub(crate) fn paint_menu(ctx: &egui::Context, texts: &[ScreenText], rows: &[Rect]) {
    let painter = ctx.layer_painter(crate::text::screen_layer());
    let window = ctx.content_rect();
    painter.rect_filled(window, 0.0, DIM);
    // The panel's size depends on the texts' laid-out rects, which exist only once painted, so
    // its slot is reserved here to keep it under them.
    let slot = painter.add(egui::Shape::Noop);
    let painted = crate::text::paint(ctx, texts);
    let bounds = rows
        .iter()
        .map(|r| egui::Rect::from_min_max(r.min.into(), r.max.into()))
        .chain(painted)
        .reduce(egui::Rect::union);
    if let Some(bounds) = bounds {
        let panel = bounds.expand(PANEL_MARGIN).intersect(window);
        painter.set(slot, egui::Shape::rect_filled(panel, PANEL_ROUNDING, PANEL));
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::menu::{MenuItem, MenuScreen, MenuStack, TEXT_COLORS};
    use crate::settings::{EngineSetting, EngineSettings};
    use crate::text::Anchor;

    fn screen(width: f32, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))
    }

    fn flatten<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
            egui::Shape::Noop => {}
            other => out.push(other),
        }
    }

    /// Paints with `paint` on a window of the given size in points and returns the shapes in
    /// paint order, with groups flattened and no-ops dropped.
    fn shapes_of(
        size: egui::Rect,
        mut paint: impl FnMut(&egui::Context),
    ) -> (Vec<egui::Shape>, egui::Rect) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(size),
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| paint(ui.ctx()));
        output.textures_delta.clear();
        let mut flat = Vec::new();
        for clipped in &output.shapes {
            flatten(&clipped.shape, &mut flat);
        }
        (flat.into_iter().cloned().collect(), size)
    }

    fn filled(shape: &egui::Shape) -> Option<(egui::Rect, egui::Color32)> {
        match shape {
            egui::Shape::Rect(rect) => Some((rect.rect, rect.fill)),
            _ => None,
        }
    }

    fn text_string(shape: &egui::Shape) -> Option<&str> {
        match shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        }
    }

    fn label(text: &str, offset: [f32; 2]) -> ScreenText {
        ScreenText {
            text: text.to_owned(),
            anchor: Anchor::Center,
            offset,
            size: 36.0,
            color: TEXT_COLORS[0],
        }
    }

    #[test]
    fn paint_menu_paints_a_window_wide_dim_then_the_panel_then_the_texts() {
        let window = screen(1280.0, 720.0);
        let texts = [label("Play", [0.0, -56.0]), label("Quit", [0.0, 0.0])];
        let rows = [Rect {
            min: [520.0, 300.0],
            max: [760.0, 356.0],
        }];

        let (shapes, _) = shapes_of(window, |ctx| paint_menu(ctx, &texts, &rows));

        assert_eq!(shapes.len(), 4, "dim, panel and two texts: {shapes:?}");
        let (dim_rect, dim_fill) = filled(&shapes[0]).expect("the dim is a filled rect first");
        assert_eq!(dim_rect, window, "the dim covers the whole window");
        assert_eq!(
            dim_fill,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 153)
        );
        let (panel_rect, panel_fill) =
            filled(&shapes[1]).expect("the panel is a filled rect second");
        assert_eq!(
            panel_fill,
            egui::Color32::from_rgba_unmultiplied(16, 16, 24, 240)
        );
        assert!(window.contains_rect(panel_rect), "{panel_rect:?}");
        assert!(
            panel_rect.contains_rect(egui::Rect::from_min_max(
                egui::pos2(520.0, 300.0),
                egui::pos2(760.0, 356.0)
            )),
            "the panel holds the row: {panel_rect:?}"
        );
        assert_eq!(text_string(&shapes[2]), Some("Play"));
        assert_eq!(text_string(&shapes[3]), Some("Quit"));
    }

    #[test]
    fn a_screen_with_no_rows_or_title_draws_the_dim_and_no_panel() {
        let window = screen(1280.0, 720.0);

        let (shapes, _) = shapes_of(window, |ctx| paint_menu(ctx, &[], &[]));

        assert_eq!(shapes.len(), 1, "only the dim: {shapes:?}");
        let (rect, fill) = filled(&shapes[0]).expect("the dim is a filled rect");
        assert_eq!(rect, window);
        assert_eq!(fill, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 153));
    }

    const WHITE: f32 = 1.0;
    const BLACK: f32 = 0.0;
    /// The main pass clears to black.
    const CLEAR: f32 = 0.0;

    fn to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(linear: [f32; 3]) -> f32 {
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    /// The contrast of an opaque text colour over a grey scene (sRGB value `grey`) under the dim
    /// and the panel, with the layers blended in linear space or in gamma space.
    fn contrast(text: [u8; 4], grey: f32, linear_blend: bool) -> f32 {
        let space = |c: f32| if linear_blend { to_linear(c) } else { c };
        let mut shown = [space(grey); 3];
        for layer in [DIM, PANEL] {
            let [r, g, b, a] = layer.to_srgba_unmultiplied();
            let alpha = f32::from(a) / 255.0;
            for (acc, c) in shown.iter_mut().zip([r, g, b]) {
                *acc = space(f32::from(c) / 255.0) * alpha + *acc * (1.0 - alpha);
            }
        }
        let background = luminance(shown.map(|c| if linear_blend { c } else { to_linear(c) }));
        let [r, g, b, _] = text;
        let foreground = luminance([r, g, b].map(|c| to_linear(f32::from(c) / 255.0)));
        let (hi, lo) = if foreground > background {
            (foreground, background)
        } else {
            (background, foreground)
        };
        (hi + 0.05) / (lo + 0.05)
    }

    fn assert_readable(grey: f32) -> Result<(), TestCaseError> {
        for text in TEXT_COLORS {
            for linear_blend in [true, false] {
                let ratio = contrast(text, grey, linear_blend);
                prop_assert!(
                    ratio >= 4.5,
                    "{text:?} over grey {grey} (linear blend {linear_blend}) is {ratio}:1"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn every_menu_text_colour_has_at_least_4_5_to_1_over_any_background() {
        for grey in [WHITE, BLACK, CLEAR] {
            assert_readable(grey).expect("a pure white, black or clear scene stays readable");
        }
    }

    proptest! {
        #[test]
        fn every_menu_text_colour_is_readable_over_any_grey(grey in 0.0f32..=1.0) {
            assert_readable(grey)?;
        }
    }

    fn settings_values() -> EngineSettings {
        EngineSettings {
            mouse_sensitivity: 2.5,
            mouse_smoothing: false,
            invert_mouse_y: false,
            sound_effect_volume: 8.0,
            music_volume: 6.0,
            ui_volume: 7.0,
            shadow_quality: 3,
            ssao_quality: 3,
            effects_quality: 3,
            window_mode: moho_core::prefs::WindowMode::Windowed,
            resolution: (1920, 1080),
        }
    }

    fn settings_screen() -> MenuScreen<()> {
        let mut items: Vec<MenuItem<()>> = EngineSetting::ALL
            .into_iter()
            .map(MenuItem::Setting)
            .collect();
        items.push(MenuItem::Apply);
        items.push(MenuItem::Button {
            label: "Back".to_owned(),
            action: (),
        });
        MenuScreen {
            title: Some("Settings".to_owned()),
            items,
            back: Some(()),
            settings: Some(settings_values()),
        }
    }

    fn controls_screen() -> MenuScreen<()> {
        let items = (0..30)
            .map(|index| MenuItem::Control {
                label: format!("Action {index:02}"),
                action: (),
                slots: [None, None],
                note: None,
            })
            .collect();
        MenuScreen {
            title: Some("Controls".to_owned()),
            items,
            back: Some(()),
            settings: None,
        }
    }

    #[test]
    fn the_panel_holds_every_row_text() {
        let windows = [
            [1280.0, 720.0],
            [1920.0, 1080.0],
            // 1920 by 1080 pixels at 125 %: layout sees points only.
            [1536.0, 864.0],
        ];
        for size in windows {
            for (name, build) in [
                ("settings", settings_screen as fn() -> MenuScreen<()>),
                ("controls", controls_screen),
            ] {
                for scrolled_to_end in [false, true] {
                    let mut stack = MenuStack::new();
                    stack.push(build());
                    // Laying out once records the window size the wheel scrolls by.
                    let _ = stack.texts(size);
                    if scrolled_to_end {
                        stack.scroll(1000.0);
                    }
                    let texts = stack.texts(size);
                    let rows = stack.visible_rows(size);
                    let case = format!("{name} on {size:?}, scrolled to end {scrolled_to_end}");

                    let (shapes, _) = shapes_of(screen(size[0], size[1]), |ctx| {
                        paint_menu(ctx, &texts, &rows);
                    });

                    let panel = shapes
                        .get(1)
                        .and_then(filled)
                        .unwrap_or_else(|| panic!("{case}: no panel in {shapes:?}"))
                        .0;
                    let held: Vec<&egui::Shape> = shapes
                        .iter()
                        .filter(|shape| matches!(shape, egui::Shape::Text(_)))
                        .collect();
                    assert_eq!(held.len(), texts.len(), "{case}: every text is painted");
                    for shape in held {
                        let text = shape.visual_bounding_rect();
                        assert!(
                            panel.contains_rect(text),
                            "{case}: {:?} at {text:?} is outside the panel {panel:?}",
                            text_string(shape)
                        );
                    }
                }
            }
        }
    }
}
