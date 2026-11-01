//! The markers that show which blocks are marked for mining.

use flagstone_game::colony::MineMarks;
use moho_render_api::MarkerId;
use moho_renderer::Markers;
use std::collections::BTreeSet;

/// The blocks whose marker is on the scene, so a frame draws only what changed.
#[derive(Debug, Default)]
pub struct DesignationMarkers {
    shown: BTreeSet<[i32; 3]>,
}

impl DesignationMarkers {
    /// Adds a marker for each new mark and removes the marker of each cancelled one.
    /// Markers it did not place are left alone.
    pub fn sync(&mut self, _marks: &MineMarks, _markers: &mut Markers) {
        todo!("diff the marks against the shown markers")
    }

    pub fn shown(&self) -> &BTreeSet<[i32; 3]> {
        &self.shown
    }
}

/// The marker id of the block at `pos`: bit 63 set, then x, y and z in 20 bits each.
pub fn marker_id(_pos: [i32; 3]) -> MarkerId {
    MarkerId(0)
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

        shown.sync(&marks, &mut markers);
        let after_marking = fills(&markers).len();
        let first = fill_at(&markers, [1, 2, 3]);
        let second = fill_at(&markers, [4, 5, 6]);
        marks.toggle([1, 2, 3]);
        marks.toggle([7, 8, 9]);
        shown.sync(&marks, &mut markers);

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
        assert_eq!(
            shown.shown().iter().copied().collect::<Vec<_>>(),
            [[4, 5, 6], [7, 8, 9]]
        );
    }

    #[test]
    fn sync_with_no_change_makes_no_renderer_calls() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let marks = marks_at(&[[1, 2, 3]]);
        shown.sync(&marks, &mut markers);
        let sentinel = Marker::new([0.0; 3], [1.0; 3], [0.0, 1.0, 0.0, 0.5]);
        markers.upsert(marker_id([1, 2, 3]), sentinel);

        shown.sync(&marks, &mut markers);

        assert_eq!(
            fills(&markers)[0].color,
            sentinel.color,
            "an unchanged mark's marker was written again"
        );
    }

    #[test]
    fn sync_with_no_marks_leaves_the_scene_empty() {
        let mut markers = Markers::new();
        let mut shown = DesignationMarkers::default();
        let mut marks = marks_at(&[[1, 2, 3]]);
        shown.sync(&marks, &mut markers);
        marks.block_removed([1, 2, 3]);

        shown.sync(&marks, &mut markers);

        assert!(fills(&markers).is_empty());
        assert!(shown.shown().is_empty());
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
            b in proptest::array::uniform3(-(1i32 << 19)..(1i32 << 19)),
        ) {
            prop_assume!(a != b);

            prop_assert_ne!(marker_id(a), marker_id(b));
        }
    }
}
