//! A pawn's job loop: pick a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use std::f32::consts::{PI, TAU};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Horizontal distance, in metres, at which the feet count as at a waypoint.
pub const ARRIVE_RADIUS: f32 = 0.35;
/// Ticks without progress toward a waypoint before the claim is given up.
pub const STALL_TICKS: u32 = 120;
/// Turn speed in radians per second: one turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;
/// Marks an idle pawn tries to route to in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Progress bookkeeping for the current waypoint; plain values only.
#[derive(Clone, Debug, PartialEq)]
pub struct StallGuard {
    /// The closest the feet have been to the waypoint, horizontally.
    pub best_distance: f32,
    /// Ticks since `best_distance` last improved.
    pub ticks: u32,
}

impl Default for StallGuard {
    /// A guard that counts the first distance it sees as progress.
    fn default() -> Self {
        Self {
            best_distance: f32::MAX,
            ticks: 0,
        }
    }
}

/// Metres closer to a waypoint that count as progress toward it.
const PROGRESS_EPSILON: f32 = 0.01;
/// A walker within this angle of its target walks at full speed.
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;
const FACE_OFFSETS: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// What a pawn is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        mark: BlockPos,
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn stands and faces: the inputs a job step reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The point on the ground under the pawn.
    pub feet: Vec3,
    /// Where the pawn's dig starts: the capsule centre.
    pub eye: Vec3,
    /// Heading in radians; the pawn faces `(sin yaw, 0, cos yaw)`.
    pub yaw: f32,
}

/// What the glue does with a job step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of a waypoint cell at its feet height.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// A tick's turn and walk toward a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to add to the heading this tick.
    pub yaw_delta: f32,
    /// Walk input, `0.0` to `1.0`.
    pub forward: f32,
}

/// Advances `job` one tick for pawn `id`.
///
/// An idle pawn claims the nearest reachable mark; a working one walks its route and then
/// digs. Losing the mark, or a route that cannot be walked, frees the claim (the mark
/// stays) and the pawn waits a tick as `Idle`.
pub fn step(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        take_a_mark(job, pawn, id, pose, marks, grid);
        return JobAction::Wait;
    };
    let block = *mark;
    if !marks.contains(block.to_array()) {
        *job = Job::Idle;
        return JobAction::Wait;
    }

    while let Some(&cell) = route.get(*next) {
        if horizontal_distance(pose.feet, waypoint(cell)) > ARRIVE_RADIUS {
            break;
        }
        *next += 1;
        *stall = StallGuard::default();
    }

    let Some(&cell) = route.get(*next) else {
        return match nearest_open_face(grid, block, pose.eye) {
            Some(aim) => JobAction::Dig { block, aim },
            None => give_up(job, marks, block),
        };
    };
    if !standable(grid, cell) {
        return give_up(job, marks, block);
    }
    let to = waypoint(cell);
    let distance = horizontal_distance(pose.feet, to);
    if distance < stall.best_distance - PROGRESS_EPSILON {
        stall.best_distance = distance;
        stall.ticks = 0;
    } else {
        stall.ticks += 1;
        if stall.ticks >= STALL_TICKS {
            return give_up(job, marks, block);
        }
    }
    JobAction::Walk { to }
}

/// Frees the claim on `block` and drops the job; the mark stays.
fn give_up(job: &mut Job, marks: &mut MineMarks, block: BlockPos) -> JobAction {
    marks.release(block.to_array());
    *job = Job::Idle;
    JobAction::Wait
}

/// Claims the nearest reachable unclaimed mark, if one routes within this tick's attempts.
fn take_a_mark(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) {
    let start = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| open_faces(grid, block).next().is_some())
        .collect();
    candidates.sort_by_key(|&block| (block - start).length_squared());

    for block in candidates.into_iter().take(ROUTE_ATTEMPTS_PER_TICK) {
        let in_reach_of = |cell: BlockPos| {
            (cell.x - block.x).abs() <= 1
                && (cell.z - block.z).abs() <= 1
                && (block.y - 1..=block.y + 2).contains(&cell.y)
        };
        let Some(route) = route(grid, start, in_reach_of, block, ROUTE_NODE_BUDGET) else {
            continue;
        };
        if marks.claim(block.to_array(), id) {
            pawn.select_slot(TOOL_SLOT);
            *job = Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            };
            return;
        }
    }
}

/// The centre of `cell` at the height of its floor.
fn waypoint(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a - b).with_y(0.0).length()
}

/// The centres of the faces of `block` that touch air.
fn open_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = Vec3> + '_ {
    FACE_OFFSETS
        .into_iter()
        .filter(move |&offset| !grid.is_solid_at(block + offset))
        .map(move |offset| block.as_vec3() + Vec3::splat(0.5) + offset.as_vec3() * 0.5)
}

fn nearest_open_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Option<Vec3> {
    open_faces(grid, block)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

/// The turn and walk that take a pawn at `pose` toward `to`.
///
/// The pawn turns at `TURN_RATE` and walks only once the target is within 60 degrees of
/// where it faced before this tick's turn.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let offset = (to - pose.feet).with_y(0.0);
    if offset.length_squared() < f32::EPSILON {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let wanted = offset.x.atan2(offset.z);
    let error = (wanted - pose.yaw + PI).rem_euclid(TAU) - PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-per_tick, per_tick),
        forward: if error.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::standable;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;
    use std::f32::consts::{FRAC_PI_2, PI};
    use std::ops::RangeInclusive;

    const ME: PawnId = PawnId(0);
    const EYE_HEIGHT: f32 = 1.15;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn fill(
        grid: &mut VoxelGrid,
        xs: RangeInclusive<i32>,
        ys: RangeInclusive<i32>,
        zs: RangeInclusive<i32>,
    ) {
        for x in xs {
            for y in ys.clone() {
                for z in zs.clone() {
                    grid.mutator().place(at(x, y, z), 1, None);
                }
            }
        }
    }

    /// A floor one block thick, its top face at y = 1, over `0..width` by `0..depth`.
    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        fill(&mut grid, 0..=width - 1, 0..=0, 0..=depth - 1);
        grid
    }

    /// The centre of `cell` at the height of its floor.
    fn centre(cell: BlockPos) -> Vec3 {
        Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
    }

    fn pose_at(feet: Vec3) -> Pose {
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    fn standing_in(cell: BlockPos) -> Pose {
        pose_at(centre(cell))
    }

    fn marked(blocks: &[BlockPos]) -> MineMarks {
        let mut marks = MineMarks::default();
        for block in blocks {
            marks.toggle(block.to_array());
        }
        marks
    }

    fn claimed_by(marks: &MineMarks, block: BlockPos) -> Option<PawnId> {
        marks
            .iter()
            .find(|&(pos, _)| pos == block.to_array())
            .expect("the block is marked")
            .1
            .claimed_by
    }

    /// A pawn with its own job, stepping against shared marks and grid.
    struct Walker {
        pawn: Pawn,
        id: PawnId,
        job: Job,
    }

    impl Walker {
        fn new() -> Self {
            Self::with_id(ME)
        }

        fn with_id(id: PawnId) -> Self {
            Self {
                pawn: Pawn::default(),
                id,
                job: Job::Idle,
            }
        }

        fn step(&mut self, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
            step(&mut self.job, &mut self.pawn, self.id, pose, marks, grid)
        }

        fn route(&self) -> Vec<BlockPos> {
            match &self.job {
                Job::Mine { route, .. } => route.clone(),
                Job::Idle => panic!("the pawn has no job"),
            }
        }
    }

    /// Follows the pawn's walk by standing it on each target it is sent to, until it digs.
    fn walk_until_dig(
        walker: &mut Walker,
        start: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> Option<(Pose, Vec<Vec3>, JobAction)> {
        let mut pose = start;
        let mut targets: Vec<Vec3> = Vec::new();
        for _ in 0..300 {
            match walker.step(pose, marks, grid) {
                JobAction::Walk { to } => {
                    if targets.last() != Some(&to) {
                        targets.push(to);
                    }
                    pose = pose_at(Vec3::new(to.x, to.y.floor(), to.z));
                }
                JobAction::Dig { block, aim } => {
                    return Some((pose, targets, JobAction::Dig { block, aim }));
                }
                JobAction::Wait => {}
            }
        }
        None
    }

    fn near_far_world() -> (VoxelGrid, MineMarks) {
        let mut grid = floor(12, 5);
        fill(&mut grid, 9..=9, 1..=1, 2..=2);
        fill(&mut grid, 3..=3, 1..=1, 2..=2);
        (grid, marked(&[at(9, 1, 2), at(3, 1, 2)]))
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = floor(8, 8);
        let mut marks = MineMarks::default();
        let mut walker = Walker::new();
        walker.pawn.select_slot(3);

        let action = walker.step(standing_in(at(3, 1, 3)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(walker.job, Job::Idle);
        assert_eq!(
            walker.pawn.equipped_tool, None,
            "an idle pawn is not touched"
        );
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = VoxelGrid::new(16);
        fill(&mut grid, 0..=9, -1..=1, 0..=9);
        let buried = at(5, 0, 5);
        let mut marks = marked(&[buried]);
        let mut walker = Walker::new();
        assert!(
            standable(&grid, at(5, 2, 5)),
            "the top of the floor is a reachable stand cell within reach of the block"
        );

        let action = walker.step(standing_in(at(1, 2, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(walker.job, Job::Idle);
        assert_eq!(claimed_by(&marks, buried), None);
    }

    #[test]
    fn four_unreachable_marks_nearer_than_a_reachable_one_cost_a_tick() {
        let mut grid = floor(14, 9);
        fill(&mut grid, 3..=7, 1..=3, 3..=7);
        for x in 4..=6 {
            for y in 1..=3 {
                for z in 4..=6 {
                    grid.mutator().remove(at(x, y, z));
                }
            }
        }
        let pen = [at(4, 1, 4), at(4, 1, 5), at(4, 1, 6), at(5, 1, 4)];
        for block in pen {
            grid.mutator().place(block, 1, None);
        }
        let reachable = at(11, 1, 5);
        grid.mutator().place(reachable, 1, None);
        let mut blocks = pen.to_vec();
        blocks.push(reachable);
        let mut marks = marked(&blocks);
        let mut walker = Walker::new();
        let start = standing_in(at(1, 1, 5));

        let first = walker.step(start, &mut marks, &grid);

        assert_eq!(first, JobAction::Wait, "four attempts are spent on the pen");
        assert_eq!(walker.job, Job::Idle);
        assert_eq!(claimed_by(&marks, reachable), None);
    }

    #[test]
    fn marks_at_equal_distance_are_taken_in_key_order() {
        let mut grid = floor(12, 9);
        fill(&mut grid, 3..=3, 1..=1, 5..=5);
        fill(&mut grid, 7..=7, 1..=1, 5..=5);
        let mut marks = marked(&[at(7, 1, 5), at(3, 1, 5)]);
        let mut walker = Walker::new();

        walker.step(standing_in(at(5, 1, 5)), &mut marks, &grid);

        assert!(
            matches!(&walker.job, Job::Mine { mark, .. } if *mark == at(3, 1, 5)),
            "{:?}",
            walker.job
        );
        assert_eq!(claimed_by(&marks, at(3, 1, 5)), Some(ME));
        assert_eq!(claimed_by(&marks, at(7, 1, 5)), None);
    }

    #[test]
    fn a_mark_that_cannot_be_reached_is_not_claimed() {
        let mut grid = floor(9, 9);
        fill(&mut grid, 3..=5, 1..=3, 3..=5);
        grid.mutator().remove(at(4, 2, 4));
        grid.mutator().remove(at(4, 3, 4));
        let mut marks = marked(&[at(4, 1, 4)]);
        let mut walker = Walker::new();

        let action = walker.step(standing_in(at(1, 1, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(walker.job, Job::Idle);
        assert_eq!(claimed_by(&marks, at(4, 1, 4)), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let (grid, mut marks) = near_far_world();
        let mut walker = Walker::new();
        walker.pawn.select_slot(3);

        walker.step(standing_in(at(11, 1, 2)), &mut marks, &grid);

        let Job::Mine { mark, route, .. } = &walker.job else {
            panic!("the pawn took no job: {:?}", walker.job);
        };
        assert_eq!(
            *mark,
            at(9, 1, 2),
            "the nearer mark, not the first in key order"
        );
        assert_eq!(route, &[at(10, 1, 2)], "one step to the stand cell");
        assert_eq!(claimed_by(&marks, at(9, 1, 2)), Some(ME));
        assert_eq!(claimed_by(&marks, at(3, 1, 2)), None);
        assert_eq!(walker.pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(walker.pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn the_stand_cell_is_the_nearest_one_within_a_column_of_the_block() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let mut marks = marked(&[at(8, 1, 2)]);
        let mut walker = Walker::new();

        walker.step(standing_in(at(1, 1, 2)), &mut marks, &grid);

        let route = walker.route();
        let stand = *route.last().expect("a walk of several cells");
        assert_eq!(route.len(), 6, "{route:?}");
        assert_eq!(stand, at(7, 1, 2));
        assert!(standable(&grid, stand));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let mut marks = marked(&[at(8, 1, 2)]);
        let mut walker = Walker::new();
        let start = standing_in(at(1, 1, 2));
        walker.step(start, &mut marks, &grid);
        let route = walker.route();

        let waiting: Vec<JobAction> = (0..3)
            .map(|_| walker.step(start, &mut marks, &grid))
            .collect();
        let (pose, targets, dig) = walk_until_dig(&mut walker, start, &mut marks, &grid)
            .expect("the pawn digs once it has walked the route");

        let over = |to: Vec3, cell: BlockPos| {
            (to.x - (cell.x as f32 + 0.5)).abs() < 1e-4
                && (to.z - (cell.z as f32 + 0.5)).abs() < 1e-4
                && (cell.y as f32..=cell.y as f32 + 1.0).contains(&to.y)
        };
        let [
            JobAction::Walk { to: first },
            JobAction::Walk { to: again },
            JobAction::Walk { to: last },
        ] = waiting[..]
        else {
            panic!("a pawn that has not arrived walks: {waiting:?}");
        };
        assert!(over(first, route[0]), "{first}");
        assert!(
            first == again && again == last,
            "the same waypoint until arrival: {waiting:?}"
        );
        assert_eq!(targets.len(), route.len(), "{targets:?} for {route:?}");
        for (to, &cell) in targets.iter().zip(&route) {
            assert!(over(*to, cell), "walked to {to}, not over {cell:?}");
        }
        assert!(over(pose.feet, *route.last().expect("a walk")));
        assert!(
            matches!(dig, JobAction::Dig { block, .. } if block == at(8, 1, 2)),
            "{dig:?}"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut grid = floor(10, 5);
        fill(&mut grid, 5..=5, 1..=1, 2..=2);
        let block = at(5, 1, 2);
        let aim_from = |stand: BlockPos| {
            let mut marks = marked(&[block]);
            marks.claim(block.to_array(), ME);
            let mut walker = Walker::new();
            walker.job = Job::Mine {
                mark: block,
                route: vec![stand],
                next: 0,
                stall: StallGuard::default(),
            };
            walker.step(standing_in(stand), &mut marks, &grid)
        };

        let from_the_side = aim_from(at(4, 1, 2));
        let from_above = aim_from(at(5, 2, 2));

        let close = |a: Vec3, b: Vec3| a.distance(b) < 1e-4;
        let JobAction::Dig { block: dug, aim } = from_the_side else {
            panic!("expected a dig, got {from_the_side:?}");
        };
        assert_eq!(dug, block);
        assert!(close(aim, Vec3::new(5.0, 1.5, 2.5)), "the west face: {aim}");
        let JobAction::Dig { aim, .. } = from_above else {
            panic!("expected a dig, got {from_above:?}");
        };
        assert!(close(aim, Vec3::new(5.5, 2.0, 2.5)), "the top face: {aim}");
    }

    #[test]
    fn a_face_covered_by_another_block_is_not_aimed_at() {
        let mut grid = floor(10, 5);
        fill(&mut grid, 5..=5, 1..=1, 2..=2);
        fill(&mut grid, 4..=4, 1..=1, 2..=2);
        let block = at(5, 1, 2);
        let mut marks = marked(&[block]);
        marks.claim(block.to_array(), ME);
        let mut walker = Walker::new();
        walker.job = Job::Mine {
            mark: block,
            route: vec![at(4, 1, 1)],
            next: 0,
            stall: StallGuard::default(),
        };
        // Beside the stand cell's centre, where the covered west face is the nearest of all.
        let pose = pose_at(Vec3::new(4.2, 1.0, 1.5));

        let action = walker.step(pose, &mut marks, &grid);

        let JobAction::Dig { aim, .. } = action else {
            panic!("expected a dig, got {action:?}");
        };
        assert!(
            aim.distance(Vec3::new(5.5, 1.5, 2.0)) < 1e-4,
            "the north face is the nearest one open to the air: {aim}"
        );
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let (grid, mut marks) = near_far_world();
        let mut walker = Walker::new();
        let start = standing_in(at(11, 1, 2));
        walker.step(start, &mut marks, &grid);
        assert!(
            matches!(walker.job, Job::Mine { .. }),
            "walking to the mark"
        );
        marks.toggle(at(9, 1, 2).to_array());
        marks.toggle(at(3, 1, 2).to_array());

        let action = walker.step(start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(walker.job, Job::Idle);
        assert!(!marks.contains(at(9, 1, 2).to_array()));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let (mut grid, mut marks) = near_far_world();
        let mut walker = Walker::new();
        let start = standing_in(at(11, 1, 2));
        walker.step(start, &mut marks, &grid);
        grid.mutator().remove(at(9, 1, 2));
        marks.block_removed(at(9, 1, 2).to_array());

        let after = walker.step(start, &mut marks, &grid);
        let released = walker.job.clone();
        walker.step(start, &mut marks, &grid);

        assert!(
            !matches!(after, JobAction::Dig { .. }),
            "nothing is left to dig: {after:?}"
        );
        assert!(
            !matches!(released, Job::Mine { mark, .. } if mark == at(9, 1, 2)),
            "the pawn still works on a block that is gone: {released:?}"
        );
        assert!(
            matches!(&walker.job, Job::Mine { mark, .. } if *mark == at(3, 1, 2)),
            "the pawn takes the next mark: {:?}",
            walker.job
        );
        assert_eq!(claimed_by(&marks, at(3, 1, 2)), Some(ME));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let block = at(8, 1, 2);
        let mut marks = marked(&[block]);
        marks.claim(block.to_array(), ME);
        let mut second = Walker::with_id(PawnId(1));
        let start = standing_in(at(1, 1, 2));

        let action = second.step(start, &mut marks, &grid);
        let while_held = (action, second.job.clone(), claimed_by(&marks, block));
        marks.release(block.to_array());
        second.step(start, &mut marks, &grid);

        assert_eq!(while_held, (JobAction::Wait, Job::Idle, Some(ME)));
        assert!(
            matches!(second.job, Job::Mine { .. }),
            "free again, it is taken"
        );
        assert_eq!(claimed_by(&marks, block), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let (grid, mut marks) = near_far_world();
        let mut walker = Walker::new();
        let start = standing_in(at(11, 1, 2));
        walker.step(start, &mut marks, &grid);

        let mut stalled_for = 0;
        while matches!(walker.job, Job::Mine { .. }) && stalled_for < 2 * STALL_TICKS {
            walker.step(start, &mut marks, &grid);
            stalled_for += 1;
        }

        assert!(
            (STALL_TICKS - 1..=STALL_TICKS + 2).contains(&stalled_for),
            "gave up after {stalled_for} ticks without progress"
        );
        assert_eq!(walker.job, Job::Idle);
        assert!(marks.contains(at(9, 1, 2).to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, at(9, 1, 2)), None, "the claim is freed");
    }

    #[test]
    fn a_walk_that_keeps_making_progress_is_not_stalled() {
        let mut grid = floor(14, 5);
        fill(&mut grid, 12..=12, 1..=1, 2..=2);
        let mut marks = marked(&[at(12, 1, 2)]);
        let mut walker = Walker::new();
        let mut pose = standing_in(at(1, 1, 2));
        let mut dug = false;

        for _ in 0..600 {
            match walker.step(pose, &mut marks, &grid) {
                JobAction::Walk { to } => {
                    let toward = (to - pose.feet).with_y(0.0).normalize_or_zero() * 0.05;
                    pose = pose_at(Vec3::new(
                        pose.feet.x + toward.x,
                        to.y.floor(),
                        pose.feet.z + toward.z,
                    ));
                }
                JobAction::Dig { .. } => {
                    dug = true;
                    break;
                }
                JobAction::Wait => {}
            }
            assert!(
                matches!(walker.job, Job::Mine { .. }),
                "gave up at {:?} while walking",
                pose.feet
            );
        }

        assert!(dug, "the pawn never reached the stand cell");
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let block = at(8, 1, 2);
        let mut marks = marked(&[block]);
        let mut walker = Walker::new();
        let start = standing_in(at(1, 1, 2));
        walker.step(start, &mut marks, &grid);
        let next_waypoint = walker.route()[0];
        grid.mutator().remove(next_waypoint - BlockPos::Y);

        let action = walker.step(start, &mut marks, &grid);

        assert!(!matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(walker.job, Job::Idle);
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, block), None);
    }

    #[test]
    fn a_pawn_already_in_the_stand_envelope_digs_the_tick_after_claiming() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let block = at(8, 1, 2);
        let mut marks = marked(&[block]);
        let mut walker = Walker::new();
        let beside = standing_in(at(7, 1, 2));

        let claiming = walker.step(beside, &mut marks, &grid);
        let next = walker.step(beside, &mut marks, &grid);

        assert!(
            !matches!(claiming, JobAction::Walk { .. }),
            "nothing to walk to: {claiming:?}"
        );
        assert!(
            matches!(next, JobAction::Dig { block: dug, .. } if dug == block),
            "{next:?}"
        );
    }

    #[test]
    fn covering_every_open_face_mid_walk_gives_the_mark_up() {
        let mut grid = floor(12, 5);
        fill(&mut grid, 8..=8, 1..=1, 2..=2);
        let block = at(8, 1, 2);
        let mut marks = marked(&[block]);
        let mut walker = Walker::new();
        let start = standing_in(at(1, 1, 2));
        walker.step(start, &mut marks, &grid);
        assert!(matches!(walker.job, Job::Mine { .. }), "walking");
        for cover in [
            at(8, 2, 2),
            at(7, 1, 2),
            at(9, 1, 2),
            at(8, 1, 1),
            at(8, 1, 3),
        ] {
            grid.mutator().place(cover, 1, None);
        }

        let action = walker.step(start, &mut marks, &grid);

        assert!(!matches!(action, JobAction::Dig { .. }), "{action:?}");
        assert_eq!(walker.job, Job::Idle);
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert!(
            marks.unclaimed().any(|pos| pos == block.to_array()),
            "the claim is released"
        );
    }

    #[test]
    fn a_mark_cancelled_and_made_again_is_not_walked_on_a_stale_claim() {
        let (grid, mut marks) = near_far_world();
        marks.toggle(at(3, 1, 2).to_array());
        let mut walker = Walker::new();
        let start = standing_in(at(11, 1, 2));
        walker.step(start, &mut marks, &grid);
        assert!(matches!(walker.job, Job::Mine { .. }), "walking");
        marks.toggle(at(9, 1, 2).to_array());
        marks.toggle(at(9, 1, 2).to_array());

        walker.step(start, &mut marks, &grid);

        if let Job::Mine { mark, .. } = &walker.job {
            assert_eq!(
                claimed_by(&marks, *mark),
                Some(ME),
                "walking to {mark:?} on a claim the marks do not hold"
            );
        }
    }

    #[test]
    fn steer_walks_at_exactly_sixty_degrees_and_not_just_past_it() {
        let at_sixty = steer(pose_facing(0.0), heading(60.0_f32.to_radians()));
        let past_sixty = steer(pose_facing(0.0), heading(60.5_f32.to_radians()));
        let past_sixty_the_other_way = steer(pose_facing(0.0), heading((-60.5_f32).to_radians()));

        assert_eq!(at_sixty.forward, 1.0, "60 degrees is still facing it");
        assert_eq!(past_sixty.forward, 0.0);
        assert_eq!(past_sixty_the_other_way.forward, 0.0);
    }

    #[test]
    fn steer_turns_the_short_way_across_the_wrap_for_a_small_gap() {
        let up = steer(pose_facing(3.1), heading(-3.1));
        let down = steer(pose_facing(-3.1), heading(3.1));

        assert!((up.yaw_delta - (2.0 * PI - 6.2)).abs() < 1e-3, "{up:?}");
        assert!((down.yaw_delta + (2.0 * PI - 6.2)).abs() < 1e-3, "{down:?}");
    }

    fn turn_per_tick() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    fn pose_facing(yaw: f32) -> Pose {
        Pose {
            yaw,
            ..pose_at(Vec3::ZERO)
        }
    }

    /// A point five metres away along the heading `angle`.
    fn heading(angle: f32) -> Vec3 {
        Vec3::new(angle.sin(), 0.0, angle.cos()) * 5.0
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let cases = [
            ("far left", 0.0, FRAC_PI_2, turn_per_tick()),
            ("far right", 0.0, -FRAC_PI_2, -turn_per_tick()),
            ("slightly left", 0.0, 0.02, 0.02),
            ("slightly right", 0.0, -0.03, -0.03),
            ("dead ahead", 0.0, 0.0, 0.0),
            ("the short way across the wrap", 3.0, -3.0, turn_per_tick()),
            ("the short way back across it", -3.0, 3.0, -turn_per_tick()),
        ];

        for (name, yaw, target, expected) in cases {
            let step = steer(pose_facing(yaw), heading(target));

            assert!(
                (step.yaw_delta - expected).abs() < 1e-4,
                "{name}: turned {} not {expected}",
                step.yaw_delta
            );
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let cases = [
            (0.0_f32, 1.0),
            (30.0, 1.0),
            (59.0, 1.0),
            (-59.0, 1.0),
            (61.0, 0.0),
            (-61.0, 0.0),
            (90.0, 0.0),
            (180.0, 0.0),
        ];

        for (degrees, expected) in cases {
            let step = steer(pose_facing(0.0), heading(degrees.to_radians()));

            assert!(
                (step.forward - expected).abs() < 1e-6,
                "{degrees} degrees off: walks at {}",
                step.forward
            );
        }
    }

    #[test]
    fn steer_is_judged_by_the_heading_before_the_turn() {
        let facing_away = steer(pose_facing(PI), heading(0.0));

        assert_eq!(facing_away.forward, 0.0);
        assert!(facing_away.yaw_delta.abs() > 0.0, "it turns in place");
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            block in (2..12i32, 2..12i32, 1..=2i32),
            start in (0..14i32, 0..14i32),
        ) {
            let (bx, bz, height) = block;
            prop_assume!((start.0, start.1) != (bx, bz));
            let mut grid = floor(14, 14);
            fill(&mut grid, bx..=bx, 1..=height, bz..=bz);
            let top = at(bx, height, bz);
            let mut marks = marked(&[top]);
            let mut walker = Walker::new();

            let dug = walk_until_dig(
                &mut walker,
                standing_in(at(start.0, 1, start.1)),
                &mut marks,
                &grid,
            );

            let (pose, _, action) = dug.expect("the block is reachable on open ground");
            let JobAction::Dig { block: dug_block, aim } = action else {
                panic!("expected a dig, got {action:?}");
            };
            let stand = pose.feet.floor().as_ivec3();
            prop_assert_eq!(dug_block, top);
            prop_assert!((stand.x - bx).abs() <= 1 && (stand.z - bz).abs() <= 1, "{stand:?}");
            prop_assert!((-1..=2).contains(&(stand.y - height)), "{stand:?}");
            prop_assert!(pose.eye.distance(aim) <= MINE_REACH, "aim {aim} from {}", pose.eye);
            prop_assert!(
                pose.eye.distance(top.as_vec3() + Vec3::splat(0.5)) <= MINE_REACH,
                "block centre from {}",
                pose.eye
            );
            let inside = |v: f32, lo: i32| v >= lo as f32 - 1e-4 && v <= lo as f32 + 1.0 + 1e-4;
            prop_assert!(
                inside(aim.x, bx) && inside(aim.y, height) && inside(aim.z, bz),
                "the aim {aim} is on the block"
            );
        }
    }
}
