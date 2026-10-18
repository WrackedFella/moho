//! Renderer-side store of box markers keyed by [`MarkerId`].

use glam::Vec3;
use moho_render_api::{Marker, MarkerGpu, MarkerId, MarkerStyle};
use std::collections::BTreeMap;

/// How far each side of a marker is pushed out, in metres, so a marker lying on
/// a block face never z-fights it.
pub const MARKER_INFLATE: f32 = 0.01;

/// Corners of the unit cube spanning `(0, 0, 0)` to `(1, 1, 1)`, scaled per marker by the shader.
pub(crate) const UNIT_CUBE_VERTICES: &[[f32; 3]] = &[
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
];

/// Triangle list over [`UNIT_CUBE_VERTICES`], counter-clockwise seen from outside.
pub(crate) const UNIT_CUBE_INDICES: &[u32] = &[
    0, 3, 2, 0, 2, 1, // -z
    4, 5, 6, 4, 6, 7, // +z
    0, 4, 7, 0, 7, 3, // -x
    1, 2, 6, 1, 6, 5, // +x
    0, 1, 5, 0, 5, 4, // -y
    3, 7, 6, 3, 6, 2, // +y
];

/// Line list over [`UNIT_CUBE_VERTICES`]: the twelve box edges.
pub(crate) const UNIT_CUBE_EDGES: &[u32] = &[
    0, 1, 1, 2, 2, 3, 3, 0, // -z
    4, 5, 5, 6, 6, 7, 7, 4, // +z
    0, 4, 1, 5, 2, 6, 3, 7, // along z
];

/// One frame's marker instances, each list sorted far to near.
#[derive(Debug, Default, Clone)]
pub struct MarkerDraws {
    pub fills: Vec<MarkerGpu>,
    pub outlines: Vec<MarkerGpu>,
}

/// The markers a game has placed, drawn each frame until removed.
///
/// Kept in id order so the draw order of equidistant markers is deterministic.
#[derive(Debug, Default)]
pub struct Markers {
    live: BTreeMap<MarkerId, Marker>,
}

impl Markers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace the marker under `id`.
    pub fn upsert(&mut self, id: MarkerId, marker: Marker) {
        self.live.insert(id, marker);
    }

    /// Remove the marker under `id`; an unknown id is a no-op.
    pub fn remove(&mut self, id: MarkerId) {
        self.live.remove(&id);
    }

    pub fn clear(&mut self) {
        self.live.clear();
    }

    /// The markers as GPU instances split by style, each box grown by
    /// [`MARKER_INFLATE`] per side and each list sorted far to near by box
    /// centre from `eye`.
    pub fn draws(&self, eye: Vec3) -> MarkerDraws {
        let mut fills: Vec<(f32, MarkerGpu)> = Vec::new();
        let mut outlines: Vec<(f32, MarkerGpu)> = Vec::new();
        for m in self.live.values() {
            let (min, max) = (Vec3::from(m.min), Vec3::from(m.max));
            let grow = Vec3::splat(MARKER_INFLATE);
            let gpu = MarkerGpu {
                min: (min - grow).extend(0.0).to_array(),
                max: (max + grow).extend(0.0).to_array(),
                color: m.color,
            };
            let distance = ((min + max) * 0.5).distance_squared(eye);
            match m.style {
                MarkerStyle::Fill => fills.push((distance, gpu)),
                MarkerStyle::Outline => outlines.push((distance, gpu)),
            }
        }
        let far_to_near = |mut keyed: Vec<(f32, MarkerGpu)>| {
            keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
            keyed.into_iter().map(|(_, gpu)| gpu).collect()
        };
        MarkerDraws {
            fills: far_to_near(fills),
            outlines: far_to_near(outlines),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use moho_render_api::MarkerStyle;
    use proptest::prelude::*;

    const TOLERANCE: f32 = 1e-5;

    fn marker(min: [f32; 3], max: [f32; 3], tag: f32) -> Marker {
        Marker::new(min, max, [tag, 0.5, 0.25, 0.5])
    }

    fn assert_near(actual: [f32; 4], expected: [f32; 3], what: &str) {
        for axis in 0..3 {
            assert!(
                (actual[axis] - expected[axis]).abs() <= TOLERANCE,
                "{what}: {actual:?} vs {expected:?}"
            );
        }
    }

    /// The `color[0]` of each draw, which the tests use to tell markers apart.
    fn tags(draws: &[MarkerGpu]) -> Vec<f32> {
        draws.iter().map(|d| d.color[0]).collect()
    }

    #[test]
    fn upserted_marker_draws_at_its_box_grown_by_the_inflation() {
        let mut markers = Markers::new();
        let color = [1.0, 0.75, 0.0, 0.4];
        markers.upsert(
            MarkerId(7),
            Marker::new([1.0, 2.0, 3.0], [2.0, 4.0, 6.0], color),
        );

        let draws = markers.draws(Vec3::ZERO).fills;

        assert_eq!(draws.len(), 1);
        assert_near(
            draws[0].min,
            [
                1.0 - MARKER_INFLATE,
                2.0 - MARKER_INFLATE,
                3.0 - MARKER_INFLATE,
            ],
            "min",
        );
        assert_near(
            draws[0].max,
            [
                2.0 + MARKER_INFLATE,
                4.0 + MARKER_INFLATE,
                6.0 + MARKER_INFLATE,
            ],
            "max",
        );
        assert_eq!(draws[0].color, color);
    }

    #[test]
    fn upsert_replaces_the_marker_under_the_same_id() {
        let mut markers = Markers::new();
        markers.upsert(MarkerId(1), marker([0.0; 3], [1.0; 3], 1.0));

        markers.upsert(MarkerId(1), marker([5.0; 3], [6.0; 3], 2.0));

        let draws = markers.draws(Vec3::ZERO).fills;
        assert_eq!(tags(&draws), vec![2.0]);
        assert_near(draws[0].min, [5.0 - MARKER_INFLATE; 3], "moved min");
    }

    #[test]
    fn draws_sort_far_to_near() {
        let mut markers = Markers::new();
        markers.upsert(MarkerId(1), marker([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], 1.0));
        markers.upsert(MarkerId(2), marker([5.0, 0.0, 0.0], [6.0, 1.0, 1.0], 5.0));
        markers.upsert(MarkerId(3), marker([3.0, 0.0, 0.0], [4.0, 1.0, 1.0], 3.0));

        let from_origin = tags(&markers.draws(Vec3::ZERO).fills);
        let from_far_x = tags(&markers.draws(Vec3::new(9.0, 0.0, 0.0)).fills);

        assert_eq!(from_origin, vec![5.0, 3.0, 1.0]);
        assert_eq!(from_far_x, vec![1.0, 3.0, 5.0]);
    }

    #[test]
    fn draws_sort_by_box_centre_not_by_corner() {
        let mut markers = Markers::new();
        // Centre 5 away from the eye, though its min corner is at the eye.
        markers.upsert(MarkerId(1), marker([0.0, 0.0, 0.0], [10.0, 1.0, 1.0], 1.0));
        // Centre 4.5 away, min corner 4 away.
        markers.upsert(MarkerId(2), marker([4.0, 0.0, 0.0], [5.0, 1.0, 1.0], 2.0));

        let order = tags(&markers.draws(Vec3::new(0.0, 0.5, 0.5)).fills);

        assert_eq!(order, vec![1.0, 2.0]);
    }

    #[test]
    fn clear_drops_every_marker() {
        let mut markers = Markers::new();
        markers.upsert(MarkerId(1), marker([0.0; 3], [1.0; 3], 1.0));
        markers.upsert(MarkerId(2), marker([2.0; 3], [3.0; 3], 2.0));

        markers.clear();

        let draws = markers.draws(Vec3::ZERO);
        assert!(draws.fills.is_empty());
        assert!(draws.outlines.is_empty());
    }

    #[test]
    fn unit_cube_faces_wind_outward() {
        let vertices: Vec<Vec3> = UNIT_CUBE_VERTICES.iter().copied().map(Vec3::from).collect();
        assert_eq!(UNIT_CUBE_INDICES.len(), 36, "12 triangles");
        let centre = vertices.iter().sum::<Vec3>() / vertices.len() as f32;
        assert!(
            (centre - Vec3::splat(0.5)).length() <= TOLERANCE,
            "unit cube is centred on (0.5, 0.5, 0.5): {centre}"
        );

        let (triangles, _) = UNIT_CUBE_INDICES.as_chunks::<3>();
        for tri in triangles {
            let [a, b, c] = tri.map(|i| vertices[usize::try_from(i).expect("index fits usize")]);
            let normal = (b - a).cross(c - a);
            let outward = (a + b + c) / 3.0 - centre;

            assert!(
                normal.dot(outward) > 0.0,
                "triangle {tri:?} faces inward: normal {normal}, outward {outward}"
            );
        }
    }

    #[test]
    fn style_picks_the_draw_list() {
        let rows = [(MarkerStyle::Fill, true), (MarkerStyle::Outline, false)];

        for (style, in_fills) in rows {
            let mut markers = Markers::new();
            markers.upsert(
                MarkerId(1),
                marker([1.0; 3], [2.0; 3], 4.0).with_style(style),
            );

            let draws = markers.draws(Vec3::ZERO);

            let (hit, miss) = if in_fills {
                (&draws.fills, &draws.outlines)
            } else {
                (&draws.outlines, &draws.fills)
            };
            assert_eq!(tags(hit), vec![4.0], "{style:?} lands in its own list");
            assert!(miss.is_empty(), "{style:?} stays out of the other list");
            assert_near(
                hit[0].min,
                [1.0 - MARKER_INFLATE; 3],
                "inflated min in either list",
            );
            assert_near(
                hit[0].max,
                [2.0 + MARKER_INFLATE; 3],
                "inflated max in either list",
            );
        }
    }

    #[test]
    fn each_list_sorts_far_to_near_on_its_own() {
        let mut markers = Markers::new();
        let at = |x: f32, tag: f32| marker([x, 0.0, 0.0], [x + 1.0, 1.0, 1.0], tag);
        markers.upsert(MarkerId(1), at(1.0, 1.0));
        markers.upsert(MarkerId(2), at(2.0, 2.0).with_style(MarkerStyle::Outline));
        markers.upsert(MarkerId(3), at(3.0, 3.0));
        markers.upsert(MarkerId(4), at(4.0, 4.0).with_style(MarkerStyle::Outline));

        let draws = markers.draws(Vec3::ZERO);

        assert_eq!(tags(&draws.fills), vec![3.0, 1.0]);
        assert_eq!(tags(&draws.outlines), vec![4.0, 2.0]);
    }

    #[test]
    fn replacing_with_another_style_moves_the_marker_between_lists() {
        let mut markers = Markers::new();
        markers.upsert(MarkerId(1), marker([0.0; 3], [1.0; 3], 1.0));

        markers.upsert(
            MarkerId(1),
            marker([0.0; 3], [1.0; 3], 1.0).with_style(MarkerStyle::Outline),
        );

        let as_outline = markers.draws(Vec3::ZERO);
        assert!(as_outline.fills.is_empty());
        assert_eq!(tags(&as_outline.outlines), vec![1.0]);

        markers.upsert(MarkerId(1), marker([0.0; 3], [1.0; 3], 1.0));

        let as_fill = markers.draws(Vec3::ZERO);
        assert_eq!(tags(&as_fill.fills), vec![1.0]);
        assert!(as_fill.outlines.is_empty());
    }

    #[test]
    fn unit_cube_edges_are_the_twelve_box_edges() {
        assert_eq!(UNIT_CUBE_EDGES.len(), 24, "12 line segments");

        let (pairs, rest) = UNIT_CUBE_EDGES.as_chunks::<2>();
        assert!(rest.is_empty());
        let mut seen = std::collections::BTreeSet::new();
        for &[a, b] in pairs {
            let va = UNIT_CUBE_VERTICES[usize::try_from(a).expect("index fits usize")];
            let vb = UNIT_CUBE_VERTICES[usize::try_from(b).expect("index fits usize")];
            let differing = (0..3).filter(|&axis| va[axis] != vb[axis]).count();

            assert_eq!(differing, 1, "edge {a}-{b} runs along exactly one axis");

            seen.insert((a.min(b), a.max(b)));
        }
        assert_eq!(seen.len(), 12, "every edge is distinct: {seen:?}");
    }

    #[derive(Clone, Debug)]
    enum Op {
        Upsert(u8, [i8; 3], [i8; 3]),
        Remove(u8),
        Clear,
    }

    fn ops() -> impl Strategy<Value = Vec<Op>> {
        let corner = || [-8i8..8, -8i8..8, -8i8..8];
        let op = prop_oneof![
            (0u8..4, corner(), corner()).prop_map(|(id, a, b)| Op::Upsert(id, a, b)),
            (0u8..4).prop_map(Op::Remove),
            Just(Op::Clear),
        ];
        prop::collection::vec(op, 0..40)
    }

    fn corner(c: [i8; 3]) -> [f32; 3] {
        c.map(f32::from)
    }

    proptest! {
        #[test]
        fn upsert_moves_and_remove_drops_a_marker(ops in ops()) {
            let mut markers = Markers::new();
            let mut model: BTreeMap<u8, Marker> = BTreeMap::new();

            for op in ops {
                match op {
                    Op::Upsert(id, a, b) => {
                        // The id doubles as the colour tag, so draws can be matched to the model.
                        let m = Marker::new(corner(a), corner(b), [f32::from(id), 0.5, 0.25, 0.5]);
                        markers.upsert(MarkerId(u64::from(id)), m);
                        model.insert(id, m);
                    }
                    Op::Remove(id) => {
                        markers.remove(MarkerId(u64::from(id)));
                        model.remove(&id);
                    }
                    Op::Clear => {
                        markers.clear();
                        model.clear();
                    }
                }
            }

            let draws = markers.draws(Vec3::new(1.0, 2.0, 3.0)).fills;

            let mut drawn: Vec<MarkerGpu> = draws;
            drawn.sort_by(|a, b| a.color[0].total_cmp(&b.color[0]));
            prop_assert_eq!(drawn.len(), model.len(), "one draw per live id");
            for (draw, (id, m)) in drawn.iter().zip(&model) {
                prop_assert_eq!(draw.color, m.color, "colour of id {}", id);
                for axis in 0..3 {
                    prop_assert!((draw.min[axis] - (m.min[axis] - MARKER_INFLATE)).abs() <= TOLERANCE);
                    prop_assert!((draw.max[axis] - (m.max[axis] + MARKER_INFLATE)).abs() <= TOLERANCE);
                }
            }
        }
    }
}
