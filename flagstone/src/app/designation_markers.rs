//! The markers that show which blocks are marked for mining.

use flagstone_game::colony::MineMarks;
use moho_render_api::{Marker, MarkerId};
use moho_renderer::Markers;
use std::collections::BTreeSet;

/// Translucent red-orange, apart from the console's amber.
const MARK_COLOR: [f32; 4] = [0.9, 0.2, 0.15, 0.45];
const COORD_MASK: u64 = (1 << 20) - 1;

/// The blocks whose marker is on the scene, so a frame draws only what changed.
#[derive(Debug, Default)]
pub struct DesignationMarkers {
    shown: BTreeSet<[i32; 3]>,
}

impl DesignationMarkers {
    /// Adds a marker for each new mark and removes the marker of each cancelled one.
    /// Markers it did not place are left alone.
    pub fn sync(&mut self, marks: &MineMarks, ceiling: Option<i32>, markers: &mut Markers) {
        let _ = ceiling;
        for (pos, _) in marks.iter() {
            if self.shown.insert(pos) {
                let min = pos.map(|c| c as f32);
                let max = pos.map(|c| c as f32 + 1.0);
                markers.upsert(marker_id(pos), Marker::new(min, max, MARK_COLOR));
            }
        }
        self.shown.retain(|&pos| {
            let keep = marks.contains(pos);
            if !keep {
                markers.remove(marker_id(pos));
            }
            keep
        });
    }
}

/// The marker id of the block at `pos`: bit 63 set, then x, y and z in 20 bits each.
pub fn marker_id(pos: [i32; 3]) -> MarkerId {
    let [x, y, z] = pos.map(|c| u64::from(c as u32) & COORD_MASK);
    MarkerId(1 << 63 | x << 40 | y << 20 | z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use moho_render_api::{Marker, MarkerGpu};
    use moho_renderer::MARKER_INFLATE;
    use proptest::prelude::*;

    const CONSOLE_ID: MarkerId = MarkerId(3);

    fn marks_at(positions: &[[i32; 3]]) -> MineMarks {
        let mut marks = MineMarks::default();
        for &pos in positions {
            marks.toggle(pos);
        }
        marks
    }

    fn fills(markers: &Markers) -> Vec<MarkerGpu> {
        markers.draws(Vec3::ZERO).fills
    }

    /// The fill whose box surrounds the unit cell at `pos`.
    fn fill_at(markers: &Markers, pos: [i32; 3]) -> Option<MarkerGpu> {
        fills(markers).into_iter().find(|fill| {
            (0..3).all(|axis| {
                let lo = pos[axis] as f32 - MARKER_INFLATE;
                (fill.min[axis] - lo).abs() < 1e-4
                    && (fill.max[axis] - (lo + 1.0 + 2.0 * MARKER_INFLATE)).abs() < 1e-4
            })
        })
    }

    #[test]
    fn sync_adds_new_marks_removes_cancelled_ones_and_touches_nothing_else() {
        let mut markers = Markers::new();
        let console = Marker::new([50.0; 3], [52.0; 3], [1.0, 0.75, 0.0, 0.4]);
        markers.upsert(CONSOLE_ID, console);
        let mut shown = DesignationMarkers::default();
        let mut marks = marks_at(&[[1, 2, 3], [4, 5, 6]]);

        shown.sync(&marks, None, &mut markers);
        let after_marking = fills(&markers).len();
        let first = fill_at(&markers, [1, 2, 3]);
        let second = fill_at(&markers, [4, 5, 6]);
        marks.toggle([1, 2, 3]);
        marks.toggle([7, 8, 9]);
        shown.sync(&marks, None, &mut markers);

        assert_eq!(after_marking, 3, "two marks and the console's marker");
        let first = first.expect("a marker spans the first marked block's cell");
        assert!(first.color[3] > 0.0 && first.color[3] < 1.0, "translucent");
        assert_ne!(
            first.color,
            [1.0, 0.75, 0.0, 0.4],
            "unlike the console's colour"
        );
        assert!(second.is_some(), "a marker spans the second block's cell");
        assert!(
            fill_at(&markers, [1, 2, 3]).is_none(),
            "cancelled mark's marker is gone"
        );
        assert!(
            fill_at(&markers, [4, 5, 6]).is_some(),
            "kept mark keeps its marker"
        );
        assert!(
            fill_at(&markers, [7, 8, 9]).is_some(),
            "new mark gets a marker"
        );
        assert_eq!(
            fills(&markers).len(),
            3,
            "the console's marker is untouched"
        );
    }

    #[test]
    fn sync_with_no_change_makes_no_renderer_calls() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let marks = marks_at(&[[1, 2, 3]]);
        shown.sync(&marks, None, &mut markers);
        let sentinel = Marker::new([0.0; 3], [1.0; 3], [0.0, 1.0, 0.0, 0.5]);
        markers.upsert(marker_id([1, 2, 3]), sentinel);

        shown.sync(&marks, None, &mut markers);

        assert_eq!(
            fills(&markers)[0].color,
            sentinel.color,
            "an unchanged mark's marker was written again"
        );
    }

    #[test]
    fn only_marks_below_the_ceiling_have_markers() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let marks = marks_at(&[[1, 2, 3], [0, 3, 0], [0, 4, 0], [0, 5, 0], [1, 7, 3]]);

        shown.sync(&marks, Some(4), &mut markers);

        assert!(fill_at(&markers, [1, 2, 3]).is_some(), "below the cut");
        assert!(fill_at(&markers, [0, 3, 0]).is_some(), "the top kept layer");
        assert!(
            fill_at(&markers, [1, 7, 3]).is_none(),
            "far above the cut is not drawn"
        );
        assert!(
            fill_at(&markers, [0, 4, 0]).is_none(),
            "y == ceiling is hidden"
        );
        assert!(
            fill_at(&markers, [0, 5, 0]).is_none(),
            "y > ceiling is hidden"
        );
        assert_eq!(fills(&markers).len(), 2);
    }

    #[test]
    fn raising_the_ceiling_shows_the_mark_again_and_lowering_hides_it() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let marks = marks_at(&[[1, 2, 3], [1, 6, 3]]);
        let sentinel = Marker::new([0.0; 3], [1.0; 3], [0.0, 1.0, 0.0, 0.5]);
        shown.sync(&marks, Some(5), &mut markers);
        markers.upsert(marker_id([1, 2, 3]), sentinel);

        shown.sync(&marks, Some(5), &mut markers);
        let unchanged = fill_at_id(&markers, sentinel);
        shown.sync(&marks, None, &mut markers);
        let raised = fill_at(&markers, [1, 6, 3]);
        let kept_untouched = fill_at_id(&markers, sentinel);
        shown.sync(&marks, Some(5), &mut markers);
        let lowered = fill_at(&markers, [1, 6, 3]);
        let still_untouched = fill_at_id(&markers, sentinel);

        assert!(unchanged, "an unchanged ceiling wrote a marker again");
        assert!(raised.is_some(), "raising the ceiling shows the mark");
        assert!(kept_untouched, "raising rewrote a marker that stayed");
        assert!(lowered.is_none(), "lowering the ceiling hides it");
        assert!(still_untouched, "lowering rewrote a marker that stayed");
    }

    #[test]
    fn a_mark_cancelled_while_hidden_does_not_return_with_the_ceiling() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let mut marks = marks_at(&[[1, 6, 3]]);
        shown.sync(&marks, Some(5), &mut markers);
        marks.toggle([1, 6, 3]);
        shown.sync(&marks, Some(5), &mut markers);

        shown.sync(&marks, None, &mut markers);

        assert!(fills(&markers).is_empty());
    }

    /// Whether the sentinel-coloured fill is still on the scene.
    fn fill_at_id(markers: &Markers, sentinel: Marker) -> bool {
        fills(markers)
            .iter()
            .any(|fill| fill.color == sentinel.color)
    }

    #[test]
    fn sync_with_no_marks_leaves_the_scene_empty() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let mut marks = marks_at(&[[1, 2, 3]]);
        shown.sync(&marks, None, &mut markers);
        marks.block_removed([1, 2, 3]);

        shown.sync(&marks, None, &mut markers);

        assert!(fills(&markers).is_empty());
    }

    #[test]
    fn marker_ids_set_bit_63_and_keep_clear_of_the_console_and_the_select_marker() {
        for pos in [
            [0, 0, 0],
            [1, 2, 3],
            [-1, -1, -1],
            [524_287, 524_287, 524_287],
        ] {
            let MarkerId(id) = marker_id(pos);

            assert_eq!(id >> 63, 1, "{pos:?}");
            assert_eq!(id >> 60 & 0b111, 0, "{pos:?}: bits 60..62 are unused");
            assert_ne!(id, u64::MAX, "{pos:?}");
        }
    }

    proptest! {
        #[test]
        fn distinct_blocks_get_distinct_marker_ids(
            a in proptest::array::uniform3(-(1i32 << 19)..(1i32 << 19)),
            axis in 0usize..3,
            value in -(1i32 << 19)..(1i32 << 19),
        ) {
            let mut b = a;
            b[axis] = value;
            prop_assume!(a != b);

            prop_assert_ne!(marker_id(a), marker_id(b));
        }
    }
}
