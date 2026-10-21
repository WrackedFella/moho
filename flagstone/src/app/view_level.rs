//! The strategy view-height slider's level: which block layer the world is cut at.

use crate::{App, RenderRequest};
use flagstone_game::controller::CameraMode;
use flagstone_ui::overlays::ViewHeightHud;
use glam::IVec3;

/// How far above a layer boundary the renderer's cut sits.
///
/// The kept layer's top faces sit exactly on the boundary; an interpolated `y` a few ulps
/// over it would otherwise be discarded as speckle by the renderer's `y > cut` test.
pub const CUT_ABOVE_LAYER: f32 = 1.0 / 256.0;

/// The block layers the loaded chunks span: the slider runs over `floor + 1 ..= top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerExtent {
    pub floor: i32,
    pub top: i32,
}

impl LayerExtent {
    /// The span of the chunks at `positions`; `None` with no chunks loaded.
    pub fn of_chunks(positions: impl IntoIterator<Item = IVec3>, chunk_size: i32) -> Option<Self> {
        let (min_cy, max_cy) =
            positions
                .into_iter()
                .fold(None, |span: Option<(i32, i32)>, p| {
                    Some(span.map_or((p.y, p.y), |(lo, hi)| (lo.min(p.y), hi.max(p.y))))
                })?;
        Some(Self {
            floor: min_cy * chunk_size,
            top: (max_cy + 1) * chunk_size,
        })
    }

    /// The stored cut as `extent` reads it: the top is no cut, the lowest step is the floor.
    fn clamp(self, cut: Option<i32>) -> Option<i32> {
        cut.filter(|&level| level < self.top)
            .map(|level| level.max(self.floor + 1))
    }
}

/// The cut the renderer was last told, distinct from no news at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RendererCut {
    /// Draw the whole world.
    Whole,
    /// Discard everything above this height.
    At(f32),
}

impl RendererCut {
    /// The height as the renderer's request carries it; `None` draws the whole world.
    pub fn height(self) -> Option<f32> {
        match self {
            Self::Whole => None,
            Self::At(height) => Some(height),
        }
    }
}

impl From<Option<f32>> for RendererCut {
    fn from(height: Option<f32>) -> Self {
        height.map_or(Self::Whole, Self::At)
    }
}

/// The player's chosen view height and what the renderer was last given for it.
#[derive(Debug, Default)]
pub struct ViewLevel {
    cut: Option<i32>,
    applied: Option<RendererCut>,
}

impl ViewLevel {
    /// Record the player's pick; the top clears the cut and anything below the lowest
    /// step clamps to it.
    pub fn choose(&mut self, level: i32, extent: LayerExtent) {
        self.cut = extent.clamp(Some(level));
    }

    /// What the slider shows for `extent`.
    pub fn hud(&self, extent: LayerExtent) -> ViewHeightHud {
        ViewHeightHud {
            floor: extent.floor,
            top: extent.top,
            level: extent.clamp(self.cut).unwrap_or(extent.top),
        }
    }

    /// The height the renderer should cut at in `mode`.
    pub fn renderer_height(&self, mode: CameraMode, extent: LayerExtent) -> Option<f32> {
        match mode {
            CameraMode::FirstPerson => None,
            CameraMode::Isometric => extent
                .clamp(self.cut)
                .map(|cut| cut as f32 + CUT_ABOVE_LAYER),
        }
    }

    /// The renderer height, only when it differs from the last one returned.
    pub fn take_change(&mut self, mode: CameraMode, extent: LayerExtent) -> Option<RendererCut> {
        let height = RendererCut::from(self.renderer_height(mode, extent));
        if self.applied == Some(height) {
            return None;
        }
        self.applied = Some(height);
        Some(height)
    }
}

/// Apply the HUD's pick and hand a changed cut to the caps and the renderer.
pub fn update(app: &mut App, chosen: Option<i32>) {
    let Some(light_system) = app.light_system.as_ref() else {
        return;
    };
    let grid = light_system.grid();
    let Some(extent) = LayerExtent::of_chunks(grid.chunk_positions(), grid.chunk_size()) else {
        return;
    };
    if let Some(level) = chosen {
        app.view_level.choose(level, extent);
    }
    let mode = app.simulation.camera_mode();
    if let Some(cut) = app.view_level.take_change(mode, extent) {
        app.cut_caps.set_cut(cut.height());
        app.pending_render
            .push(RenderRequest::ViewHeight(cut.height()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cut_caps::tests::ground_app;
    use proptest::prelude::*;

    const EXTENT: LayerExtent = LayerExtent { floor: 0, top: 144 };

    /// `ground_app` starts possessing a pawn; the slider is a strategy-view tool.
    fn strategy_app() -> App {
        let mut app = ground_app();
        app.simulation.set_camera_mode(CameraMode::Isometric);
        app
    }

    fn queued_heights(app: &App) -> Vec<Option<f32>> {
        app.pending_render
            .iter()
            .map(|request| match request {
                RenderRequest::ViewHeight(height) => *height,
                _ => panic!("only view heights are queued here"),
            })
            .collect()
    }

    #[test]
    fn a_new_level_has_no_cut() {
        let level = ViewLevel::default();

        assert_eq!(level.renderer_height(CameraMode::Isometric, EXTENT), None);
        assert_eq!(level.hud(EXTENT).level, EXTENT.top);
        assert_eq!(level.hud(EXTENT).floor, EXTENT.floor);
        assert_eq!(level.hud(EXTENT).top, EXTENT.top);
    }

    #[test]
    fn n_steps_down_cuts_above_that_layer() {
        for steps in [1, 5] {
            let mut level = ViewLevel::default();

            level.choose(EXTENT.top - steps, EXTENT);

            assert_eq!(
                level.renderer_height(CameraMode::Isometric, EXTENT),
                Some((EXTENT.top - steps) as f32 + CUT_ABOVE_LAYER),
                "{steps} steps down"
            );
            assert_eq!(level.hud(EXTENT).level, EXTENT.top - steps);
        }
    }

    #[test]
    fn choosing_the_top_clears_the_cut() {
        let mut level = ViewLevel::default();
        level.choose(EXTENT.top - 3, EXTENT);
        assert!(
            level
                .renderer_height(CameraMode::Isometric, EXTENT)
                .is_some()
        );

        level.choose(EXTENT.top, EXTENT);

        assert_eq!(level.renderer_height(CameraMode::Isometric, EXTENT), None);
        assert_eq!(level.hud(EXTENT).level, EXTENT.top);
    }

    #[test]
    fn a_choice_outside_the_extent_clamps() {
        let mut level = ViewLevel::default();

        level.choose(EXTENT.top + 5, EXTENT);
        assert_eq!(level.renderer_height(CameraMode::Isometric, EXTENT), None);
        assert_eq!(level.hud(EXTENT).level, EXTENT.top);

        level.choose(EXTENT.floor - 10, EXTENT);
        assert_eq!(
            level.renderer_height(CameraMode::Isometric, EXTENT),
            Some((EXTENT.floor + 1) as f32 + CUT_ABOVE_LAYER)
        );
        assert_eq!(level.hud(EXTENT).level, EXTENT.floor + 1);
    }

    #[test]
    fn a_stored_cut_reads_clamped_in_a_changed_extent_without_being_rewritten() {
        let mut level = ViewLevel::default();
        level.choose(100, EXTENT);
        let shallow = LayerExtent { floor: 10, top: 50 };
        let high_floor = LayerExtent {
            floor: 110,
            top: 200,
        };

        assert_eq!(level.hud(shallow).level, shallow.top);
        assert_eq!(level.hud(high_floor).level, high_floor.floor + 1);
        assert_eq!(level.hud(EXTENT).level, 100, "the choice itself is kept");
    }

    #[test]
    fn extent_spans_loaded_chunks() {
        let tall = (0..=8).map(|y| IVec3::new(3, y, -2));
        let below_zero = [IVec3::new(0, -2, 0), IVec3::new(1, 1, 0)];

        assert_eq!(
            LayerExtent::of_chunks(tall, 16),
            Some(LayerExtent { floor: 0, top: 144 })
        );
        assert_eq!(
            LayerExtent::of_chunks(below_zero, 16),
            Some(LayerExtent {
                floor: -32,
                top: 32
            })
        );
        assert_eq!(LayerExtent::of_chunks(std::iter::empty(), 16), None);
    }

    #[test]
    fn possessing_a_pawn_lifts_the_cut_and_returning_restores_it() {
        let mut level = ViewLevel::default();
        level.choose(EXTENT.top - 3, EXTENT);
        let cut = RendererCut::At((EXTENT.top - 3) as f32 + CUT_ABOVE_LAYER);

        let in_strategy = level.take_change(CameraMode::Isometric, EXTENT);
        let possessed = level.take_change(CameraMode::FirstPerson, EXTENT);
        let still_possessed = level.take_change(CameraMode::FirstPerson, EXTENT);
        let returned = level.take_change(CameraMode::Isometric, EXTENT);

        assert_eq!(in_strategy, Some(cut));
        assert_eq!(possessed, Some(RendererCut::Whole));
        assert_eq!(still_possessed, None);
        assert_eq!(returned, Some(cut));
        assert_eq!(level.hud(EXTENT).level, EXTENT.top - 3, "the level stays");
    }

    #[test]
    fn an_unchanged_level_pushes_nothing() {
        let mut app = strategy_app();
        update(&mut app, Some(27));
        assert_eq!(queued_heights(&app).len(), 1, "the move itself is queued");
        app.pending_render.clear();

        update(&mut app, None);
        update(&mut app, Some(27));

        assert!(queued_heights(&app).is_empty());
    }

    #[test]
    fn a_choice_reaches_the_render_queue() {
        let mut app = strategy_app();

        update(&mut app, Some(27));

        assert_eq!(queued_heights(&app), [Some(27.0 + CUT_ABOVE_LAYER)]);
    }

    #[test]
    fn a_choice_sets_the_cut_caps() {
        let mut app = strategy_app();

        update(&mut app, Some(27));

        let queued = queued_heights(&app);
        assert_eq!(app.cut_caps.cut(), Some(27.0 + CUT_ABOVE_LAYER));
        assert_eq!(queued, [app.cut_caps.cut()]);
    }

    #[test]
    fn possessing_a_pawn_takes_the_caps_and_the_renderer_cut_away() {
        let mut app = strategy_app();
        update(&mut app, Some(27));
        app.pending_render.clear();

        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        update(&mut app, None);

        assert_eq!(queued_heights(&app), [None]);
        assert_eq!(app.cut_caps.cut(), None);

        app.pending_render.clear();
        app.simulation.set_camera_mode(CameraMode::Isometric);
        update(&mut app, None);

        assert_eq!(queued_heights(&app), [Some(27.0 + CUT_ABOVE_LAYER)]);
        assert_eq!(app.cut_caps.cut(), Some(27.0 + CUT_ABOVE_LAYER));
    }

    proptest! {
        #[test]
        fn every_level_cuts_on_a_layer_boundary(
            floor in -1000_i32..1000,
            span in 2_i32..500,
            chosen in -3000_i32..3000,
        ) {
            let extent = LayerExtent { floor, top: floor + span };
            let mut level = ViewLevel::default();

            level.choose(chosen, extent);

            if let Some(height) = level.renderer_height(CameraMode::Isometric, extent) {
                let boundary = height - CUT_ABOVE_LAYER;
                prop_assert_eq!(boundary.fract(), 0.0, "cut {} is off a layer boundary", height);
                prop_assert!(
                    boundary >= (extent.floor + 1) as f32 && boundary < extent.top as f32,
                    "cut {} outside the steps {}..{}", height, extent.floor + 1, extent.top
                );
            } else {
                prop_assert!(chosen >= extent.top, "level {} below the top lost its cut", chosen);
            }
        }
    }
}
