//! Navmesh building and path queries over small hand-made worlds. Geometry is Y-up and
//! floor tops sit at y = 0.

use std::sync::LazyLock;

use glam::Vec3;
use moho_level::{AgentProfile, NavMesh};
use moho_render_api::WorldMesh;
use proptest::prelude::*;

const PROFILE: AgentProfile = AgentProfile {
    radius: 0.4,
    height: 2.0,
    max_climb: 0.5,
    max_slope: std::f32::consts::FRAC_PI_4,
};

/// The navmesh generator's voxel size: a third of the agent radius. It bounds how far a corner
/// can stray inside the radius clearance.
const CELL: f32 = PROFILE.radius / 3.0;

/// An axis-aligned box on the XZ plane.
#[derive(Clone, Copy)]
struct Rect {
    min: [f32; 2],
    max: [f32; 2],
}

const WALL: Rect = Rect {
    min: [-0.25, -6.0],
    max: [0.25, 6.0],
};

/// A closed axis-aligned box with outward-facing triangles.
fn box_mesh(min: Vec3, max: Vec3) -> WorldMesh {
    let positions = vec![
        [min.x, min.y, min.z],
        [max.x, min.y, min.z],
        [max.x, max.y, min.z],
        [min.x, max.y, min.z],
        [min.x, min.y, max.z],
        [max.x, min.y, max.z],
        [max.x, max.y, max.z],
        [min.x, max.y, max.z],
    ];
    let indices = vec![
        0, 3, 2, 0, 2, 1, // -z
        4, 5, 6, 4, 6, 7, // +z
        0, 4, 7, 0, 7, 3, // -x
        1, 2, 6, 1, 6, 5, // +x
        0, 1, 5, 0, 5, 4, // -y
        3, 7, 6, 3, 6, 2, // +y
    ];
    WorldMesh::new(
        positions,
        vec![[0.0, 1.0, 0.0]; 8],
        vec![1.0; 8],
        vec![[0.0; 3]; 8],
        vec![1.0; 8],
        vec![0; 8],
        indices,
    )
    .expect("a box has matching channels and whole triangles")
}

/// A 20 m square floor whose top is at y = 0.
fn floor() -> WorldMesh {
    box_mesh(Vec3::new(-10.0, -1.0, -10.0), Vec3::new(10.0, 0.0, 10.0))
}

/// A box standing on the floor over `rect`, `top` metres high.
fn block(rect: Rect, top: f32) -> WorldMesh {
    box_mesh(
        Vec3::new(rect.min[0], 0.0, rect.min[1]),
        Vec3::new(rect.max[0], top, rect.max[1]),
    )
}

fn build(meshes: &[WorldMesh]) -> NavMesh {
    NavMesh::build(meshes, PROFILE).expect("the fixture meshes build")
}

fn wall_world() -> Vec<WorldMesh> {
    vec![floor(), block(WALL, 3.0)]
}

static WALLED: LazyLock<NavMesh> = LazyLock::new(|| build(&wall_world()));

/// Whether the XZ projection of the segment `a`-`b` touches `rect`.
fn segment_touches(a: Vec3, b: Vec3, rect: Rect) -> bool {
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    let from = [a.x, a.z];
    let delta = [b.x - a.x, b.z - a.z];
    for axis in 0..2 {
        if delta[axis].abs() < f32::EPSILON {
            if from[axis] < rect.min[axis] || from[axis] > rect.max[axis] {
                return false;
            }
            continue;
        }
        let t0 = (rect.min[axis] - from[axis]) / delta[axis];
        let t1 = (rect.max[axis] - from[axis]) / delta[axis];
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
    }
    enter <= leave
}

/// Metres between `p` and `rect` on the XZ plane; 0 inside.
fn distance_to(p: Vec3, rect: Rect) -> f32 {
    let dx = (rect.min[0] - p.x).max(0.0).max(p.x - rect.max[0]);
    let dz = (rect.min[1] - p.z).max(0.0).max(p.z - rect.max[1]);
    dx.hypot(dz)
}

fn flat_gap(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

#[test]
fn a_path_around_a_wall_reaches_the_goal_and_no_segment_crosses_it() {
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let path = WALLED.path(a, b).expect("the wall can be walked around");

    let goal = *path.last().expect("a path has corners");
    assert!(flat_gap(goal, b) <= 0.05, "ends at {goal:?}, not {b:?}");
    assert!(
        path.len() >= 3,
        "a straight line would cross the wall: {path:?}"
    );
    for pair in path.windows(2) {
        assert!(
            !segment_touches(pair[0], pair[1], WALL),
            "{:?} -> {:?} crosses the wall",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn two_builds_of_one_level_give_identical_paths() {
    let a = Vec3::new(-5.0, 0.0, 1.0);
    let b = Vec3::new(5.0, 0.0, -2.0);
    let world = wall_world();

    let first = build(&world);
    let second = build(&world);

    let path = first.path(a, b).expect("the wall can be walked around");
    assert_eq!(Some(path), second.path(a, b));
    assert_eq!(first, second);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    #[test]
    fn path_corners_keep_the_agent_radius_from_walls(
        ax in -9.0_f32..-1.0, az in -9.0_f32..9.0,
        bx in 1.0_f32..9.0, bz in -9.0_f32..9.0,
    ) {
        let a = Vec3::new(ax, 0.0, az);
        let b = Vec3::new(bx, 0.0, bz);

        let path = WALLED.path(a, b);

        let path = path.ok_or_else(|| TestCaseError::fail(format!("no path {a:?} to {b:?}")))?;
        prop_assert_eq!(WALLED.path(a, b), Some(path.clone()));
        for corner in path {
            prop_assert!(
                distance_to(corner, WALL) >= PROFILE.radius - CELL,
                "corner {corner:?} is {} m from the wall",
                distance_to(corner, WALL)
            );
        }
    }
}

#[test]
fn a_point_inside_a_closed_room_has_no_path() {
    let room = |min: [f32; 2], max: [f32; 2]| block(Rect { min, max }, 3.0);
    let world = vec![
        floor(),
        room([3.5, -2.5], [4.0, 2.5]),
        room([8.0, -2.5], [8.5, 2.5]),
        room([3.5, -2.5], [8.5, -2.0]),
        room([3.5, 2.0], [8.5, 2.5]),
    ];
    let navmesh = build(&world);
    let outside = Vec3::new(-5.0, 0.0, 0.0);
    let inside = Vec3::new(6.0, 0.0, 0.0);
    let inside_too = Vec3::new(7.0, 0.0, 1.0);

    let to_room = navmesh.path(outside, inside);

    assert!(
        navmesh.path(outside, Vec3::new(0.0, 0.0, 5.0)).is_some(),
        "open floor outside the room is reachable"
    );
    assert!(
        navmesh.path(inside, inside_too).is_some(),
        "the room's own floor is on the mesh"
    );
    assert_eq!(to_room, None);
}

#[test]
fn a_point_above_the_floor_snaps_down_to_it() {
    let hovering = Vec3::new(-5.0, 1.5, 0.0);
    let goal = Vec3::new(5.0, 0.0, 0.0);

    let path = WALLED
        .path(hovering, goal)
        .expect("the point is within head height of the floor");

    let start = path[0];
    assert!(
        flat_gap(start, hovering) <= 0.05,
        "moved sideways to {start:?}"
    );
    assert!(start.y.abs() <= 0.3, "start is at height {}", start.y);
}

#[test]
fn a_goal_beside_a_raised_block_snaps_to_the_floor_not_the_block_top() {
    let crate_rect = Rect {
        min: [2.0, -1.0],
        max: [4.0, 1.0],
    };
    let navmesh = build(&[floor(), block(crate_rect, 0.8)]);
    let start = Vec3::new(-5.0, 0.0, 0.0);
    let beside = Vec3::new(4.3, 0.4, 0.0);

    let path = navmesh
        .path(start, beside)
        .expect("the floor beside the block is reachable");

    let goal = *path.last().expect("a path has corners");
    assert!(
        goal.y <= 0.3,
        "goal is on the block top at height {}",
        goal.y
    );
    assert!(
        flat_gap(goal, beside) <= PROFILE.radius + 0.1,
        "goal moved to {goal:?}"
    );
}

#[test]
fn a_point_off_the_mesh_beyond_the_radius_has_no_path() {
    let start = Vec3::new(-5.0, 0.0, 0.0);
    let goal = Vec3::new(5.0, 0.0, 0.0);
    let floor_edge = 10.0 - PROFILE.radius;
    let drop = PROFILE.max_climb + 0.2;

    let on_floor = WALLED.path(start, goal);
    let near_edge = WALLED.path(start, Vec3::new(floor_edge + 0.2, 0.0, 0.0));
    let past_edge = WALLED.path(start, Vec3::new(floor_edge + 0.9, 0.0, 0.0));
    let head_high = WALLED.path(start, Vec3::new(5.0, PROFILE.height - 0.1, 0.0));
    let too_high = WALLED.path(start, Vec3::new(5.0, PROFILE.height + 0.3, 0.0));
    let within_climb = WALLED.path(start, Vec3::new(5.0, -(PROFILE.max_climb - 0.2), 0.0));
    let beneath = WALLED.path(start, Vec3::new(5.0, -drop, 0.0));
    let from_off = WALLED.path(Vec3::new(0.0, 0.0, 30.0), goal);

    assert!(on_floor.is_some(), "the control point is reachable");
    assert!(near_edge.is_some(), "a point within the radius snaps on");
    assert_eq!(past_edge, None);
    assert!(head_high.is_some(), "a point under head height snaps down");
    assert_eq!(too_high, None);
    assert!(
        within_climb.is_some(),
        "a point within climb of the floor snaps up"
    );
    assert_eq!(beneath, None);
    assert_eq!(from_off, None);
}

#[test]
fn an_empty_mesh_list_builds_an_empty_navmesh_with_no_paths() {
    let none: [WorldMesh; 0] = [];
    let floored = build(&[floor()]);
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let empty = NavMesh::build(&none, PROFILE).expect("no meshes is not a failure");

    assert_eq!(empty, NavMesh::default());
    assert_eq!(empty.path(a, b), None);
    assert!(floored.path(a, b).is_some(), "a floor has paths");
    assert_ne!(floored, empty);
}

#[test]
fn a_path_within_one_polygon_is_start_and_goal() {
    let navmesh = build(&[floor()]);
    let a = Vec3::new(-3.0, 0.0, -2.0);
    let b = Vec3::new(4.0, 0.0, 3.0);

    let path = navmesh.path(a, b).expect("open floor joins any two points");

    assert_eq!(path.len(), 2, "{path:?}");
    assert!(flat_gap(path[0], a) <= 0.05, "{path:?}");
    assert!(flat_gap(path[1], b) <= 0.05, "{path:?}");
}

fn flat_length(path: &[Vec3]) -> f32 {
    path.windows(2).map(|pair| flat_gap(pair[0], pair[1])).sum()
}

#[test]
fn a_clear_line_beside_a_pillar_stays_two_corners_while_a_blocked_one_bends() {
    let pillar = Rect {
        min: [2.0, 3.0],
        max: [4.0, 5.0],
    };
    let navmesh = build(&[floor(), block(pillar, 3.0)]);
    let start = Vec3::new(-5.0, 0.0, 0.0);
    let goal = Vec3::new(5.0, 0.0, 0.0);
    let blocked_start = Vec3::new(-5.0, 0.0, 4.0);
    let blocked_goal = Vec3::new(8.0, 0.0, 4.0);

    let clear = navmesh
        .path(start, goal)
        .expect("open floor joins the points");
    let around = navmesh
        .path(blocked_start, blocked_goal)
        .expect("the pillar can be walked around");

    assert!(
        distance_to(Vec3::new(0.0, 0.0, 0.0), pillar) >= 1.0 + PROFILE.radius,
        "the fixture line must clear the pillar's eroded edge"
    );
    assert_eq!(clear.len(), 2, "spurious corners: {clear:?}");
    assert!(flat_gap(clear[0], start) <= 0.05, "{clear:?}");
    assert!(flat_gap(clear[1], goal) <= 0.05, "{clear:?}");
    assert!(
        around.len() > 2,
        "the pillar should split the navmesh and force a bend: {around:?}"
    );
}

#[test]
fn a_path_through_two_staggered_walls_bends_at_both_gaps_and_crosses_neither() {
    let west = Rect {
        min: [-3.25, -10.0],
        max: [-2.75, 6.0],
    };
    let east = Rect {
        min: [2.75, -6.0],
        max: [3.25, 10.0],
    };
    let navmesh = build(&[floor(), block(west, 3.0), block(east, 3.0)]);
    let start = Vec3::new(-6.0, 0.0, 0.0);
    let goal = Vec3::new(6.0, 0.0, 0.0);

    let path = navmesh.path(start, goal).expect("the S-route is open");

    let end = *path.last().expect("a path has corners");
    assert!(flat_gap(end, goal) <= 0.05, "ends at {end:?}");
    assert!(
        path.len() >= 4,
        "start, two bends and goal expected: {path:?}"
    );
    for pair in path.windows(2) {
        for wall in [west, east] {
            assert!(
                !segment_touches(pair[0], pair[1], wall),
                "{:?} -> {:?} crosses a wall",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn a_path_takes_the_near_gap_rather_than_the_far_one() {
    let pieces = [[-10.0, -8.0], [-5.0, 1.0], [4.0, 10.0]];
    let mut meshes = vec![floor()];
    for [z0, z1] in pieces {
        meshes.push(block(
            Rect {
                min: [-0.25, z0],
                max: [0.25, z1],
            },
            3.0,
        ));
    }
    let navmesh = build(&meshes);
    let a = Vec3::new(-5.0, 0.0, 2.5);
    let b = Vec3::new(5.0, 0.0, 2.5);
    let far_gap_edge = Vec3::new(0.0, 0.0, -5.0);

    let path = navmesh.path(a, b).expect("the near gap is open");

    let detour_bound = flat_gap(a, far_gap_edge) + flat_gap(far_gap_edge, b);
    assert!(
        flat_length(&path) < detour_bound,
        "path of {} m is no shorter than the {detour_bound} m far-gap route: {path:?}",
        flat_length(&path)
    );
}

#[test]
fn a_path_from_a_point_to_itself_is_that_point_snapped() {
    let navmesh = build(&[floor()]);
    let p = Vec3::new(2.0, 0.0, 3.0);

    let path = navmesh.path(p, p).expect("a floor point reaches itself");

    let first = *path.first().expect("a path has corners");
    let last = *path.last().expect("a path has corners");
    assert_eq!(first, last, "{path:?}");
    assert!(flat_gap(first, p) <= 0.05, "{path:?}");
    assert!(
        path.iter().all(|corner| corner.is_finite()),
        "non-finite corner in {path:?}"
    );
}
