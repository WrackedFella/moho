//! A pawn's own work: taking a marked block, walking to it and digging it.

use std::f32::consts::{FRAC_PI_3, PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::{Vec2, Vec3};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Route searches an idle pawn starts in one tick.
const MAX_ROUTE_ATTEMPTS: usize = 4;

/// Metres closer to the waypoint that count as progress.
const PROGRESS_EPSILON: f32 = 1e-4;

/// A pawn walks once the target is within this angle of straight ahead.
const WALK_CONE: f32 = FRAC_PI_3;

/// Horizontal distance, in metres, within which the feet have reached a waypoint centre.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Radians per second a pawn turns at: one full turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Height of the eye above the feet, in metres: the centre of the 2.3 m capsule.
pub const EYE_HEIGHT: f32 = 1.15;

/// Counts ticks without progress toward the current waypoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    pub ticks: u32,
    pub best_distance: f32,
}

impl StallGuard {
    /// A guard that counts the first distance it sees as progress.
    fn fresh() -> Self {
        Self {
            ticks: 0,
            best_distance: f32::INFINITY,
        }
    }
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The marked block this pawn has claimed.
        mark: [i32; 3],
        /// Cells to walk through, from the pawn's start to the stand cell.
        route: Vec<BlockPos>,
        /// Index in `route` of the waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn stands and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// World position of the feet.
    pub feet: Vec3,
    /// Rotation about +Y in radians; `0` faces +Z and a positive turn heads toward +X.
    pub yaw: f32,
}

impl Pose {
    pub fn eye(&self) -> Vec3 {
        self.feet + Vec3::Y * EYE_HEIGHT
    }
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the feet position at the centre of the next waypoint cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim` from the eye.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// A turn and a walk for one tick, for the caller to apply as a character command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to add to the yaw this tick.
    pub yaw_delta: f32,
    /// `1.0` to walk at full speed, `0.0` to stand and turn.
    pub forward: f32,
}

/// Whether a pawn with its feet in `cell` can dig `block` from within the mining reach: within
/// one column of it in x and z, with the feet from one below to two above the block.
pub fn is_stand_cell(block: BlockPos, cell: BlockPos) -> bool {
    let d = cell - block;
    d.x.abs() <= 1 && d.z.abs() <= 1 && (-1..=2).contains(&d.y)
}

/// The six unit steps from a block to its neighbours.
const NEIGHBOURS: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

fn centre_of(block: BlockPos) -> Vec3 {
    block.as_vec3() + Vec3::splat(0.5)
}

/// Whether any face of `block` borders air.
fn has_exposed_face(grid: &VoxelGrid, block: BlockPos) -> bool {
    NEIGHBOURS.iter().any(|&d| !grid.is_solid_at(block + d))
}

/// The point on `block`'s air-facing faces, at a face centre, closest to `eye`.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Option<Vec3> {
    NEIGHBOURS
        .iter()
        .filter(|&&d| !grid.is_solid_at(block + d))
        .map(|&d| centre_of(block) + d.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

/// Horizontal distance from `feet` to the centre of `cell`.
fn horizontal_distance(feet: Vec3, cell: BlockPos) -> f32 {
    let c = centre_of(cell);
    Vec2::new(c.x - feet.x, c.z - feet.z).length()
}

/// Takes the nearest reachable unclaimed mark: claims it, equips the tool and returns the job.
fn claim_nearest(
    id: PawnId,
    pawn: &mut Pawn,
    feet: BlockPos,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let mut candidates: Vec<([i32; 3], i64)> = marks
        .unclaimed()
        .filter(|&pos| has_exposed_face(grid, BlockPos::from_array(pos)))
        .map(|pos| {
            let d = BlockPos::from_array(pos) - feet;
            (pos, d.as_i64vec3().length_squared())
        })
        .collect();
    candidates.sort_by_key(|&(_, distance)| distance);

    for (pos, _) in candidates.into_iter().take(MAX_ROUTE_ATTEMPTS) {
        let block = BlockPos::from_array(pos);
        let Some(route) = route(
            grid,
            feet,
            |cell| is_stand_cell(block, cell),
            block,
            ROUTE_NODE_BUDGET,
        ) else {
            continue;
        };
        marks.claim(pos, id);
        pawn.select_slot(TOOL_SLOT);
        return Some(Job::Mine {
            mark: pos,
            route,
            next: 0,
            stall: StallGuard::fresh(),
        });
    }
    None
}

/// Advances the job of pawn `id` by one tick.
///
/// An idle pawn claims the nearest reachable unclaimed mark, equips its tool and starts walking;
/// a mining pawn walks its route and digs from the stand cell. A job whose mark is gone, whose
/// next waypoint is no longer standable, or which has stalled for [`STALL_TICKS`] is released
/// (the mark stays) and the pawn goes idle.
pub fn step(
    job: &mut Job,
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(job, Job::Idle) {
        match claim_nearest(id, pawn, feet_cell(pose.feet), marks, grid) {
            Some(claimed) => *job = claimed,
            None => return JobAction::Wait,
        }
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
    let block = BlockPos::from_array(mark);

    if !marks.contains(mark) {
        *job = Job::Idle;
        return JobAction::Wait;
    }

    if *next < route.len() && horizontal_distance(pose.feet, route[*next]) <= ARRIVE_RADIUS {
        *next += 1;
        *stall = StallGuard::fresh();
    }

    let Some(&waypoint) = route.get(*next) else {
        let aim = nearest_exposed_face(grid, block, pose.eye());
        marks.release(mark);
        *job = Job::Idle;
        return aim.map_or(JobAction::Wait, |aim| JobAction::Dig { block, aim });
    };

    let distance = horizontal_distance(pose.feet, waypoint);
    if distance < stall.best_distance - PROGRESS_EPSILON {
        stall.best_distance = distance;
        stall.ticks = 0;
    } else {
        stall.ticks += 1;
    }

    if !standable(grid, waypoint) || stall.ticks >= STALL_TICKS {
        marks.release(mark);
        *job = Job::Idle;
        return JobAction::Wait;
    }

    JobAction::Walk {
        to: Vec3::new(
            waypoint.x as f32 + 0.5,
            waypoint.y as f32,
            waypoint.z as f32 + 0.5,
        ),
    }
}

/// Turns toward `to` at [`TURN_RATE`] (at most one tick's worth) and walks at full speed once
/// within 60 degrees of facing it, else turns in place.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let heading = (to.x - pose.feet.x).atan2(to.z - pose.feet.z);
    let error = (heading - pose.yaw + PI).rem_euclid(TAU) - PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-per_tick, per_tick),
        forward: if error.abs() < WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::colony::MineMarks;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::feet_cell;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;
    use std::f32::consts::FRAC_PI_3;

    const ME: PawnId = PawnId(0);

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -3..=12 {
            for z in -3..=3 {
                grid.mutator().place(cell(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn place(grid: &mut VoxelGrid, blocks: &[[i32; 3]]) {
        for &[x, y, z] in blocks {
            grid.mutator().place(cell(x, y, z), 1, None);
        }
    }

    fn marked(blocks: &[[i32; 3]]) -> MineMarks {
        let mut marks = MineMarks::default();
        for &pos in blocks {
            marks.toggle(pos);
        }
        marks
    }

    fn centre(c: BlockPos) -> Vec3 {
        Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5)
    }

    fn at(c: BlockPos) -> Pose {
        Pose {
            feet: centre(c),
            yaw: 0.0,
        }
    }

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|&(p, _)| p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn mining(job: &Job) -> Option<[i32; 3]> {
        match job {
            Job::Mine { mark, .. } => Some(*mark),
            Job::Idle => None,
        }
    }

    /// Walks a pawn from `start`, placing it on each waypoint it is sent to, until it digs.
    fn walk_and_dig(
        grid: &VoxelGrid,
        marks: &mut MineMarks,
        start: BlockPos,
    ) -> (Vec<Vec3>, BlockPos, Vec3, Pose) {
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let mut pose = at(start);
        let mut walked = Vec::new();
        for _ in 0..60 {
            match step(&mut job, ME, &mut pawn, pose, marks, grid) {
                JobAction::Walk { to } => {
                    walked.push(to);
                    pose = Pose { feet: to, yaw: 0.0 };
                }
                JobAction::Dig { block, aim } => return (walked, block, aim, pose),
                JobAction::Wait => panic!("the pawn waits although a way to the mark exists"),
            }
        }
        panic!("the pawn never dug");
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = ground();
        let mut marks = MineMarks::default();
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();

        let action = step(
            &mut job,
            ME,
            &mut pawn,
            at(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(mining(&job), None);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let surrounded = |cap: bool| {
            let mut grid = ground();
            place(
                &mut grid,
                &[[4, 1, 0], [6, 1, 0], [5, 1, -1], [5, 1, 1], [5, 1, 0]],
            );
            if cap {
                place(&mut grid, &[[5, 2, 0]]);
            }
            grid
        };

        let mut marks = marked(&[[5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let action = step(
            &mut job,
            ME,
            &mut pawn,
            at(cell(0, 1, 0)),
            &mut marks,
            &surrounded(true),
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(mining(&job), None);
        assert_eq!(claimed_by(&marks, [5, 1, 0]), None);

        let mut open_marks = marked(&[[5, 1, 0]]);
        let mut open_job = Job::Idle;
        step(
            &mut open_job,
            ME,
            &mut pawn,
            at(cell(0, 1, 0)),
            &mut open_marks,
            &surrounded(false),
        );

        assert_eq!(mining(&open_job), Some([5, 1, 0]), "an open top is a face");
        assert_eq!(claimed_by(&open_marks, [5, 1, 0]), Some(ME));
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = ground();
        place(&mut grid, &[[2, 1, 0], [9, 1, 0], [5, 1, 0]]);
        // A walled-in block nearer than every other mark: no air beside it, so no route.
        for x in -1..=1 {
            for y in 1..=3 {
                for z in -3..=-1 {
                    place(&mut grid, &[[x + 11, y, z]]);
                }
            }
        }
        let mut marks = marked(&[[2, 1, 0], [9, 1, 0], [11, 2, -2]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        step(
            &mut job,
            ME,
            &mut pawn,
            at(cell(12, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(mining(&job), Some([9, 1, 0]));
        assert_eq!(claimed_by(&marks, [9, 1, 0]), Some(ME));
        assert_eq!(claimed_by(&marks, [2, 1, 0]), None);
        assert_eq!(claimed_by(&marks, [11, 2, -2]), None);
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);

        let (walked, block, _, pose) = walk_and_dig(&grid, &mut marks, cell(0, 1, 0));

        assert_eq!(block, cell(5, 1, 0));
        assert_eq!(feet_cell(pose.feet), cell(4, 1, 0));
        assert!(walked.len() >= 4, "walked {walked:?}");
        assert!(walked.iter().all(|w| w.y == 1.0 && w.z == 0.5));
        assert!(walked.windows(2).all(|w| w[1].x >= w[0].x));
        assert_eq!(walked.last(), Some(&centre(cell(4, 1, 0))));
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);

        let mut from_west = marked(&[[5, 1, 0]]);
        let (_, _, aim_west, pose_west) = walk_and_dig(&grid, &mut from_west, cell(0, 1, 0));
        let mut from_east = marked(&[[5, 1, 0]]);
        let (_, _, aim_east, pose_east) = walk_and_dig(&grid, &mut from_east, cell(10, 1, 0));

        assert!(
            (aim_west - Vec3::new(5.0, 1.5, 0.5)).length() < 1e-4,
            "{aim_west}"
        );
        assert!(
            (aim_east - Vec3::new(6.0, 1.5, 0.5)).length() < 1e-4,
            "{aim_east}"
        );
        assert!((aim_west - pose_west.eye()).length() <= MINE_REACH);
        assert!((aim_east - pose_east.eye()).length() <= MINE_REACH);
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let pose = at(cell(0, 1, 0));
        step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
        assert_eq!(mining(&job), Some([5, 1, 0]), "the pawn took the mark");

        marks.toggle([5, 1, 0]);
        let action = step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(mining(&job), None);
        assert!(!marks.contains([5, 1, 0]));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0], [9, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0], [9, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let pose = at(cell(0, 1, 0));
        step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
        assert_eq!(mining(&job), Some([5, 1, 0]), "the pawn took the near mark");

        grid.mutator().remove(cell(5, 1, 0));
        marks.block_removed([5, 1, 0]);
        step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
        step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(mining(&job), Some([9, 1, 0]), "on to the next mark");
        assert_eq!(claimed_by(&marks, [9, 1, 0]), Some(ME));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);
        marks.claim([5, 1, 0], PawnId(1));
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();

        let action = step(
            &mut job,
            ME,
            &mut pawn,
            at(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(mining(&job), None);
        assert_eq!(claimed_by(&marks, [5, 1, 0]), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let pose = at(cell(0, 1, 0));
        step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
        assert_eq!(mining(&job), Some([5, 1, 0]), "the pawn took the mark");

        let released_after = (1..=STALL_TICKS + 5).find(|_| {
            step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
            mining(&job).is_none()
        });

        let released_after = released_after.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS - 2..=STALL_TICKS + 3).contains(&released_after),
            "released after {released_after} ticks"
        );
        assert!(marks.contains([5, 1, 0]));
        assert_eq!(claimed_by(&marks, [5, 1, 0]), None);
    }

    #[test]
    fn a_pawn_creeping_toward_its_waypoint_is_not_stalled() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let mut pose = at(cell(0, 1, 0));

        for tick in 0..STALL_TICKS + 30 {
            let action = step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);
            assert!(mining(&job).is_some(), "released at tick {tick}");
            if let JobAction::Walk { to } = action {
                let toward = Vec3::new(to.x - pose.feet.x, 0.0, to.z - pose.feet.z);
                pose.feet += toward.normalize_or_zero() * 0.005;
            }
        }

        assert_eq!(claimed_by(&marks, [5, 1, 0]), Some(ME));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = ground();
        place(&mut grid, &[[5, 1, 0]]);
        let mut marks = marked(&[[5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        let pose = at(cell(0, 1, 0));
        let waypoint = (0..3)
            .find_map(
                |_| match step(&mut job, ME, &mut pawn, pose, &mut marks, &grid) {
                    JobAction::Walk { to } => Some(feet_cell(to)),
                    _ => None,
                },
            )
            .expect("the pawn starts walking");

        place(&mut grid, &[[waypoint.x, waypoint.y, waypoint.z]]);
        let action = step(&mut job, ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(mining(&job), None);
        assert_eq!(claimed_by(&marks, [5, 1, 0]), None);
        assert!(marks.contains([5, 1, 0]));
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            block in (-20..20i32, -20..20i32, -20..20i32),
            offset in (-4..=4i32, -4..=5i32, -4..=4i32),
        ) {
            let block = cell(block.0, block.1, block.2);
            let stand = block + cell(offset.0, offset.1, offset.2);

            let allowed = is_stand_cell(block, stand);

            let expected = offset.0.abs() <= 1 && offset.2.abs() <= 1 && (-1..=2).contains(&offset.1);
            prop_assert_eq!(allowed, expected);
            if allowed {
                let eye = centre(stand) + Vec3::Y * EYE_HEIGHT;
                let target = centre(block) + Vec3::Y * 0.5;
                prop_assert!((eye - target).length() <= MINE_REACH);
            }
        }

        #[test]
        fn steer_turns_toward_the_target_no_faster_than_the_turn_rate(
            yaw in -10.0f32..10.0,
            error in -3.1f32..3.1,
            distance in 1.0f32..10.0,
        ) {
            let per_tick = TURN_RATE / TICK_HZ as f32;
            let facing = Pose { feet: Vec3::ZERO, yaw: 0.0 };
            let behind = steer(facing, Vec3::new(0.0, 0.0, -5.0));
            prop_assert!((behind.yaw_delta.abs() - per_tick).abs() < 1e-5, "{behind:?}");
            prop_assert_eq!(behind.forward, 0.0);
            let pose = Pose { feet: Vec3::new(1.0, 1.0, 1.0), yaw };
            let heading = yaw + error;
            let to = pose.feet + Vec3::new(heading.sin(), 0.0, heading.cos()) * distance;

            let step = steer(pose, to);

            prop_assert!(step.yaw_delta.abs() <= per_tick + 1e-5);
            prop_assert!((step.yaw_delta - error.clamp(-per_tick, per_tick)).abs() < 1e-4);
        }

        #[test]
        fn steer_walks_only_when_facing_the_target(
            yaw in -10.0f32..10.0,
            error in -3.0f32..3.0,
            distance in 1.0f32..10.0,
        ) {
            prop_assume!((error.abs() - FRAC_PI_3).abs() > 0.01);
            let pose = Pose { feet: Vec3::new(1.0, 1.0, 1.0), yaw };
            let heading = yaw + error;
            let to = pose.feet + Vec3::new(heading.sin(), 0.0, heading.cos()) * distance;

            let step = steer(pose, to);

            let expected = if error.abs() < FRAC_PI_3 { 1.0 } else { 0.0 };
            prop_assert_eq!(step.forward, expected);
        }
    }
}
