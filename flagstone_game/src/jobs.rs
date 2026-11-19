//! A pawn's own work: taking a marked block, walking to it and digging it, decided from
//! simulation state alone. The glue applies each [`JobAction`] through the pawn's own
//! actions, so a pawn on a job moves and mines exactly as a possessed one does.

use std::f32::consts::{PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// How near, horizontally, the feet must be to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint after which a walk is given up.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second: one full turn.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Height of the eye (the capsule's centre) above the feet, in world units.
pub const EYE_HEIGHT: f32 = 1.15;

/// What a pawn is doing on its own.
#[derive(Debug, Default)]
pub enum Job {
    #[default]
    Idle,
    /// Digging the marked block `mark`, by walking `route` (`next` is the waypoint the
    /// pawn is heading for) and then digging from its last cell.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// Notices a walk that has stopped getting closer to its waypoint.
#[derive(Clone, Copy, Debug)]
pub struct StallGuard {
    pub closest: f32,
    pub ticks: u32,
}

impl StallGuard {
    /// A guard that has seen no waypoint distance yet, so the first one counts as progress.
    pub fn new() -> Self {
        Self {
            closest: f32::INFINITY,
            ticks: 0,
        }
    }
}

impl Default for StallGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a pawn stands and which way it faces. Yaw is as the first-person camera's:
/// zero faces +Z, and a positive turn goes toward +X.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    /// Nothing; the pawn stays where it is.
    Wait,
    /// Walk toward `to`, a point on the ground at the centre of the waypoint cell.
    Walk { to: Vec3 },
    /// Dig `block` by aiming from the eye at `aim`, the centre of its exposed face nearest
    /// the eye.
    Dig { block: BlockPos, aim: Vec3 },
}

/// A tick's steering: turn by `yaw_delta` radians and walk `forward` (`0.0` to `1.0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Runs one tick of the pawn `id`'s job against the colony's marks and the grid.
///
/// An idle pawn takes the nearest reachable unclaimed mark and equips its tool; a pawn on
/// a mine job walks its route and, past the last waypoint, digs. The job ends, the claim
/// is released and the mark stays when the walk stalls or a waypoint stops being
/// standable; when the mark is gone the claim just ends.
pub fn step(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(job, Job::Idle) {
        start(job, pawn, id, pose, marks, grid);
    }
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        return JobAction::Wait;
    };
    let mark = *mark;

    match work(mark, route, next, stall, pose, marks, grid) {
        Tick::Act(action) => action,
        Tick::Finish { release, action } => {
            if release {
                marks.release(mark);
            }
            *job = Job::Idle;
            action
        }
    }
}

/// The most routes an idle pawn searches for in one tick.
const MAX_ROUTE_ATTEMPTS: usize = 4;

const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// What one tick of a mine job came to.
enum Tick {
    Act(JobAction),
    /// The job is over; the claim is released first when `release` is set.
    Finish {
        release: bool,
        action: JobAction,
    },
}

/// Centres of the faces of `block` that border air.
fn open_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = Vec3> + '_ {
    let centre = block.as_vec3() + Vec3::splat(0.5);
    FACES
        .into_iter()
        .filter(move |&face| !grid.is_solid_at(block + face))
        .map(move |face| centre + face.as_vec3() * 0.5)
}

fn waypoint_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// Whether a pawn standing in `cell` may dig `block`: within one column of it and with
/// the feet from one below to two above it, but never on top of it.
fn is_stand_cell(cell: BlockPos, block: BlockPos) -> bool {
    let offset = cell - block;
    offset.x.abs() <= 1
        && offset.z.abs() <= 1
        && (-1..=2).contains(&offset.y)
        && offset != BlockPos::Y
}

/// Takes the nearest reachable unclaimed mark, if there is one, and starts the job.
fn start(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) {
    let from = feet_cell(pose.feet);
    let mut candidates: Vec<[i32; 3]> = marks
        .unclaimed()
        .filter(|&mark| {
            open_faces(grid, BlockPos::from_array(mark))
                .next()
                .is_some()
        })
        .collect();
    candidates.sort_by_key(|&mark| ((BlockPos::from_array(mark) - from).length_squared(), mark));

    for mark in candidates.into_iter().take(MAX_ROUTE_ATTEMPTS) {
        let block = BlockPos::from_array(mark);
        let Some(route) = route(
            grid,
            from,
            |cell| is_stand_cell(cell, block),
            block,
            ROUTE_NODE_BUDGET,
        ) else {
            continue;
        };
        marks.claim(mark, id);
        pawn.select_slot(TOOL_SLOT);
        *job = Job::Mine {
            mark,
            route,
            next: 0,
            stall: StallGuard::new(),
        };
        return;
    }
}

/// One tick of walking `route` to `mark` and digging it from the last cell.
fn work(
    mark: [i32; 3],
    route: &[BlockPos],
    next: &mut usize,
    stall: &mut StallGuard,
    pose: Pose,
    marks: &MineMarks,
    grid: &VoxelGrid,
) -> Tick {
    let gone = Tick::Finish {
        release: false,
        action: JobAction::Wait,
    };
    let abandon = Tick::Finish {
        release: true,
        action: JobAction::Wait,
    };
    if !marks.contains(mark) {
        return gone;
    }
    let block = BlockPos::from_array(mark);
    let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
    let Some(aim) = open_faces(grid, block)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
    else {
        return abandon;
    };

    while let Some(&cell) = route.get(*next) {
        if horizontal_distance(pose.feet, waypoint_centre(cell)) > ARRIVE_RADIUS {
            break;
        }
        *next += 1;
        *stall = StallGuard::new();
    }
    let Some(&cell) = route.get(*next) else {
        return Tick::Finish {
            release: true,
            action: JobAction::Dig { block, aim },
        };
    };
    if !standable(grid, cell) {
        return abandon;
    }

    let to = waypoint_centre(cell);
    let distance = horizontal_distance(pose.feet, to);
    if distance < stall.closest {
        stall.closest = distance;
        stall.ticks = 0;
    } else {
        stall.ticks += 1;
        if stall.ticks >= STALL_TICKS {
            return abandon;
        }
    }
    Tick::Act(JobAction::Walk { to })
}

/// Steers from `pose` toward `to`: turns at [`TURN_RATE`] (at most that much per tick),
/// and walks at full speed once within 60 degrees of facing it, otherwise turns in place.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let dx = to.x - pose.feet.x;
    let dz = to.z - pose.feet.z;
    if dx.hypot(dz) < 1e-6 {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let off = (dx.atan2(dz) - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: off.clamp(-max_turn, max_turn),
        forward: if off.abs() <= FACING_LIMIT { 1.0 } else { 0.0 },
    }
}

/// How far off its target a pawn may face and still walk: 60 degrees.
const FACING_LIMIT: f32 = std::f32::consts::FRAC_PI_3;

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const ARENA: i32 = 10;
    const TOLERANCE: f32 = 1e-3;
    const ME: PawnId = PawnId(0);

    fn flat() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -ARENA..=ARENA {
            for z in -ARENA..=ARENA {
                grid.mutator().place(BlockPos::new(x, -1, z), 1, None);
            }
        }
        grid
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    fn marked(grid: &mut VoxelGrid, marks: &mut MineMarks, x: i32, y: i32, z: i32) {
        place(grid, x, y, z);
        marks.toggle([x, y, z]);
    }

    fn standing_in(x: i32, y: i32, z: i32) -> Pose {
        Pose {
            feet: Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
            yaw: 0.0,
        }
    }

    fn eye(pose: Pose) -> Vec3 {
        pose.feet + Vec3::Y * EYE_HEIGHT
    }

    fn tick(
        job: &mut Job,
        pawn: &mut Pawn,
        pose: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> JobAction {
        step(job, pawn, ME, pose, marks, grid)
    }

    /// What a pawn did walking a job through: where it was told to walk, and the dig.
    struct Outing {
        waypoints: Vec<Vec3>,
        pose: Pose,
        dig: (BlockPos, Vec3),
    }

    /// Steps `job`, moving the pawn exactly onto each point it is told to walk to,
    /// until it digs.
    fn walk_until_dig(
        job: &mut Job,
        pawn: &mut Pawn,
        start: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> Outing {
        let mut pose = start;
        let mut waypoints = Vec::new();
        for _ in 0..400 {
            match tick(job, pawn, pose, marks, grid) {
                JobAction::Walk { to } => {
                    waypoints.push(to);
                    pose.feet = to;
                }
                JobAction::Dig { block, aim } => {
                    return Outing {
                        waypoints,
                        pose,
                        dig: (block, aim),
                    };
                }
                JobAction::Wait => panic!("the pawn waited after {waypoints:?}"),
            }
        }
        panic!("the pawn never dug, it walked {waypoints:?}");
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < TOLERANCE,
            "{actual:?} is not {expected:?}"
        );
    }

    fn claimed_marks(marks: &MineMarks) -> Vec<[i32; 3]> {
        marks
            .iter()
            .filter(|(_, mark)| mark.claimed_by.is_some())
            .map(|(pos, _)| pos)
            .collect()
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = flat();
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;

        let action = tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = flat();
        for x in -ARENA..=ARENA {
            for z in -ARENA..=ARENA {
                for y in -3..=-2 {
                    place(&mut grid, x, y, z);
                }
            }
        }
        let mut marks = MineMarks::default();
        marks.toggle([4, -2, 0]);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;

        let action = tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(claimed_marks(&marks).is_empty());
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, -9, 0, 0);
        marked(&mut grid, &mut marks, 4, 0, 0);
        marked(&mut grid, &mut marks, 8, 0, 3);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        assert_eq!(pawn.equipped_tool, None);
        let mut job = Job::Idle;

        let action = tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert!(
            matches!(&job, Job::Mine { mark, .. } if *mark == [4, 0, 0]),
            "{job:?}"
        );
        assert_eq!(claimed_marks(&marks), [[4, 0, 0]]);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn an_unreachable_nearer_mark_is_passed_over_for_a_reachable_one() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        for x in 1..=5 {
            for z in -2..=2 {
                if x == 1 || x == 5 || z == -2 || z == 2 {
                    for y in 0..=2 {
                        place(&mut grid, x, y, z);
                    }
                }
            }
        }
        marked(&mut grid, &mut marks, 3, 0, 0);
        marked(&mut grid, &mut marks, -8, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;

        let action = tick(
            &mut job,
            &mut pawn,
            standing_in(-1, 0, 0),
            &mut marks,
            &grid,
        );

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(claimed_marks(&marks), [[-8, 0, 0]]);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let start = standing_in(0, 0, 0);

        let outing = walk_until_dig(&mut job, &mut pawn, start, &mut marks, &grid);

        let mut targets: Vec<(f32, f32)> = outing
            .waypoints
            .iter()
            .filter(|to| (to.x - start.feet.x).abs() > ARRIVE_RADIUS)
            .map(|to| (to.x, to.z))
            .collect();
        targets.dedup();
        assert_eq!(targets, [(1.5, 0.5), (2.5, 0.5), (3.5, 0.5)]);
        assert_eq!(outing.dig.0, BlockPos::new(4, 0, 0));
        assert_near(outing.pose.feet, Vec3::new(3.5, 0.0, 0.5));
    }

    #[test]
    fn a_waypoint_is_reached_within_the_arrive_radius_and_not_beyond() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 3, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let mut at_start = standing_in(0, 0, 0);
        let mut heading_for_first_step = None;
        for _ in 0..3 {
            match tick(&mut job, &mut pawn, at_start, &mut marks, &grid) {
                JobAction::Walk { to } if (to.x - 1.5).abs() < TOLERANCE => {
                    heading_for_first_step = Some(to);
                    break;
                }
                _ => {}
            }
        }
        let first = heading_for_first_step.expect("the pawn heads for the cell after its own");
        at_start.feet.x = first.x + ARRIVE_RADIUS + 0.05;

        let short = tick(&mut job, &mut pawn, at_start, &mut marks, &grid);
        at_start.feet.x = first.x + ARRIVE_RADIUS - 0.05;
        let close = tick(&mut job, &mut pawn, at_start, &mut marks, &grid);

        assert!(
            matches!(short, JobAction::Walk { to } if (to.x - 1.5).abs() < TOLERANCE),
            "{short:?}"
        );
        assert!(
            matches!(close, JobAction::Walk { to } if (to.x - 2.5).abs() < TOLERANCE),
            "{close:?}"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut grid = flat();
        for z in -2..=2 {
            for y in 0..=1 {
                place(&mut grid, 5, y, z);
            }
        }
        let mut marks = MineMarks::default();
        marks.toggle([5, 0, 0]);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let mut from_the_far_side = Job::Idle;
        let mut other_marks = MineMarks::default();
        other_marks.toggle([5, 0, 0]);

        let west = walk_until_dig(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);
        let east = walk_until_dig(
            &mut from_the_far_side,
            &mut Pawn::default(),
            standing_in(9, 0, 0),
            &mut other_marks,
            &grid,
        );

        assert_eq!(west.dig.0, BlockPos::new(5, 0, 0));
        assert_near(west.dig.1, Vec3::new(5.0, 0.5, 0.5));
        assert_eq!(east.dig.0, BlockPos::new(5, 0, 0));
        assert_near(east.dig.1, Vec3::new(6.0, 0.5, 0.5));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in -6i32..=6,
            bz in -6i32..=6,
            by in 0i32..=1,
            sx in -6i32..=6,
            sz in -6i32..=6,
        ) {
            let mut grid = flat();
            let mut marks = MineMarks::default();
            if by == 1 {
                place(&mut grid, bx, 0, bz);
            }
            marked(&mut grid, &mut marks, bx, by, bz);
            let mut pawn = Pawn::default();
            let mut job = Job::Idle;

            let on_the_mark = (sx, sz) == (bx, bz);
            let start = standing_in(sx, if on_the_mark { by + 1 } else { 0 }, sz);

            let outing = walk_until_dig(&mut job, &mut pawn, start, &mut marks, &grid);

            let stand = outing.pose.feet.floor().as_ivec3();
            prop_assert_eq!(outing.dig.0, BlockPos::new(bx, by, bz));
            prop_assert_ne!(stand, BlockPos::new(bx, by + 1, bz), "stood on the block it digs");
            prop_assert!((stand.x - bx).abs() <= 1 && (stand.z - bz).abs() <= 1, "stood at {:?}", stand);
            prop_assert!(stand.y >= by - 1 && stand.y <= by + 2, "stood at {:?}", stand);
            let block_centre = BlockPos::new(bx, by, bz).as_vec3() + Vec3::splat(0.5);
            prop_assert!(eye(outing.pose).distance(outing.dig.1) <= MINE_REACH);
            prop_assert!(eye(outing.pose).distance(block_centre) <= MINE_REACH);
        }
    }

    #[test]
    fn the_cell_on_top_of_the_marked_block_is_not_a_stand_cell() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let on_top = standing_in(4, 1, 0);
        let mut another_job = Job::Idle;
        let mut another_marks = MineMarks::default();
        another_marks.toggle([4, 0, 0]);

        let first = tick(&mut job, &mut pawn, on_top, &mut marks, &grid);
        let outing = walk_until_dig(
            &mut another_job,
            &mut Pawn::default(),
            on_top,
            &mut another_marks,
            &grid,
        );

        assert!(
            matches!(first, JobAction::Walk { .. }),
            "told to dig its own footing: {first:?}"
        );
        assert_eq!(outing.dig.0, BlockPos::new(4, 0, 0));
        let stand = outing.pose.feet.floor().as_ivec3();
        assert_ne!(
            stand,
            BlockPos::new(4, 1, 0),
            "dug from on top of the block"
        );
        assert_eq!(stand.y, 0, "dug from the ground beside it: {stand:?}");
    }

    #[test]
    fn the_stand_cell_is_the_nearest_by_route_not_by_straight_line() {
        let mut grid = flat();
        for z in -ARENA..=ARENA {
            if z != -3 {
                for y in 0..=1 {
                    place(&mut grid, 3, y, z);
                }
            }
        }
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 6, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;

        let outing = walk_until_dig(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        assert_eq!(outing.dig.0, BlockPos::new(6, 0, 0));
        assert_near(outing.pose.feet, Vec3::new(5.5, 0.0, -0.5));
        assert!(
            outing
                .waypoints
                .iter()
                .any(|to| (to.x, to.z) == (3.5, -2.5)),
            "round by the gap: {:?}",
            outing.waypoints
        );
    }

    #[test]
    fn two_marks_at_equal_distance_are_taken_in_key_order() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        marked(&mut grid, &mut marks, -4, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;

        let action = tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(claimed_marks(&marks), [[-4, 0, 0]]);
        assert!(
            matches!(&job, Job::Mine { mark, .. } if *mark == [-4, 0, 0]),
            "{job:?}"
        );
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 6, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let started = tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);
        assert!(matches!(started, JobAction::Walk { .. }), "{started:?}");

        marks.toggle([6, 0, 0]);
        let mut halfway = standing_in(0, 0, 0);
        halfway.feet.x = 3.0;
        let action = tick(&mut job, &mut pawn, halfway, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(
            grid.is_solid_at(BlockPos::new(6, 0, 0)),
            "the block is not removed"
        );
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 6, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        tick(&mut job, &mut pawn, standing_in(0, 0, 0), &mut marks, &grid);

        grid.mutator().remove(BlockPos::new(6, 0, 0));
        marks.block_removed([6, 0, 0]);
        let action = tick(&mut job, &mut pawn, standing_in(1, 0, 0), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(claimed_marks(&marks).is_empty());
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        marks.claim([4, 0, 0], PawnId(7));
        let mut second = Pawn::default();
        let mut job = Job::Idle;

        let alone = step(
            &mut job,
            &mut second,
            PawnId(8),
            standing_in(0, 0, 0),
            &mut marks,
            &grid,
        );
        marked(&mut grid, &mut marks, -7, 0, 0);
        let with_another = step(
            &mut job,
            &mut second,
            PawnId(8),
            standing_in(0, 0, 0),
            &mut marks,
            &grid,
        );

        assert_eq!(alone, JobAction::Wait);
        assert!(
            matches!(&job, Job::Mine { mark, .. } if *mark == [-7, 0, 0]),
            "{job:?}"
        );
        assert!(matches!(with_another, JobAction::Walk { .. }));
        assert_eq!(claimed_marks(&marks), [[-7, 0, 0], [4, 0, 0]]);
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let stuck = standing_in(0, 0, 0);
        let mut ticks = 1;
        let claimed = tick(&mut job, &mut pawn, stuck, &mut marks, &grid);
        assert!(matches!(claimed, JobAction::Walk { .. }), "{claimed:?}");

        let released = loop {
            ticks += 1;
            assert!(ticks <= STALL_TICKS + 10, "the pawn never gave up");
            let action = tick(&mut job, &mut pawn, stuck, &mut marks, &grid);
            if matches!(job, Job::Idle) {
                break action;
            }
        };

        assert!(
            ticks.abs_diff(STALL_TICKS) <= 2,
            "gave up after {ticks} ticks, not about {STALL_TICKS}"
        );
        assert_eq!(released, JobAction::Wait);
        assert!(marks.contains([4, 0, 0]), "the mark stays");
        assert!(claimed_marks(&marks).is_empty(), "the claim is released");
    }

    #[test]
    fn a_walk_that_makes_progress_is_not_stalled() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 4, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let mut pose = standing_in(0, 0, 0);
        let mut ticks = 0;
        let mut heading_for = Vec3::NAN;
        let mut ticks_on_it = 0;
        let mut longest_on_one_waypoint = 0;

        let dug = loop {
            ticks += 1;
            assert!(ticks < 2000, "the pawn never arrived");
            match tick(&mut job, &mut pawn, pose, &mut marks, &grid) {
                JobAction::Walk { to } => {
                    if to == heading_for {
                        ticks_on_it += 1;
                    } else {
                        heading_for = to;
                        ticks_on_it = 1;
                    }
                    longest_on_one_waypoint = longest_on_one_waypoint.max(ticks_on_it);
                    let toward = (to.x - pose.feet.x).signum();
                    pose.feet.x += toward * 0.005_f32.min((to.x - pose.feet.x).abs());
                    pose.feet.z = to.z;
                    assert!(
                        matches!(job, Job::Mine { .. }),
                        "released after {ticks} ticks"
                    );
                }
                JobAction::Dig { block, .. } => break block,
                JobAction::Wait => panic!("waited after {ticks} ticks"),
            }
        };

        assert!(
            longest_on_one_waypoint > STALL_TICKS,
            "one waypoint took {longest_on_one_waypoint} ticks, within the stall limit"
        );
        assert_eq!(dug, BlockPos::new(4, 0, 0));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = flat();
        let mut marks = MineMarks::default();
        marked(&mut grid, &mut marks, 6, 0, 0);
        let mut pawn = Pawn::default();
        let mut job = Job::Idle;
        let start = standing_in(0, 0, 0);
        tick(&mut job, &mut pawn, start, &mut marks, &grid);

        for x in 0..=5 {
            place(&mut grid, x, 2, 0);
        }
        let action = tick(&mut job, &mut pawn, start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle), "{job:?}");
        assert!(marks.contains([6, 0, 0]), "the mark stays");
        assert!(claimed_marks(&marks).is_empty(), "the claim is released");
    }

    fn per_tick_turn() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    fn facing(angle: f32) -> Vec3 {
        Vec3::new(angle.sin(), 0.0, angle.cos()) * 5.0
    }

    fn at_yaw(yaw: f32) -> Pose {
        Pose {
            feet: Vec3::ZERO,
            yaw,
        }
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let right = steer(at_yaw(0.0), facing(FRAC_PI_2));
        let left = steer(at_yaw(0.0), facing(-FRAC_PI_2));
        let nearly_there = steer(at_yaw(0.0), facing(0.01));
        let across_the_wrap = steer(at_yaw(3.0), facing(-3.0));

        assert!(
            (right.yaw_delta - per_tick_turn()).abs() < TOLERANCE,
            "{right:?}"
        );
        assert!(
            (left.yaw_delta + per_tick_turn()).abs() < TOLERANCE,
            "{left:?}"
        );
        assert!(
            (nearly_there.yaw_delta - 0.01).abs() < TOLERANCE,
            "no overshoot: {nearly_there:?}"
        );
        assert!(
            (across_the_wrap.yaw_delta - per_tick_turn()).abs() < TOLERANCE,
            "the short way round: {across_the_wrap:?}"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let ahead = steer(at_yaw(0.0), facing(0.0));
        let within_sixty = steer(at_yaw(0.0), facing(50.0_f32.to_radians()));
        let past_sixty = steer(at_yaw(0.0), facing(70.0_f32.to_radians()));
        let behind = steer(at_yaw(0.0), facing(PI));

        assert_eq!(ahead.forward, 1.0);
        assert_eq!(within_sixty.forward, 1.0);
        assert_eq!(past_sixty.forward, 0.0);
        assert_eq!(behind.forward, 0.0);
        assert!(past_sixty.yaw_delta > 0.0, "it still turns in place");
    }
}
