//! A navmesh cut along the world grid lines into cells a game's rule can judge, and the
//! cost-shaped route query over them.

use std::ops::Range;

use glam::Vec3;

use super::{NavMesh, Polygon};

/// The smallest cell size, in metres; keeps the grid far above the on-line tolerance.
const MIN_CELL_SIZE: f32 = 0.05;

/// The most cells one build may cut (2^20), checked against an estimate before cutting.
const MAX_CELLS: u64 = 1 << 20;

/// Whether `estimate` cells are within the limit.
fn fits(_estimate: u64) -> bool {
    todo!("compare the estimate with MAX_CELLS")
}

/// A `NavMesh` cut into cells no wider than one grid square. Plain data, deterministic.
#[derive(Clone, Debug, PartialEq)]
pub struct CellMesh {
    coarse: NavMesh,
    cells: Vec<Polygon>,
    of_polygon: Vec<Range<usize>>,
}

impl CellMesh {
    /// Cuts `navmesh` along the grid lines x = k·`cell_size` and z = k·`cell_size`.
    ///
    /// # Errors
    /// [`CellMeshError`] when `cell_size` is not finite or is below the minimum cell size
    /// (0.05 m), or would cut more than 2^20 cells.
    pub fn build(_navmesh: &NavMesh, _cell_size: f32) -> Result<Self, CellMeshError> {
        todo!("tessellate the navmesh into cells")
    }

    /// Corners from `from` to `to` through cells `rule` passed, start and goal included;
    /// `None` when an end is off the mesh or in a not-walkable cell, or no walkable corridor
    /// joins them.
    pub fn path_by_cost(
        &self,
        _from: Vec3,
        _to: Vec3,
        _rule: impl FnMut(NavCell<'_>) -> CellCost,
    ) -> Option<Vec<Vec3>> {
        todo!("A* over cell portals shaped by the rule, then funnel")
    }
}

/// The cell the rule judges: a convex piece of one navmesh polygon inside one grid square.
#[derive(Clone, Copy, Debug)]
pub struct NavCell<'a> {
    corners: &'a [Vec3],
}

impl NavCell<'_> {
    /// The corners in world space, counter-clockwise on the ground plane.
    pub fn corners(&self) -> &[Vec3] {
        self.corners
    }

    /// The mean of the corners; inside the cell, which is convex.
    pub fn centre(&self) -> Vec3 {
        todo!("mean of the corners")
    }
}

/// What the rule says about one cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellCost {
    Walkable(CostMultiplier),
    NotWalkable,
}

/// At least 1 and finite: the straight-line heuristic stays admissible, so A* stays optimal.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct CostMultiplier(f32);

impl CostMultiplier {
    /// The plain cost of walking a cell.
    pub const ONE: Self = Self(1.0);

    /// # Errors
    /// [`CostMultiplierError`] when `multiplier` is below 1, NaN or infinite.
    pub fn new(_multiplier: f32) -> Result<Self, CostMultiplierError> {
        todo!("refuse a multiplier below 1 or not finite")
    }
}

/// A cost multiplier below 1, NaN or infinite.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
#[error("cost multiplier {multiplier} is not a finite number of at least 1")]
pub struct CostMultiplierError {
    pub multiplier: f32,
}

/// Why a navmesh could not be cut into cells.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CellMeshError {
    #[error(
        "cell size {size} m is not a finite size of at least {} m",
        MIN_CELL_SIZE
    )]
    CellSize { size: f32 },
    #[error(
        "cell size {size} m would cut about {estimate} cells, more than {}",
        MAX_CELLS
    )]
    TooManyCells { size: f32, estimate: u64 },
}

#[cfg(test)]
mod tests {
    use super::super::SnapLimits;
    use super::*;

    const SIZES: [f32; 3] = [1.0, 0.75, 0.3];

    fn quad(x0: f32, z0: f32, x1: f32, z1: f32, neighbours: [Option<usize>; 4]) -> Polygon {
        Polygon {
            vertices: vec![
                Vec3::new(x0, 0.0, z0),
                Vec3::new(x1, 0.0, z0),
                Vec3::new(x1, 0.0, z1),
                Vec3::new(x0, 0.0, z1),
            ],
            neighbours: neighbours.to_vec(),
        }
    }

    fn lone(points: &[[f32; 2]]) -> Polygon {
        Polygon {
            vertices: points.iter().map(|p| Vec3::new(p[0], 0.0, p[1])).collect(),
            neighbours: vec![None; points.len()],
        }
    }

    fn mesh(polygons: Vec<Polygon>) -> NavMesh {
        NavMesh {
            polygons,
            limits: SnapLimits {
                radius: 0.4,
                below: 2.0,
                above: 0.5,
            },
        }
    }

    /// A convex pentagon off the grid and a triangle beside it, unlinked.
    fn scattered() -> NavMesh {
        mesh(vec![
            lone(&[
                [-3.3, -2.2],
                [4.7, -1.9],
                [5.2, 2.6],
                [0.4, 3.1],
                [-3.6, 1.4],
            ]),
            lone(&[[10.0, 0.0], [13.3, 0.2], [11.1, 2.9]]),
        ])
    }

    /// Three squares in a row, 2 m each: the first two linked, the third touching the second
    /// but not linked to it.
    fn row_of_three() -> NavMesh {
        mesh(vec![
            quad(0.0, 0.0, 2.0, 2.0, [None, Some(1), None, None]),
            quad(2.0, 0.0, 4.0, 2.0, [None, None, None, Some(0)]),
            quad(4.0, 0.0, 6.0, 2.0, [None; 4]),
        ])
    }

    fn area(corners: &[Vec3]) -> f64 {
        let n = corners.len();
        (0..n)
            .map(|i| {
                let (a, b) = (corners[i], corners[(i + 1) % n]);
                f64::from(a.x) * f64::from(b.z) - f64::from(b.x) * f64::from(a.z)
            })
            .sum::<f64>()
            / 2.0
    }

    const X: fn(&Vec3) -> f32 = |p| p.x;
    const Z: fn(&Vec3) -> f32 = |p| p.z;

    fn extent(corners: &[Vec3], axis: fn(&Vec3) -> f32) -> (f32, f32) {
        let values: Vec<f32> = corners.iter().map(axis).collect();
        (
            values.iter().copied().fold(f32::INFINITY, f32::min),
            values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        )
    }

    fn cut(navmesh: &NavMesh, size: f32) -> CellMesh {
        CellMesh::build(navmesh, size).expect("a finite size above the minimum cuts")
    }

    fn owner(cells: &CellMesh, cell: usize) -> usize {
        cells
            .of_polygon
            .iter()
            .position(|range| range.contains(&cell))
            .expect("every cell is cut from a polygon")
    }

    #[test]
    fn a_multiplier_below_one_or_not_finite_is_refused() {
        for refused in [0.99, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(
                CostMultiplier::new(refused).is_err(),
                "{refused} was accepted"
            );
        }

        assert_eq!(CostMultiplier::new(1.0), Ok(CostMultiplier::ONE));
        assert_eq!(CostMultiplier::new(7.5), Ok(CostMultiplier(7.5)));
        assert_eq!(
            CostMultiplier::new(0.99),
            Err(CostMultiplierError { multiplier: 0.99 })
        );
    }

    #[test]
    fn a_cells_centre_is_the_mean_of_its_corners() {
        let cells = cut(&row_of_three(), 1.0);
        let unit_square = cells
            .cells
            .iter()
            .find(|cell| {
                let ((x0, x1), (z0, z1)) = (extent(&cell.vertices, X), extent(&cell.vertices, Z));
                (x0, x1, z0, z1) == (0.0, 1.0, 0.0, 1.0)
            })
            .expect("the first square is cut into a cell [0, 1] x [0, 1]");
        let corners = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(3.0, 3.0, 0.0),
            Vec3::new(0.0, 3.0, 3.0),
        ];

        let square = NavCell {
            corners: &unit_square.vertices,
        }
        .centre();
        let triangle = NavCell { corners: &corners }.centre();

        assert!(
            square.distance(Vec3::new(0.5, 0.0, 0.5)) <= 1e-5,
            "{square:?}"
        );
        assert!(
            triangle.distance(Vec3::new(1.0, 2.0, 1.0)) <= 1e-5,
            "{triangle:?}"
        );
    }

    #[test]
    fn an_estimate_over_the_cell_limit_does_not_fit() {
        assert!(fits(0));
        assert!(fits(MAX_CELLS));
        assert!(!fits(MAX_CELLS + 1));
    }

    #[test]
    fn every_cell_lies_in_one_grid_square() {
        let navmesh = scattered();

        for size in SIZES {
            let cells = cut(&navmesh, size);

            assert!(!cells.cells.is_empty());
            for cell in &cells.cells {
                assert!(cell.vertices.len() >= 3, "{cell:?}");
                for (axis, name) in [(X, "x"), (Z, "z")] {
                    let (low, high) = extent(&cell.vertices, axis);
                    let square = ((low + high) / 2.0 / size).floor();
                    assert!(
                        low >= square * size - 1e-4 && high <= (square + 1.0) * size + 1e-4,
                        "size {size}: {name} runs {low}..{high}, outside one square: {cell:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn cell_areas_sum_to_each_polygons_area() {
        let navmesh = scattered();

        for size in SIZES {
            let cells = cut(&navmesh, size);

            assert_eq!(cells.of_polygon.len(), navmesh.polygons.len());
            assert_eq!(cells.of_polygon[0].start, 0);
            for pair in cells.of_polygon.windows(2) {
                assert_eq!(pair[0].end, pair[1].start, "ranges leave a gap");
            }
            assert_eq!(cells.of_polygon[1].end, cells.cells.len());
            for (polygon, range) in navmesh.polygons.iter().zip(&cells.of_polygon) {
                let whole = area(&polygon.vertices);
                let parts: f64 = cells.cells[range.clone()]
                    .iter()
                    .map(|cell| area(&cell.vertices))
                    .sum();
                assert!(
                    (parts - whole).abs() <= whole * 1e-4,
                    "size {size}: cells cover {parts} m2 of {whole} m2"
                );
            }
        }
    }

    #[test]
    fn cell_links_follow_the_coarse_links() {
        let navmesh = row_of_three();

        for size in SIZES {
            let cells = cut(&navmesh, size);
            let count = cells.cells.len();
            let mut component: Vec<Option<usize>> = vec![None; count];
            for root in 0..count {
                if component[root].is_some() {
                    continue;
                }
                let mut stack = vec![root];
                component[root] = Some(root);
                while let Some(at) = stack.pop() {
                    for next in cells.cells[at].neighbours.iter().flatten() {
                        if component[*next].is_none() {
                            component[*next] = Some(root);
                            stack.push(*next);
                        }
                    }
                }
            }

            let of = |polygon: usize| -> Vec<Option<usize>> {
                cells.of_polygon[polygon]
                    .clone()
                    .map(|cell| component[cell])
                    .collect()
            };
            for polygon in 0..3 {
                let own = of(polygon);
                assert!(
                    !own.is_empty(),
                    "size {size}: polygon {polygon} has no cells"
                );
                assert!(
                    own.iter().all(|c| *c == own[0]),
                    "size {size}: polygon {polygon} is split in pieces"
                );
            }
            assert_eq!(of(0)[0], of(1)[0], "size {size}: linked polygons are apart");
            assert_ne!(of(1)[0], of(2)[0], "size {size}: unlinked polygons joined");
            for (index, cell) in cells.cells.iter().enumerate() {
                for next in cell.neighbours.iter().flatten() {
                    assert!(
                        cells.cells[*next].neighbours.contains(&Some(index)),
                        "size {size}: link {index} -> {next} is one way"
                    );
                    let (a, b) = (owner(&cells, index), owner(&cells, *next));
                    assert!(
                        a == b || navmesh.polygons[a].neighbours.contains(&Some(b)),
                        "size {size}: cells of polygons {a} and {b} are linked, the polygons are not"
                    );
                }
            }
        }
    }

    #[test]
    fn a_vertex_just_off_a_grid_line_cuts_no_sliver() {
        let apex = |x: f32| mesh(vec![lone(&[[0.0, 0.0], [4.0, 0.0], [x, 3.0]])]);

        let exact = cut(&apex(2.0), 1.0);
        let off = cut(&apex(2.0 + 5e-5), 1.0);

        assert!(!off.cells.is_empty());
        assert_eq!(off.cells.len(), exact.cells.len());
        for cell in &off.cells {
            for axis in [X, Z] {
                let (low, high) = extent(&cell.vertices, axis);
                assert!(high - low >= 0.01, "sliver cell {cell:?}");
            }
        }
    }

    #[test]
    fn a_polygon_bound_just_past_a_grid_line_cuts_no_strip_and_no_repeated_corner() {
        let strip = |x: f32| mesh(vec![lone(&[[0.0, 0.0], [x, 0.0], [x, 3.0], [0.0, 3.0]])]);

        let exact = cut(&strip(2.0), 1.0);
        let off = cut(&strip(2.0 + 5e-5), 1.0);

        assert_eq!(off.cells.len(), 6);
        assert_eq!(off.cells.len(), exact.cells.len());
        for cell in &off.cells {
            let n = cell.vertices.len();
            for i in 0..n {
                let gap = cell.vertices[i].distance(cell.vertices[(i + 1) % n]);
                assert!(
                    gap >= 1e-3,
                    "corners {i} and {} repeat: {cell:?}",
                    (i + 1) % n
                );
            }
            let (low, high) = extent(&cell.vertices, X);
            assert!(high - low >= 0.01, "strip cell {cell:?}");
        }
    }

    #[test]
    fn rebuilt_cell_meshes_are_identical() {
        for size in SIZES {
            let first = cut(&scattered(), size);
            let second = cut(&scattered(), size);

            assert!(!first.cells.is_empty());
            assert_eq!(first, second);
        }
    }
}
