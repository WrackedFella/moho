//! Game-drawn screen text: immediate-mode text a game queues each frame and the shell paints.

/// The point of the screen a text is positioned from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// One line of text drawn over the game for a single frame.
///
/// `offset` is in logical points on screen axes (x right, y down) from the anchor point; `color`
/// is straight-alpha sRGBA.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenText {
    pub text: String,
    pub anchor: Anchor,
    pub offset: [f32; 2],
    pub size: f32,
    pub color: [u8; 4],
}

/// The point of `screen` that `anchor` names.
pub(crate) fn anchor_point(_anchor: Anchor, _screen: egui::Rect) -> egui::Pos2 {
    todo!()
}

/// Paints `texts` on the background layer, under the console.
pub fn paint(_ctx: &egui::Context, _texts: &[ScreenText]) {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::console::Console;
    use crate::console::tests::Cmd;

    const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    /// What a painted text looks like in the output.
    #[derive(Debug)]
    struct Painted {
        text: String,
        size: f32,
        color: [u8; 4],
        rect: egui::Rect,
    }

    fn screen(width: f32, height: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))
    }

    fn input(screen_rect: egui::Rect) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen_rect),
            ..egui::RawInput::default()
        }
    }

    fn fps(text: &str, anchor: Anchor, offset: [f32; 2]) -> ScreenText {
        ScreenText {
            text: text.to_owned(),
            anchor,
            offset,
            size: 24.0,
            color: [255, 255, 255, 255],
        }
    }

    fn collect(shape: &egui::Shape, out: &mut Vec<Painted>) {
        match shape {
            egui::Shape::Text(t) => {
                let section = &t.galley.job.sections[0];
                out.push(Painted {
                    text: t.galley.job.text.clone(),
                    size: section.format.font_id.size,
                    color: section.format.color.to_srgba_unmultiplied(),
                    rect: t.galley.rect.translate(t.pos.to_vec2()),
                });
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }

    fn painted(output: &egui::FullOutput) -> Vec<Painted> {
        let mut out = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    fn pass(ctx: &egui::Context, size: egui::Rect, texts: &[ScreenText]) -> Vec<Painted> {
        let mut output = ctx.run_ui(input(size), |ui| paint(ui.ctx(), texts));
        let shapes = painted(&output);
        output.textures_delta.clear();
        shapes
    }

    /// The point of a rect that an anchor names: a screen's anchor point, or the part of a text's
    /// rect aligned to it.
    fn aligned_point(rect: egui::Rect, anchor: Anchor) -> egui::Pos2 {
        match anchor {
            Anchor::TopLeft => rect.left_top(),
            Anchor::Top => rect.center_top(),
            Anchor::TopRight => rect.right_top(),
            Anchor::Left => rect.left_center(),
            Anchor::Center => rect.center(),
            Anchor::Right => rect.right_center(),
            Anchor::BottomLeft => rect.left_bottom(),
            Anchor::Bottom => rect.center_bottom(),
            Anchor::BottomRight => rect.right_bottom(),
        }
    }

    #[test]
    fn drawn_text_is_in_the_frame_at_anchor_size_and_colour() {
        let ctx = egui::Context::default();
        let texts = [fps("30 / 30", Anchor::BottomRight, [-10.0, -12.0])];

        let shapes = pass(&ctx, screen(800.0, 600.0), &texts);

        assert_eq!(shapes.len(), 1, "{shapes:?}");
        let text = &shapes[0];
        assert_eq!(text.text, "30 / 30");
        assert_eq!(text.size, 24.0);
        assert_eq!(text.color, [255, 255, 255, 255]);
        assert!((text.rect.right() - 790.0).abs() <= 1.0, "{:?}", text.rect);
        assert!((text.rect.bottom() - 588.0).abs() <= 1.0, "{:?}", text.rect);
    }

    #[test]
    fn only_this_frames_text_is_drawn() {
        let ctx = egui::Context::default();
        let size = screen(800.0, 600.0);
        pass(
            &ctx,
            size,
            &[fps("30 / 30", Anchor::BottomRight, [-10.0, -10.0])],
        );

        let second = pass(
            &ctx,
            size,
            &[fps("29 / 30", Anchor::BottomRight, [-10.0, -10.0])],
        );

        let texts: Vec<&str> = second.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["29 / 30"]);
    }

    #[test]
    fn frame_without_draws_holds_no_text() {
        let ctx = egui::Context::default();

        let shapes = pass(&ctx, screen(800.0, 600.0), &[]);

        assert!(shapes.is_empty(), "{shapes:?}");
    }

    #[test]
    fn anchored_text_keeps_its_offset_after_resize() {
        let ctx = egui::Context::default();
        let texts = [fps("30 / 30", Anchor::BottomRight, [-10.0, -12.0])];

        let before = pass(&ctx, screen(800.0, 600.0), &texts);
        let after = pass(&ctx, screen(1280.0, 720.0), &texts);

        assert_eq!(before.len(), 1, "{before:?}");
        assert_eq!(after.len(), 1, "{after:?}");
        assert!((before[0].rect.right() - 790.0).abs() <= 1.0);
        assert!((before[0].rect.bottom() - 588.0).abs() <= 1.0);
        assert!(
            (after[0].rect.right() - 1270.0).abs() <= 1.0,
            "{:?}",
            after[0].rect
        );
        assert!(
            (after[0].rect.bottom() - 708.0).abs() <= 1.0,
            "{:?}",
            after[0].rect
        );
    }

    #[test]
    fn text_paints_before_the_console() {
        let ctx = egui::Context::default();
        let (mut console, _) = Console::<Cmd>::new(&BTreeMap::new());
        console.open();
        let texts = [fps("30 / 30", Anchor::TopLeft, [0.0, 0.0])];

        let mut output = ctx.run_ui(input(screen(800.0, 600.0)), |ui| {
            paint(ui.ctx(), &texts);
            console.show(ui.ctx());
        });

        let mut order = Vec::new();
        for clipped in &output.shapes {
            let mut found = Vec::new();
            collect(&clipped.shape, &mut found);
            order.push(found.iter().any(|p| p.text == "30 / 30"));
        }
        output.textures_delta.clear();
        let first = order.iter().position(|&is_game_text| is_game_text);
        assert_eq!(
            first,
            Some(0),
            "game text is not the first shape: {order:?}"
        );
        assert!(order.len() > 1, "the console painted nothing after it");
    }

    proptest! {
        #[test]
        fn anchor_point_aligns_text(
            index in 0..ALL.len(),
            width in 200.0f32..3000.0,
            height in 200.0f32..2000.0,
            dx in -50.0f32..50.0,
            dy in -50.0f32..50.0,
        ) {
            let anchor = ALL[index];
            let ctx = egui::Context::default();
            let size = screen(width, height);
            let texts = [fps("30 / 30", anchor, [dx, dy])];

            let shapes = pass(&ctx, size, &texts);

            prop_assert_eq!(shapes.len(), 1);
            let point = aligned_point(size, anchor) + egui::vec2(dx, dy);
            let at = aligned_point(shapes[0].rect, anchor);
            prop_assert!((at.x - point.x).abs() <= 1.0, "{at:?} vs {point:?}");
            prop_assert!((at.y - point.y).abs() <= 1.0, "{at:?} vs {point:?}");
        }
    }
}
