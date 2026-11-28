//! A navmesh cut along the world grid lines into cells a game's rule can judge, and the
//! cost-shaped route query over them.

use std::collections::HashMap;
use std::ops::Range;

use glam::Vec3;

use super::{NavMesh, Polygon, query};

/// The smallest cell size, in metres; keeps the grid far above the on-line tolerance.
const MIN_CELL_SIZE: f32 = 0.05;

/// The most cells one build may cut (2^20), checked against an estimate before cutting.
const MAX_CELLS: u64 = 1 << 20;

/// How far from a grid line a vertex may sit and still count as on it, so no sliver is cut.
const ON_LINE: f32 = 1e-4;

/// Index of the x and z coordinates in a `Vec3`.
const X: usize = 0;
const Z: usize = 2;

/// Whether `estimate` cells are within the limit.
fn fits(estimate: u64) -> bool {
    estimate <= MAX_CELLS
}

/// The lowest and highest coordinate of `corners` on `axis`; (∞, -∞) when there are none.
fn extent(corners: &[Vec3], axis: usize) -> (f32, f32) {
    let values = corners.iter().map(|v| v[axis]);
    (
        values.clone().fold(f32::INFINITY, f32::min),
        values.fold(f32::NEG_INFINITY, f32::max),
    )
}

/// An upper bound on the cells cut from `polygon`: one per grid square its ground bounds touch.
fn estimate(polygon: &Polygon, size: f32) -> u64 {
    let span = |axis: usize| {
        let (low, high) = extent(&polygon.vertices, axis);
        if high >= low {
            ((high - low) / size).ceil() as u64 + 1
        } else {
            0
        }
    };
    span(X).saturating_mul(span(Z))
}

/// The point where the edge `a`-`b` crosses `line` on `axis`. The ends are ordered by the
/// axis first, so both polygons sharing the edge compute the same bits.
fn cut_point(a: Vec3, b: Vec3, axis: usize, line: f32) -> Vec3 {
    let (low, high) = if a[axis] < b[axis] { (a, b) } else { (b, a) };
    let t = (line - low[axis]) / (high[axis] - low[axis]);
    let mut point = low + (high - low) * t;
    point[axis] = line;
    point
}

/// Splits a convex `polygon` at `line` on `axis` into the part below and the part above it.
/// `None` when it has no corner clearly on each side (corners within `ON_LINE` count as on it).
fn split(polygon: &[Vec3], axis: usize, line: f32) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
    let sides: Vec<i8> = polygon
        .iter()
        .map(|p| {
            let gap = p[axis] - line;
            if gap < -ON_LINE {
                -1
            } else {
                i8::from(gap > ON_LINE)
            }
        })
        .collect();
    if !sides.contains(&-1) || !sides.contains(&1) {
        return None;
    }
    let n = polygon.len();
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        let (side_a, side_b) = (sides[i], sides[(i + 1) % n]);
        if side_a <= 0 {
            below.push(a);
        }
        if side_a >= 0 {
            above.push(a);
        }
        if side_a * side_b < 0 {
            let crossing = cut_point(a, b, axis, line);
            below.push(crossing);
            above.push(crossing);
        }
    }
    Some((below, above))
}

/// Cuts `piece` at every grid line on `axis` strictly inside its bounds, lowest line first.
fn sweep(piece: Vec<Vec3>, axis: usize, size: f32) -> Vec<Vec<Vec3>> {
    let (low, high) = extent(&piece, axis);
    let mut pieces = Vec::new();
    let mut rest = piece;
    let mut k = (low / size).floor() as i64;
    loop {
        let line = k as f32 * size;
        if line >= high {
            break;
        }
        if line > low
            && let Some((below, above)) = split(&rest, axis, line)
        {
            pieces.push(below);
            rest = above;
        }
        k += 1;
    }
    pieces.push(rest);
    pieces
}

/// Twice the signed ground area of `corners`.
fn doubled_area(corners: &[Vec3]) -> f64 {
    let n = corners.len();
    (0..n)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % n]);
            f64::from(a.x) * f64::from(b.z) - f64::from(b.x) * f64::from(a.z)
        })
        .sum()
}

/// `corners` without consecutive repeats, or `None` when under three corners or no positive
/// ground area are left.
fn tidy(mut corners: Vec<Vec3>) -> Option<Vec<Vec3>> {
    corners.dedup_by(|a, b| a.x == b.x && a.z == b.z);
    while corners.len() > 1
        && corners
            .first()
            .zip(corners.last())
            .is_some_and(|(a, b)| a.x == b.x && a.z == b.z)
    {
        corners.pop();
    }
    (corners.len() >= 3 && doubled_area(&corners) > 0.0).then_some(corners)
}

/// The cells of one navmesh polygon: cut at every x line, then every z line.
fn tessellate(polygon: &Polygon, size: f32) -> Vec<Vec<Vec3>> {
    let columns = sweep(polygon.vertices.clone(), X, size);
    columns
        .into_iter()
        .flat_map(|column| sweep(column, Z, size))
        .filter_map(tidy)
        .collect()
}

type EdgeKey = [u32; 6];

fn edge_key(a: Vec3, b: Vec3) -> EdgeKey {
    [
        a.x.to_bits(),
        a.y.to_bits(),
        a.z.to_bits(),
        b.x.to_bits(),
        b.y.to_bits(),
        b.z.to_bits(),
    ]
}

/// Links cell edges that match bit for bit when their coarse polygons are the same or linked.
fn link(cells: &mut [Polygon], of_polygon: &[Range<usize>], coarse: &[Polygon]) {
    let mut owner = vec![0; cells.len()];
    for (polygon, range) in of_polygon.iter().enumerate() {
        owner[range.clone()].fill(polygon);
    }
    // Only looked up, never iterated: the order of its entries must not reach the output.
    let edge_count = cells.iter().map(|cell| cell.vertices.len()).sum();
    let mut edges: HashMap<EdgeKey, usize> = HashMap::with_capacity(edge_count);
    for (index, cell) in cells.iter().enumerate() {
        for (a, b) in cell.corners() {
            edges.entry(edge_key(a, b)).or_insert(index);
        }
    }
    let joined = |a: usize, b: usize| a == b || coarse[a].neighbours.contains(&Some(b));
    let links: Vec<Vec<Option<usize>>> = cells
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            cell.corners()
                .map(|(a, b)| {
                    edges
                        .get(&edge_key(b, a))
                        .copied()
                        .filter(|other| *other != index && joined(owner[index], owner[*other]))
                })
                .collect()
        })
        .collect();
    for (cell, neighbours) in cells.iter_mut().zip(links) {
        cell.neighbours = neighbours;
    }
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
    pub fn build(navmesh: &NavMesh, cell_size: f32) -> Result<Self, CellMeshError> {
        if !cell_size.is_finite() || cell_size < MIN_CELL_SIZE {
            return Err(CellMeshError::CellSize { size: cell_size });
        }
        let estimate = navmesh
            .polygons
            .iter()
            .map(|polygon| estimate(polygon, cell_size))
            .fold(0, u64::saturating_add);
        if !fits(estimate) {
            return Err(CellMeshError::TooManyCells {
                size: cell_size,
                estimate,
            });
        }
        let mut cells = Vec::new();
        let mut of_polygon = Vec::with_capacity(navmesh.polygons.len());
        for polygon in &navmesh.polygons {
            let first = cells.len();
            for corners in tessellate(polygon, cell_size) {
                cells.push(Polygon {
                    neighbours: vec![None; corners.len()],
                    vertices: corners,
                });
            }
            of_polygon.push(first..cells.len());
        }
        link(&mut cells, &of_polygon, &navmesh.polygons);
        tracing::debug!(cells = cells.len(), cell_size, "cut navmesh into cells");
        Ok(Self {
            coarse: navmesh.clone(),
            cells,
            of_polygon,
        })
    }

    /// Corners from `from` to `to` through cells `rule` passed, start and goal included;
    /// `None` when an end is not finite, off the mesh or in a not-walkable cell, or no walkable corridor
    /// joins them. The route is never partial.
    ///
    /// Both ends snap onto the navmesh as [`NavMesh::path`] does, so the route starts and ends
    /// where that route would. The route is the shortest way through the passed cells where a
    /// [`CellCost::Walkable`] multiplier scales the distance walked in that cell. Over equal
    /// cost it can be a few percent longer than [`NavMesh::path`] (about 3 to 5 percent on
    /// 1 m and 0.5 m cells), because the search runs over cell portal midpoints.
    ///
    /// `rule` is asked about the start's cell, then the goal's, and `NotWalkable` for either
    /// ends the query with nothing else asked. After that it is asked once, the first time the
    /// search reaches each further cell, so it is never asked twice about one cell and never
    /// asked about a cell the search does not reach (at most every cell of the start's
    /// connected area). The same question gives the same routes and the same questions in the
    /// same order.
    ///
    /// What the game owns:
    /// - **Partial visibility.** The cell is the unit. The route lies in cells the rule
    ///   passed; the rule decides whether a cell is clear, from its corners and centre or from
    ///   more samples of its own (a cell is convex and no wider than the grid). A seen strip
    ///   narrower than the rule's sample spacing can pass.
    /// - **Padding.** The route is the agent's centre and can run along the edge of a
    ///   ruled-out cell with zero margin: ruled-out area can be touched, never entered. A game
    ///   whose body must stay clear pads its rule, for example judging a point seen when any
    ///   point within its radius is.
    /// - **Snapshot.** The route reflects the rule when asked; threats that move are handled
    ///   by asking again.
    /// - **Budget.** Rule calls are bounded by the cells reached, and the rule's own cost (for
    ///   example ray casts) dominates the query, so ask once per decision, not every tick.
    pub fn path_by_cost(
        &self,
        from: Vec3,
        to: Vec3,
        mut rule: impl FnMut(NavCell<'_>) -> CellCost,
    ) -> Option<Vec<Vec3>> {
        let mut verdicts: Vec<Option<CellCost>> = vec![None; self.cells.len()];
        query::path_by_cost(
            &self.coarse,
            &self.cells,
            &self.of_polygon,
            from,
            to,
            |cell| {
                let verdict = *verdicts[cell].get_or_insert_with(|| {
                    rule(NavCell {
                        corners: &self.cells[cell].vertices,
                    })
                });
                match verdict {
                    CellCost::Walkable(multiplier) => Some(multiplier.0),
                    CellCost::NotWalkable => None,
                }
            },
        )
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
        self.corners.iter().sum::<Vec3>() / self.corners.len() as f32
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
    pub fn new(multiplier: f32) -> Result<Self, CostMultiplierError> {
        if multiplier.is_finite() && multiplier >= 1.0 {
            Ok(Self(multiplier))
        } else {
            Err(CostMultiplierError { multiplier })
        }
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
