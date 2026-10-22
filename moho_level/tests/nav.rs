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

    #[test]
    fn a_path_between_two_points_never_repeats_a_corner(
        ax in -9.0_f32..9.0, az in -9.0_f32..9.0,
        bx in -9.0_f32..9.0, bz in -9.0_f32..9.0,
    ) {
        let a = Vec3::new(ax, 0.0, az);
        let b = Vec3::new(bx, 0.0, bz);
        prop_assume!(flat_gap(a, b) > 0.1);

        let Some(path) = WALLED.path(a, b) else {
            return Ok(());
        };

        for pair in path.windows(2) {
            prop_assert!(flat_gap(pair[0], pair[1]) > 0.0, "repeated corner in {path:?}");
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

#[test]
fn an_agent_too_small_for_the_level_fails_the_build_instead_of_allocating_the_field() {
    // Cells are radius / 3, so a 3 cm agent needs 2100 x 2100 cells over 21 m: just past the
    // limit, so a build that wrongly went ahead would still finish quickly.
    let big_floor = box_mesh(Vec3::new(-10.5, -1.0, -10.5), Vec3::new(10.5, 0.0, 10.5));
    let tiny = AgentProfile {
        radius: 0.03,
        ..PROFILE
    };

    assert!(NavMesh::build([&big_floor], tiny).is_err());
    assert!(NavMesh::build([&big_floor], PROFILE).is_ok());
}

#[test]
fn an_agent_profile_that_cannot_describe_a_walker_fails_the_build() {
    let world = [floor()];
    let build_with = |change: fn(&mut AgentProfile)| {
        let mut profile = PROFILE;
        change(&mut profile);
        NavMesh::build(&world, profile)
    };

    let nan_radius = build_with(|p| p.radius = f32::NAN);
    let zero_radius = build_with(|p| p.radius = 0.0);
    let negative_height = build_with(|p| p.height = -1.0);
    let negative_climb = build_with(|p| p.max_climb = -0.1);
    let upright_slope = build_with(|p| p.max_slope = std::f32::consts::FRAC_PI_2);
    let nan_slope = build_with(|p| p.max_slope = f32::NAN);
    let no_climb = build_with(|p| p.max_climb = 0.0);

    assert!(nan_radius.is_err(), "NaN radius built");
    assert!(zero_radius.is_err(), "zero radius built");
    assert!(negative_height.is_err(), "negative height built");
    assert!(negative_climb.is_err(), "negative climb built");
    assert!(upright_slope.is_err(), "a slope of pi/2 built");
    assert!(nan_slope.is_err(), "NaN slope built");
    let flat = no_climb.expect("an agent that cannot climb at all is a valid profile");
    // The generated surface sits up to a voxel above the mesh, so with no climb allowed a
    // query has to start above it to snap.
    assert!(
        flat.path(Vec3::new(-5.0, 0.3, 0.0), Vec3::new(5.0, 0.3, 0.0))
            .is_some(),
        "a flat floor is still walkable without climbing"
    );
}

/// A thin walkable slab over the middle of the floor, with agent-height clearance beneath.
fn slab() -> WorldMesh {
    box_mesh(Vec3::new(-4.0, 2.15, -4.0), Vec3::new(4.0, 2.35, 4.0))
}

/// A point inside both the floor and the slab on the ground plane: the slab's top is 0.4 m
/// above it, the floor 1.95 m below.
const BETWEEN: Vec3 = Vec3::new(0.0, 1.95, 0.0);

#[test]
fn a_point_between_two_walkable_levels_snaps_to_the_nearer_one_whatever_the_mesh_order() {
    let floor_first = build(&[floor(), slab()]);
    let slab_first = build(&[slab(), floor()]);
    let far = Vec3::new(3.0, 2.35, 3.0);

    let from_floor_first = floor_first
        .path(BETWEEN, far)
        .expect("the slab is walkable");
    let from_slab_first = slab_first.path(BETWEEN, far).expect("the slab is walkable");

    for path in [&from_floor_first, &from_slab_first] {
        let start = path[0];
        assert!(
            (start.y - 2.35).abs() <= 0.3,
            "snapped to height {} instead of the slab top at 2.35: {path:?}",
            start.y
        );
    }
}

/// A surface of triangles with no volume, normals up where the winding faces up.
fn surface(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> WorldMesh {
    let count = positions.len();
    WorldMesh::new(
        positions,
        vec![[0.0, 1.0, 0.0]; count],
        vec![1.0; count],
        vec![[0.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .expect("a surface has matching channels and whole triangles")
}

/// A ramp rising along +x from y = 0 at x = -4 to `rise` at x = 4, 6 m wide in z.
fn ramp(rise: f32) -> WorldMesh {
    surface(
        vec![
            [-4.0, 0.0, 3.0],
            [4.0, rise, 3.0],
            [4.0, rise, -3.0],
            [-4.0, 0.0, -3.0],
        ],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// Two ramps meeting in a ridge at x = 0, 2 m high, so that one navmesh polygon spans a crease.
fn tent() -> WorldMesh {
    surface(
        vec![
            [-4.0, 0.0, 3.0],
            [0.0, 2.0, 3.0],
            [4.0, 0.0, 3.0],
            [-4.0, 0.0, -3.0],
            [0.0, 2.0, -3.0],
            [4.0, 0.0, -3.0],
        ],
        vec![0, 1, 4, 0, 4, 3, 1, 2, 5, 1, 5, 4],
    )
}

/// Where a point asked for at `from` ends up on the mesh.
fn snapped(navmesh: &NavMesh, from: Vec3) -> Option<Vec3> {
    navmesh.path(from, from).map(|path| path[0])
}

/// The tent's height at `x`.
fn tent_height(x: f32) -> f32 {
    2.0 - x.abs() * 0.5
}

#[test]
fn points_on_a_creased_surface_snap_to_its_height_on_both_slopes() {
    let navmesh = NavMesh::build(&[tent()], PROFILE).expect("the tent builds");
    let mut errors = Vec::new();

    for x in [-3.0_f32, -2.0, -1.0, -0.3, 0.3, 1.0, 2.0, 3.0] {
        for z in [-2.0_f32, 0.0, 2.0] {
            let wanted = tent_height(x);
            let Some(at) = snapped(&navmesh, Vec3::new(x, wanted, z)) else {
                errors.push(format!("({x}, {z}) did not snap"));
                continue;
            };
            if flat_gap(at, Vec3::new(x, wanted, z)) > 0.05 || (at.y - wanted).abs() > 0.25 {
                errors.push(format!("({x}, {z}) wanted height {wanted}, got {at:?}"));
            }
        }
    }

    assert_eq!(errors, Vec::<String>::new());
}

/// The ramp's height at `x`.
fn ramp_height(rise: f32, x: f32) -> f32 {
    (x + 4.0) / 8.0 * rise
}

#[test]
fn a_path_up_a_ramp_follows_its_surface() {
    let rise = 4.0;
    let navmesh = NavMesh::build(&[ramp(rise)], PROFILE).expect("the ramp builds");
    let foot = Vec3::new(-3.0, ramp_height(rise, -3.0), 0.5);
    let top = Vec3::new(3.0, ramp_height(rise, 3.0), -1.5);

    let path = navmesh
        .path(foot, top)
        .expect("a 27 degree ramp is walkable");

    let start = path[0];
    let end = *path.last().expect("a path has corners");
    assert!((start.y - foot.y).abs() <= 0.25, "starts at {start:?}");
    assert!((end.y - top.y).abs() <= 0.25, "ends at {end:?}");
    assert!(flat_gap(end, top) <= 0.05, "ends at {end:?}");
}

#[test]
fn a_ramp_steeper_than_the_profile_slope_is_not_walkable_and_a_gentler_one_is() {
    let rise = 8.0 * 40.0_f32.to_radians().tan();
    let foot = Vec3::new(-3.0, ramp_height(rise, -3.0), 0.0);
    let top = Vec3::new(3.0, ramp_height(rise, 3.0), 0.0);
    let thirty = AgentProfile {
        max_slope: 30.0_f32.to_radians(),
        ..PROFILE
    };

    let forty_five = NavMesh::build(&[ramp(rise)], PROFILE).expect("the ramp builds");
    let strict = NavMesh::build(&[ramp(rise)], thirty).expect("an unwalkable level still builds");
    let gentle = NavMesh::build(&[ramp(4.0)], thirty).expect("the ramp builds");

    let path = forty_five
        .path(foot, top)
        .expect("a 40 degree ramp is inside a 45 degree profile");
    assert!(
        (path[0].y - foot.y).abs() <= 0.35 && (path[path.len() - 1].y - top.y).abs() <= 0.35,
        "{path:?}"
    );
    assert_eq!(strict.path(foot, top), None);
    assert!(
        gentle
            .path(
                Vec3::new(-3.0, ramp_height(4.0, -3.0), 0.0),
                Vec3::new(3.0, ramp_height(4.0, 3.0), 0.0)
            )
            .is_some(),
        "a 27 degree ramp is inside a 30 degree profile"
    );
}

/// A barrier across the floor at x = 0, 0.5 m thick, with an opening for each `(z0, z1)` gap.
fn barrier_with_gaps(gaps: &[(f32, f32)]) -> Vec<WorldMesh> {
    barrier_at(0.0, gaps)
}

/// The same barrier centred on `x`.
fn barrier_at(x: f32, gaps: &[(f32, f32)]) -> Vec<WorldMesh> {
    let mut meshes = vec![floor()];
    let mut from = -10.0;
    for &(z0, z1) in gaps.iter().chain(&[(10.0, 10.0)]) {
        if z0 > from {
            meshes.push(block(
                Rect {
                    min: [x - 0.25, from],
                    max: [x + 0.25, z0],
                },
                3.0,
            ));
        }
        from = z1;
    }
    meshes
}

#[test]
fn a_gap_narrower_than_the_agent_is_refused_and_a_wider_one_is_walked() {
    let wide_agent = AgentProfile {
        radius: 0.6,
        ..PROFILE
    };
    let narrow = NavMesh::build(&barrier_with_gaps(&[(-0.45, 0.45)]), wide_agent)
        .expect("the barrier builds");
    let wide =
        NavMesh::build(&barrier_with_gaps(&[(-0.9, 0.9)]), wide_agent).expect("the barrier builds");
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let through_narrow = narrow.path(a, b);
    let through_wide = wide.path(a, b);

    assert_eq!(through_narrow, None);
    assert!(through_wide.is_some(), "a 1.8 m gap fits a 1.2 m agent");
}

#[test]
fn paths_keep_a_wide_agents_radius_from_walls() {
    let wide_agent = AgentProfile {
        radius: 0.6,
        ..PROFILE
    };
    let navmesh = NavMesh::build(&wall_world(), wide_agent).expect("the wall builds");
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let path = navmesh.path(a, b).expect("the wall can be walked around");

    for corner in &path {
        assert!(
            distance_to(*corner, WALL) >= wide_agent.radius - wide_agent.radius / 3.0,
            "corner {corner:?} is {} m from the wall",
            distance_to(*corner, WALL)
        );
    }
}

/// A beam across the whole floor with its underside `clearance` metres above it.
fn lintel(clearance: f32) -> WorldMesh {
    box_mesh(
        Vec3::new(-1.0, clearance, -10.0),
        Vec3::new(1.0, clearance + 1.0, 10.0),
    )
}

#[test]
fn a_ceiling_lower_than_the_agent_blocks_the_way_and_a_higher_one_does_not() {
    let tall = AgentProfile {
        height: 2.4,
        ..PROFILE
    };
    let short = AgentProfile {
        height: 1.6,
        ..PROFILE
    };
    let world = [floor(), lintel(2.0)];
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let tall_agent = NavMesh::build(&world, tall).expect("the lintel builds");
    let short_agent = NavMesh::build(&world, short).expect("the lintel builds");

    assert_eq!(tall_agent.path(a, b), None);
    assert!(
        short_agent.path(a, b).is_some(),
        "a 1.6 m agent walks under a 2.0 m beam"
    );
}

/// A step across the whole floor, `height` metres high.
fn step(height: f32) -> WorldMesh {
    block(
        Rect {
            min: [-1.0, -10.0],
            max: [1.0, 10.0],
        },
        height,
    )
}

#[test]
fn a_step_above_the_profile_climb_blocks_the_way_and_one_within_it_does_not() {
    let nimble = AgentProfile {
        max_climb: 1.2,
        ..PROFILE
    };
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);

    let too_high = NavMesh::build(&[floor(), step(0.7)], PROFILE).expect("the step builds");
    let within_climb = NavMesh::build(&[floor(), step(0.3)], PROFILE).expect("the step builds");
    let high_climber = NavMesh::build(&[floor(), step(1.0)], nimble).expect("the step builds");

    assert_eq!(too_high.path(a, b), None);
    assert!(within_climb.path(a, b).is_some(), "0.3 m is within 0.5 m");
    assert!(high_climber.path(a, b).is_some(), "1.0 m is within 1.2 m");
}

#[test]
fn a_goal_off_the_floor_edge_snaps_to_the_nearest_point_of_the_walkable_edge() {
    let navmesh = build(&[floor()]);
    let start = Vec3::new(0.0, 0.0, 0.0);
    let edge = 10.0 - PROFILE.radius;
    let beyond = [
        (Vec3::new(edge + 0.2, 0.0, 2.3), [edge, 2.3]),
        (Vec3::new(-edge - 0.2, 0.0, -3.1), [-edge, -3.1]),
        (Vec3::new(1.7, 0.0, edge + 0.2), [1.7, edge]),
        (Vec3::new(-4.2, 0.0, -edge - 0.2), [-4.2, -edge]),
    ];

    for (goal, [x, z]) in beyond {
        let path = navmesh.path(start, goal).expect("within the snap radius");
        let end = *path.last().expect("a path has corners");
        let lateral = if x.abs() == edge { z } else { x };
        let lateral_end = if x.abs() == edge { end.z } else { end.x };
        let (edge_axis, edge_end) = if x.abs() == edge {
            (x, end.x)
        } else {
            (z, end.z)
        };
        assert!(
            (lateral_end - lateral).abs() <= 0.02 && (edge_end - edge_axis).abs() <= 2.0 * CELL,
            "{goal:?} snapped to {end:?}, wanted about ({x}, {z})"
        );
    }
}

#[test]
fn a_start_off_the_floor_edge_snaps_to_the_nearest_point_of_the_walkable_edge() {
    let navmesh = build(&[floor()]);
    let goal = Vec3::new(0.0, 0.0, 0.0);
    let edge = 10.0 - PROFILE.radius;

    let path = navmesh
        .path(Vec3::new(edge + 0.25, 0.0, -6.4), goal)
        .expect("within the snap radius");

    assert!(
        (path[0].x - edge).abs() <= 2.0 * CELL && (path[0].z + 6.4).abs() <= 0.02,
        "snapped to {:?}",
        path[0]
    );
}

#[test]
fn a_point_high_up_a_ramp_still_snaps_to_it() {
    let rise = 6.0;
    let navmesh = NavMesh::build(&[ramp(rise)], PROFILE).expect("the ramp builds");
    let high = Vec3::new(3.5, ramp_height(rise, 3.5) + 0.2, 0.0);

    let at = snapped(&navmesh, high).expect("the point is just above the ramp");

    assert!((at.y - ramp_height(rise, 3.5)).abs() <= 0.35, "{at:?}");
}

/// The shortest walk from `a` to `b` through the opening `(z0, z1)` of a barrier centred on `x`:
/// the agent's centre stays `PROFILE.radius` inside the opening and clear of the barrier's
/// 0.5 m thickness.
fn shortest_through_gap(a: Vec3, b: Vec3, x: f32, (z0, z1): (f32, f32)) -> f32 {
    let (lo, hi) = (z0 + PROFILE.radius, z1 - PROFILE.radius);
    let half = 0.25 + PROFILE.radius;
    let steps = 24;
    let at = |i: i32| lo + (hi - lo) * i as f32 / steps as f32;
    let mut best = f32::INFINITY;
    for i in 0..=steps {
        for j in 0..=steps {
            let entry = Vec3::new(x - half, 0.0, at(i));
            let exit = Vec3::new(x + half, 0.0, at(j));
            best = best.min(flat_gap(a, entry) + flat_gap(entry, exit) + flat_gap(exit, b));
        }
    }
    best
}

#[test]
fn a_path_through_one_of_several_gaps_takes_the_gap_that_makes_it_shortest() {
    let gaps = [(-8.5, -7.0), (-0.8, 0.8), (7.0, 8.5)];
    let mut off = Vec::new();

    for (wall_x, start_x, goal_x) in [(0.0, -5.0, 5.0), (6.0, 1.0, 9.0), (-6.0, -9.0, -1.0)] {
        let navmesh = build(&barrier_at(wall_x, &gaps));
        for az in [-9.0, -6.0, -2.0, 3.0, 6.5, 9.0] {
            for bz in [-9.0, -4.5, 1.0, 4.0, 8.0] {
                let a = Vec3::new(start_x, 0.0, az);
                let b = Vec3::new(goal_x, 0.0, bz);
                let shortest = gaps
                    .iter()
                    .map(|&gap| shortest_through_gap(a, b, wall_x, gap))
                    .fold(f32::INFINITY, f32::min);
                let path = navmesh.path(a, b).expect("every gap is open");
                let length = flat_length(&path);
                if length > shortest * 1.2 {
                    off.push(format!(
                        "wall at {wall_x}, {az} to {bz}: {length} m, shortest about {shortest} m"
                    ));
                }
            }
        }
    }

    assert_eq!(off, Vec::<String>::new());
}

#[test]
fn a_path_around_a_wall_end_hugs_the_eroded_wall_and_is_no_longer_than_needed() {
    let a = Vec3::new(-5.0, 0.0, 0.0);
    let b = Vec3::new(5.0, 0.0, 0.0);
    let reach = PROFILE.radius;
    let end_z = WALL.max[1];
    let corner = |x: f32| Vec3::new(x, 0.0, end_z);
    let bare_wall = flat_gap(a, corner(WALL.min[0]))
        + (WALL.max[0] - WALL.min[0])
        + flat_gap(corner(WALL.max[0]), b);
    let eroded_box = flat_gap(a, corner(WALL.min[0] - reach) + Vec3::Z * reach)
        + (WALL.max[0] - WALL.min[0] + 2.0 * reach)
        + flat_gap(corner(WALL.max[0] + reach) + Vec3::Z * reach, b);

    let path = WALLED.path(a, b).expect("the wall can be walked around");

    let length = flat_length(&path);
    let bends = &path[1..path.len() - 1];
    assert!(!bends.is_empty(), "{path:?}");
    for bend in bends {
        let gap = distance_to(*bend, WALL);
        assert!(
            (end_z..=end_z + reach + 2.0 * CELL).contains(&bend.z)
                && (reach - CELL..=reach + 2.0 * CELL).contains(&gap),
            "bend {bend:?} is {gap} m from the wall: {path:?}"
        );
    }
    assert!(
        length >= bare_wall - 0.05,
        "{length} m is shorter than the bare wall's {bare_wall} m"
    );
    assert!(
        length <= eroded_box + 0.1,
        "{length} m is longer than the eroded box's {eroded_box} m"
    );
}

#[test]
fn a_point_reaching_two_levels_snaps_to_the_one_with_the_smaller_vertical_gap() {
    let long_reach = AgentProfile {
        max_climb: 1.5,
        ..PROFILE
    };
    let lower_is_nearer = Vec3::new(0.0, 1.0, 0.0);
    let upper_is_nearer = Vec3::new(0.0, 1.5, 0.0);

    let floor_first = NavMesh::build(&[floor(), slab()], long_reach).expect("the slab builds");
    let slab_first = NavMesh::build(&[slab(), floor()], long_reach).expect("the slab builds");

    for navmesh in [&floor_first, &slab_first] {
        let low = snapped(navmesh, lower_is_nearer).expect("both levels are within reach");
        let high = snapped(navmesh, upper_is_nearer).expect("both levels are within reach");
        assert!(
            low.y <= 0.3,
            "snapped to height {} instead of the floor",
            low.y
        );
        assert!(
            (high.y - 2.35).abs() <= 0.3,
            "snapped to height {} instead of the slab top",
            high.y
        );
    }
}
