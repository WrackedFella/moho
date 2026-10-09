//! In-game overlay components
//!
//! This module contains non-modal UI elements that appear during gameplay,
//! such as HUD elements, notifications, and interactive in-game menus.
//!
//! Overlays differ from screens in that they:
//! - Do not block the game (non-modal)
//! - Can be partially transparent
//! - May receive limited input (some pass-through to game)
//! - Can be shown/hidden independently
//!
//! The [`OverlayManager`] holds all registered [`Overlay`] layers and
//! renders them each frame with shared [`HudData`].

pub mod chunk_debug;
pub mod console;
pub mod debug_hud;
pub mod fps_hud;
pub mod gameplay_hud;
pub mod overlay_manager;
pub mod rts_hud;

pub use chunk_debug::ChunkDebugOverlay;
pub use console::{Console, ConsoleAction};
pub use debug_hud::DebugHud;
pub use fps_hud::FpsHud;
pub use gameplay_hud::GameplayHud;
pub use overlay_manager::{HudData, Overlay, OverlayManager};
pub use rts_hud::RtsHud;

#[cfg(test)]
pub(crate) mod test_support {
    use egui::epaint::Shape;

    fn collect_text(shape: &Shape, out: &mut Vec<String>) {
        match shape {
            Shape::Text(text) => out.push(text.galley.text().to_string()),
            Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_text(s, out)),
            _ => {}
        }
    }

    pub(crate) fn input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        }
    }

    /// Returns the second frame's output; egui sizes areas on the first.
    pub(crate) fn second_frame(
        ctx: &egui::Context,
        mut add_contents: impl FnMut(&egui::Context),
    ) -> egui::FullOutput {
        let _ = ctx.run(input(Vec::new()), &mut add_contents);
        ctx.run(input(Vec::new()), &mut add_contents)
    }

    pub(crate) fn texts(output: &egui::FullOutput) -> Vec<String> {
        let mut out = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::second_frame;
    use super::*;

    fn draws(overlay: &mut dyn Overlay, is_fps_mode: bool) -> bool {
        let ctx = egui::Context::default();
        let data = HudData {
            is_fps_mode,
            ..Default::default()
        };

        !second_frame(&ctx, |ctx| overlay.render(ctx, &data))
            .shapes
            .is_empty()
    }

    #[test]
    fn hud_default_visibility() {
        let cases: [(&str, Box<dyn Overlay>, bool); 4] = [
            ("debug", Box::new(DebugHud::new()), false),
            ("fps", Box::new(FpsHud::new()), true),
            ("rts", Box::new(RtsHud::new()), true),
            ("gameplay", Box::new(GameplayHud::new()), true),
        ];

        for (case, hud, expected) in cases {
            assert_eq!(hud.is_visible(), expected, "{case} hud default visibility");
        }
    }

    #[test]
    fn hud_draws_only_in_its_mode() {
        assert!(draws(&mut FpsHud::new(), true), "fps hud in fps mode");
        assert!(
            !draws(&mut FpsHud::new(), false),
            "fps hud outside fps mode"
        );
        assert!(
            draws(&mut GameplayHud::new(), true),
            "gameplay hud in fps mode"
        );
        assert!(
            !draws(&mut GameplayHud::new(), false),
            "gameplay hud outside fps mode"
        );
        assert!(!draws(&mut RtsHud::new(), true), "rts hud in fps mode");
        assert!(draws(&mut RtsHud::new(), false), "rts hud outside fps mode");
    }
}
