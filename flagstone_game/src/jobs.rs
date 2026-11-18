//! A pawn's own work: the mine job loop and the steering that walks its route.

use std::f32::consts::{PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::{Vec3, Vec3Swizzles};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Horizontal distance from a waypoint centre, in world units, at which a pawn counts as
/// having reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// Fastest turn, in radians per second: one full turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Half-angle, in radians, of the cone around its facing inside which a pawn walks
/// instead of turning in place.
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// Route searches an idle pawn makes in one tick.
const MAX_ROUTE_ATTEMPTS: usize = 4;

const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// Counts ticks a walking pawn has made no progress toward its waypoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    best_distance: Option<f32>,
    stalled_ticks: u32,
}

impl StallGuard {
    /// Records this tick's distance to the waypoint; true once `STALL_TICKS` ticks in a
    /// row have not got closer than the best so far.
    fn stalled(&mut self, distance: f32) -> bool {
        if self.best_distance.is_none_or(|best| distance < best) {
            self.best_distance = Some(distance);
            self.stalled_ticks = 0;
        } else {
            self.stalled_ticks += 1;
        }
        self.stalled_ticks >= STALL_TICKS
    }
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The claimed mark.
        mark: [i32; 3],
        /// Standable cells from the pawn's cell to the stand cell.
        route: Vec<BlockPos>,
        /// Index into `route` of the waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn is: feet position, eye position and facing (radians; forward is
/// `(sin yaw, 0, cos yaw)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub eye: Vec3,
    pub yaw: f32,
}

/// What the glue does for a pawn this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of the current waypoint cell at feet height.
    Walk {
        to: Vec3,
    },
    /// Dig `block`, aiming at `aim`, the centre of its exposed face nearest the eye.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of steering: turn this much, walk this fast (0 to 1 of full speed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances `pawn`'s job one tick: an idle pawn claims the nearest reachable unclaimed
/// mark and equips its tool; a mining pawn walks its route, then digs. `id` is the
/// identity written on the claim.
pub fn step(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(pawn.job, Job::Idle)
        && let Some(job) = claim_nearest(id, pose, marks, grid, &mut pawn.scan_cursor)
    {
        pawn.select_slot(TOOL_SLOT);
        pawn.job = job;
    }
    advance(&mut pawn.job, pose, marks, grid)
}

/// The faces of `block` open to air, as offsets from the block.
fn open_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    FACES
        .into_iter()
        .filter(move |&face| !grid.is_solid_at(block + face))
}

/// Claims the nearest unclaimed mark with an exposed face that routes to a stand cell.
///
/// Each call tries at most `MAX_ROUTE_ATTEMPTS` candidates, nearest first, starting at
/// `cursor` (an index into the distance-sorted list). A failed scan moves `cursor` past
/// the candidates tried, wrapping, so unreachable near marks cannot starve far reachable
/// ones; a claim resets it to the nearest.
fn claim_nearest(
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
    cursor: &mut usize,
) -> Option<Job> {
    let from = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(|[x, y, z]| BlockPos::new(x, y, z))
        .filter(|&block| open_faces(grid, block).next().is_some())
        .collect();
    candidates.sort_by_key(|&block| (block - from).length_squared());

    if candidates.is_empty() {
        *cursor = 0;
        return None;
    }
    let start = *cursor % candidates.len();
    let attempts = MAX_ROUTE_ATTEMPTS.min(candidates.len());
    *cursor = (start + attempts) % candidates.len();

    for offset in 0..attempts {
        let block = candidates[(start + offset) % candidates.len()];
        let is_stand_cell = |c: BlockPos| {
            (c.x - block.x).abs() <= 1
                && (c.z - block.z).abs() <= 1
                && (-1..=2).contains(&(c.y - block.y))
                && standable(grid, c)
        };
        let Some(route) = route(grid, from, is_stand_cell, block, ROUTE_NODE_BUDGET) else {
            continue;
        };
        let mark = block.to_array();
        if !marks.claim(mark, id) {
            continue;
        }
        *cursor = 0;
        return Some(Job::Mine {
            mark,
            route,
            next: 0,
            stall: StallGuard::default(),
        });
    }
    None
}

fn cell_centre(cell: BlockPos) -> Vec3 {
    cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5)
}

/// Releases the claim on `mark` and goes idle, leaving the mark on offer.
fn release_and_idle(job: &mut Job, mark: [i32; 3], marks: &mut MineMarks) {
    marks.release(mark);
    *job = Job::Idle;
}

/// One tick of a mining job: walk the route, then dig. An idle job waits.
fn advance(job: &mut Job, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
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
    if !marks.contains(mark) {
        release_and_idle(job, mark, marks);
        return JobAction::Wait;
    }

    while route.get(*next).is_some_and(|&waypoint| {
        cell_centre(waypoint).xz().distance(pose.feet.xz()) <= ARRIVE_RADIUS
    }) {
        *next += 1;
        *stall = StallGuard::default();
    }

    let Some(&waypoint) = route.get(*next) else {
        let block = BlockPos::from_array(mark);
        let aim = open_faces(grid, block)
            .map(|face| block.as_vec3() + Vec3::splat(0.5) + face.as_vec3() * 0.5)
            .min_by(|a, b| {
                a.distance_squared(pose.eye)
                    .total_cmp(&b.distance_squared(pose.eye))
            });
        release_and_idle(job, mark, marks);
        return aim.map_or(JobAction::Wait, |aim| JobAction::Dig { block, aim });
    };

    let to = cell_centre(waypoint);
    if !standable(grid, waypoint) || stall.stalled(to.distance(pose.feet)) {
        release_and_idle(job, mark, marks);
        return JobAction::Wait;
    }
    JobAction::Walk { to }
}

/// Turns toward `to` at no more than `TURN_RATE` per tick and walks at full speed once
/// within 60 degrees of facing it, else turns in place.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let offset = to - pose.feet;
    if offset.xz().length_squared() <= f32::EPSILON {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let bearing = offset.x.atan2(offset.z);
    let diff = (bearing - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: diff.clamp(-max_turn, max_turn),
        forward: if diff.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::routing::feet_cell;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const EYE_HEIGHT: f32 = 1.15;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn pose(feet: Vec3, yaw: f32) -> Pose {
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw,
        }
    }

    /// The pose of a pawn standing at the centre of a feet cell.
    fn standing_in(c: BlockPos) -> Pose {
        pose(c.as_vec3() + Vec3::new(0.5, 0.0, 0.5), 0.0)
    }

    /// A floor of solid blocks at y = 0 (feet cells at y = 1), 17 by 17.
    fn flat_world() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -8..=8 {
            for z in -8..=8 {
                grid.mutator().place(cell(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn world_with_marks(blocks: &[[i32; 3]]) -> (VoxelGrid, MineMarks) {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        for &b in blocks {
            grid.mutator().place(cell(b[0], b[1], b[2]), 1, Some(7));
            marks.toggle(b);
        }
        (grid, marks)
    }

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    /// Runs ticks, teleporting the pawn onto each walk target, until a dig. Returns every
    /// action taken and the pose at the dig.
    fn run_to_dig(
        pawn: &mut Pawn,
        start: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> (Vec<JobAction>, Pose) {
        let mut at = start;
        let mut actions = Vec::new();
        for _ in 0..64 {
            let action = step(PawnId(0), pawn, at, marks, grid);
            actions.push(action);
            match action {
                JobAction::Dig { .. } => return (actions, at),
                JobAction::Walk { to } => at = pose(to, at.yaw),
                JobAction::Wait => {}
            }
        }
        panic!("no dig within 64 ticks: {actions:?}");
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let (grid, mut marks) = world_with_marks(&[]);
        let mut pawn = Pawn::default();

        let action = step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        for x in 2..=4 {
            for y in 1..=2 {
                for z in -1..=1 {
                    grid.mutator().place(cell(x, y, z), 1, None);
                }
            }
        }
        grid.mutator().place(cell(1, 1, 0), 1, None);
        marks.toggle([3, 1, 0]);
        let mut pawn = Pawn::default();

        let action = step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(claimed_by(&marks, [3, 1, 0]), None);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[3, 1, 0]]);
    }

    #[test]
    fn a_walled_in_nearest_mark_is_skipped_for_a_reachable_farther_one() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 1, 0], [-6, 1, 0]]);
        for x in 2..=4 {
            for y in 1..=2 {
                for z in -1..=1 {
                    if (x, z) != (3, 0) {
                        grid.mutator().place(cell(x, y, z), 1, None);
                    }
                }
            }
        }
        let mut pawn = Pawn::default();

        step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(claimed_by(&marks, [-6, 1, 0]), Some(PawnId(0)));
        assert_eq!(claimed_by(&marks, [3, 1, 0]), None);
        assert!(matches!(
            pawn.job,
            Job::Mine {
                mark: [-6, 1, 0],
                ..
            }
        ));
    }

    #[test]
    fn a_reachable_mark_behind_many_unreachable_nearer_ones_is_claimed_within_ten_ticks() {
        let enclosed: Vec<[i32; 3]> = [-6, -3, 0, 3, 6].map(|z| [3, 1, z]).to_vec();
        let reachable = [-7, 1, 0];
        let mut all = enclosed.clone();
        all.push(reachable);
        let (mut grid, mut marks) = world_with_marks(&all);
        for &[cx, _, cz] in &enclosed {
            for x in cx - 1..=cx + 1 {
                for y in 1..=2 {
                    for z in cz - 1..=cz + 1 {
                        if (x, z) != (cx, cz) {
                            grid.mutator().place(cell(x, y, z), 1, None);
                        }
                    }
                }
            }
        }
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));

        let mut claimed_after = None;
        for tick in 1..=10 {
            step(PawnId(0), &mut pawn, start, &mut marks, &grid);
            if claimed_by(&marks, reachable) == Some(PawnId(0)) {
                claimed_after = Some(tick);
                break;
            }
        }

        assert!(
            claimed_after.is_some(),
            "the reachable mark starved behind five unreachable nearer ones"
        );
        for pos in enclosed {
            assert_eq!(claimed_by(&marks, pos), None);
        }
    }

    #[test]
    fn two_reachable_marks_are_dug_one_after_the_other_and_the_pawn_ends_idle() {
        let (grid, mut marks) = world_with_marks(&[[3, 1, 0], [-3, 1, 0]]);
        let mut pawn = Pawn::default();
        let mut at = standing_in(cell(0, 1, 0));
        let mut dug = Vec::new();

        for _ in 0..200 {
            match step(PawnId(0), &mut pawn, at, &mut marks, &grid) {
                JobAction::Dig { block, .. } => {
                    marks.block_removed(block.to_array());
                    dug.push(block);
                }
                JobAction::Walk { to } => at = pose(to, at.yaw),
                JobAction::Wait => {}
            }
            if dug.len() == 2 {
                break;
            }
        }

        assert_eq!(dug.len(), 2, "both marks are dug: {dug:?}");
        assert_ne!(dug[0], dug[1], "each mark is dug once");
        assert!(dug.contains(&cell(3, 1, 0)) && dug.contains(&cell(-3, 1, 0)));
        assert_eq!(marks.iter().count(), 0, "no mark or claim lingers");
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(
            step(PawnId(0), &mut pawn, at, &mut marks, &grid),
            JobAction::Wait
        );
    }

    #[test]
    fn a_mark_above_the_feet_is_dug_from_a_stand_cell_at_another_height() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 2, 0]]);
        grid.mutator().place(cell(3, 1, 0), 1, None);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));

        let (actions, at) = run_to_dig(&mut pawn, start, &mut marks, &grid);

        let Some(JobAction::Dig { block, aim }) = actions.last() else {
            panic!("the run ends in a dig");
        };
        assert_eq!(*block, cell(3, 2, 0));
        let stand = feet_cell(at.feet);
        assert_eq!(stand, cell(2, 1, 0), "the nearest standable cell beside it");
        assert_ne!(stand.y, block.y, "feet are not at the block's height");
        assert!(aim.distance(at.eye) <= crate::pawn::MINE_REACH);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let (grid, mut marks) = world_with_marks(&[[6, 1, 0], [3, 1, 0], [-2, 1, 6]]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(claimed_by(&marks, [3, 1, 0]), Some(PawnId(0)));
        assert_eq!(claimed_by(&marks, [6, 1, 0]), None);
        assert_eq!(claimed_by(&marks, [-2, 1, 6]), None);
        assert!(matches!(
            pawn.job,
            Job::Mine {
                mark: [3, 1, 0],
                ..
            }
        ));
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let (grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));

        let (actions, at) = run_to_dig(&mut pawn, start, &mut marks, &grid);

        let centres =
            [cell(1, 1, 0), cell(2, 1, 0)].map(|c| c.as_vec3() + Vec3::new(0.5, 0.0, 0.5));
        let walks: Vec<Vec3> = actions
            .iter()
            .filter_map(|a| match a {
                JobAction::Walk { to } => Some(*to),
                _ => None,
            })
            .collect();
        for centre in centres {
            assert!(walks.contains(&centre), "walks to {centre}: {walks:?}");
        }
        let stand = feet_cell(at.feet);
        assert_eq!(stand, cell(2, 1, 0), "digs from the nearest stand cell");
        let Some(JobAction::Dig { block, .. }) = actions.last() else {
            panic!("the run ends in a dig");
        };
        assert_eq!(*block, cell(3, 1, 0));
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let (grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(2, 1, 0));

        let (actions, _) = run_to_dig(&mut pawn, start, &mut marks, &grid);

        let Some(JobAction::Dig { aim, .. }) = actions.last() else {
            panic!("the run ends in a dig");
        };
        let west_face = Vec3::new(3.0, 1.5, 0.5);
        assert!(
            aim.distance(west_face) < 1e-4,
            "aims at the west face centre {west_face}, got {aim}"
        );
    }

    #[test]
    fn the_dig_skips_a_covered_face_for_the_nearest_exposed_one() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        for (x, y, z) in [(3, 2, 0), (2, 1, 0), (2, 1, 1)] {
            grid.mutator().place(cell(x, y, z), 1, None);
        }
        let mut pawn = Pawn::default();
        let start = standing_in(cell(2, 2, 1));

        let (actions, _) = run_to_dig(&mut pawn, start, &mut marks, &grid);

        let Some(JobAction::Dig { block, aim }) = actions.last() else {
            panic!("the run ends in a dig");
        };
        assert_eq!(*block, cell(3, 1, 0));
        let south_face = Vec3::new(3.5, 1.5, 1.0);
        assert!(
            aim.distance(south_face) < 1e-4,
            "aims at the exposed south face {south_face}, got {aim}"
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            block in (-5i32..=5, -5i32..=5),
            from in (-6i32..=6, -6i32..=6),
        ) {
            prop_assume!(block != from);
            let (grid, mut marks) = world_with_marks(&[[block.0, 1, block.1]]);
            let mut pawn = Pawn::default();
            let start = standing_in(cell(from.0, 1, from.1));

            let (actions, at) = run_to_dig(&mut pawn, start, &mut marks, &grid);

            let Some(JobAction::Dig { aim, .. }) = actions.last() else {
                panic!("the run ends in a dig");
            };
            prop_assert!(
                aim.distance(at.eye) <= crate::pawn::MINE_REACH,
                "aim {aim} is out of reach of eye {}", at.eye
            );
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let (grid, mut marks) = world_with_marks(&[[6, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));
        step(PawnId(0), &mut pawn, start, &mut marks, &grid);
        assert!(matches!(pawn.job, Job::Mine { .. }), "walking to the mark");

        marks.toggle([6, 1, 0]);
        let action = step(PawnId(0), &mut pawn, start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let (mut grid, mut marks) = world_with_marks(&[[6, 1, 0], [-6, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 1));
        step(PawnId(0), &mut pawn, start, &mut marks, &grid);
        let Job::Mine { mark, .. } = pawn.job.clone() else {
            panic!("a mark was claimed");
        };
        grid.mutator().remove(cell(mark[0], mark[1], mark[2]));
        marks.block_removed(mark);

        let action = step(PawnId(0), &mut pawn, start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        let remaining: Vec<[i32; 3]> = marks.unclaimed().collect();
        assert_eq!(remaining.len(), 1, "the other mark is still on offer");
        step(PawnId(0), &mut pawn, start, &mut marks, &grid);
        assert!(
            matches!(pawn.job, Job::Mine { .. }),
            "and the pawn takes it"
        );
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let (grid, mut marks) = world_with_marks(&[[3, 1, 0], [-6, 1, 0]]);
        let (mut first, mut second) = (Pawn::default(), Pawn::default());
        let at = standing_in(cell(0, 1, 0));
        step(PawnId(0), &mut first, at, &mut marks, &grid);

        step(PawnId(1), &mut second, at, &mut marks, &grid);

        assert_eq!(claimed_by(&marks, [3, 1, 0]), Some(PawnId(0)));
        assert_eq!(claimed_by(&marks, [-6, 1, 0]), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let (grid, mut marks) = world_with_marks(&[[6, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));

        let mut ticks = 0;
        while ticks <= STALL_TICKS + 5 {
            step(PawnId(0), &mut pawn, start, &mut marks, &grid);
            ticks += 1;
            if pawn.job == Job::Idle {
                break;
            }
        }

        assert_eq!(pawn.job, Job::Idle, "gave up after {ticks} ticks");
        assert!(ticks >= STALL_TICKS, "not before the stall limit: {ticks}");
        assert!(marks.contains([6, 1, 0]), "the mark stays");
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[6, 1, 0]]);
    }

    #[test]
    fn a_walk_that_makes_progress_never_stalls() {
        let (grid, mut marks) = world_with_marks(&[[7, 1, 0]]);
        let mut pawn = Pawn::default();
        let mut at = standing_in(cell(0, 1, 0));

        for tick in 0..(3 * STALL_TICKS) {
            let action = step(PawnId(0), &mut pawn, at, &mut marks, &grid);
            let JobAction::Walk { to } = action else {
                panic!("tick {tick}: expected a walk, got {action:?}");
            };
            let toward = (to - at.feet).normalize_or_zero() * 0.004;
            at = pose(at.feet + toward, at.yaw);
            assert!(
                matches!(pawn.job, Job::Mine { .. }),
                "tick {tick}: the claim was released"
            );
        }

        assert!(matches!(pawn.job, Job::Mine { .. }), "still walking");
        assert_eq!(claimed_by(&marks, [7, 1, 0]), Some(PawnId(0)));
    }

    #[test]
    fn a_long_straight_route_walked_steadily_never_stalls_across_waypoints() {
        // A power-of-two stride that divides the 1 m waypoint spacing exactly, so every
        // waypoint is approached through the same distances and a guard that keeps one
        // best distance for the whole route never sees a strictly closer tick.
        const SPEED: f32 = 0.0625;
        let mut grid = VoxelGrid::new(16);
        for x in -12..=12 {
            for z in -2..=2 {
                grid.mutator().place(cell(x, 0, z), 1, None);
            }
        }
        let mut marks = MineMarks::default();
        grid.mutator().place(cell(12, 1, 0), 1, Some(7));
        marks.toggle([12, 1, 0]);
        let mut pawn = Pawn::default();
        let mut at = standing_in(cell(-12, 1, 0));

        let mut ticks = 0u32;
        let dug = loop {
            let action = step(PawnId(0), &mut pawn, at, &mut marks, &grid);
            ticks += 1;
            match action {
                JobAction::Walk { to } => {
                    let offset = to - at.feet;
                    let stride = offset.length().min(SPEED);
                    at = pose(at.feet + offset.normalize_or_zero() * stride, at.yaw);
                    assert!(
                        matches!(pawn.job, Job::Mine { .. }),
                        "tick {ticks}: walking but the job is {:?}",
                        pawn.job
                    );
                    assert_eq!(
                        claimed_by(&marks, [12, 1, 0]),
                        Some(PawnId(0)),
                        "tick {ticks}: the claim was lost mid-walk"
                    );
                }
                JobAction::Dig { block, .. } => break block,
                JobAction::Wait => panic!("tick {ticks}: waited mid-walk, job {:?}", pawn.job),
            }
            assert!(ticks < 2_000, "never reached the mark");
        };

        assert_eq!(dug, cell(12, 1, 0));
        assert!(
            ticks > STALL_TICKS,
            "the walk must outlast the stall limit to mean anything: {ticks}"
        );
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));
        let mut waypoint = None;
        for _ in 0..4 {
            match step(PawnId(0), &mut pawn, start, &mut marks, &grid) {
                JobAction::Walk { to } if feet_cell(to) != cell(0, 1, 0) => {
                    waypoint = Some(feet_cell(to));
                    break;
                }
                JobAction::Walk { .. } | JobAction::Wait => {}
                other @ JobAction::Dig { .. } => panic!("expected a walk, got {other:?}"),
            }
        }
        let waypoint = waypoint.expect("a walk to a waypoint beyond the start within 4 ticks");
        grid.mutator()
            .place(cell(waypoint.x, waypoint.y + 2, waypoint.z), 1, None);

        let action = step(PawnId(0), &mut pawn, start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[3, 1, 0]]);
    }

    #[test]
    fn a_stall_guard_never_trips_while_each_tick_gets_closer() {
        let mut guard = StallGuard::default();

        let tripped = (0..3 * STALL_TICKS).any(|i| guard.stalled(1000.0 - i as f32));

        assert!(!tripped);
    }

    #[test]
    fn a_stall_guard_trips_after_stall_ticks_without_getting_closer() {
        let mut guard = StallGuard::default();
        assert!(!guard.stalled(5.0), "the first distance is the best so far");

        let mut results = Vec::new();
        for i in 0..STALL_TICKS {
            let distance = if i % 2 == 0 { 5.0 } else { 6.0 };
            results.push(guard.stalled(distance));
        }

        let (last, earlier) = results.split_last().expect("STALL_TICKS is not zero");
        assert!(earlier.iter().all(|stalled| !stalled));
        assert!(last, "tripped on the {STALL_TICKS}th tick without progress");
    }

    #[test]
    fn a_closer_tick_resets_the_stall_guard_count() {
        let mut guard = StallGuard::default();
        guard.stalled(5.0);
        for _ in 0..STALL_TICKS - 1 {
            assert!(!guard.stalled(5.0));
        }

        let closer = guard.stalled(4.0);
        let after: Vec<bool> = (0..STALL_TICKS - 1).map(|_| guard.stalled(4.0)).collect();

        assert!(!closer);
        assert!(after.iter().all(|stalled| !stalled));
        assert!(guard.stalled(4.0));
    }

    #[test]
    fn the_nearest_mark_is_judged_from_the_pawns_own_cell_not_the_origin() {
        let (grid, mut marks) = world_with_marks(&[[5, 1, 0], [-2, 1, 0]]);
        let mut pawn = Pawn::default();

        let action = step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(4, 1, 0)),
            &mut marks,
            &grid,
        );

        assert!(
            matches!(action, JobAction::Dig { block, .. } if block == cell(5, 1, 0)),
            "{action:?}"
        );
        assert_eq!(claimed_by(&marks, [-2, 1, 0]), None);
    }

    /// Marks each walled in by solid blocks on every side, so no route reaches a stand cell.
    fn enclosed_world(marks: &[[i32; 3]]) -> (VoxelGrid, MineMarks) {
        let (mut grid, mine_marks) = world_with_marks(marks);
        for &[cx, _, cz] in marks {
            for x in cx - 1..=cx + 1 {
                for y in 1..=2 {
                    for z in cz - 1..=cz + 1 {
                        if (x, z) != (cx, cz) {
                            grid.mutator().place(cell(x, y, z), 1, None);
                        }
                    }
                }
            }
        }
        (grid, mine_marks)
    }

    #[test]
    fn failed_scans_advance_the_cursor_by_the_attempts_made_and_wrap_at_the_list_length() {
        let blocks = [
            [3, 1, -6],
            [3, 1, -3],
            [3, 1, 0],
            [3, 1, 3],
            [3, 1, 6],
            [6, 1, 0],
        ];
        let (grid, mut marks) = enclosed_world(&blocks);
        let mut pawn = Pawn::default();
        let at = standing_in(cell(0, 1, 0));

        let mut cursors = Vec::new();
        for _ in 0..3 {
            step(PawnId(0), &mut pawn, at, &mut marks, &grid);
            cursors.push(pawn.scan_cursor);
        }

        assert_eq!(cursors, [4, 2, 0]);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_stand_cell_two_above_the_marks_height_is_accepted() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        for x in 2..=4 {
            for z in -1..=1 {
                if (x, z) != (3, 0) {
                    for y in 1..=2 {
                        grid.mutator().place(cell(x, y, z), 1, None);
                    }
                }
            }
        }
        grid.mutator().place(cell(3, 3, 0), 1, None);
        grid.mutator().place(cell(1, 1, 0), 1, None);
        let mut pawn = Pawn::default();

        step(
            PawnId(0),
            &mut pawn,
            standing_in(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        let Job::Mine { mark, route, .. } = &pawn.job else {
            panic!(
                "the ring top two above the mark is a stand cell: {:?}",
                pawn.job
            );
        };
        assert_eq!(*mark, [3, 1, 0]);
        let stand = route.last().expect("a route has cells");
        assert_eq!(stand.y - 1, 2, "feet two above the mark");
        assert!((stand.x - 3).abs() <= 1 && stand.z.abs() <= 1);
    }

    #[test]
    fn stepping_from_the_route_start_cell_moves_on_to_the_next_waypoint() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
            let mut pawn = Pawn::default();
            let action = step(
                PawnId(0),
                &mut pawn,
                standing_in(cell(0, 1, 0)),
                &mut marks,
                &grid,
            );
            tx.send(action).expect("the test waits for the step");
        });

        let action = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the step returned instead of looping on the start cell");

        assert_eq!(
            action,
            JobAction::Walk {
                to: Vec3::new(1.5, 1.0, 0.5)
            }
        );
    }

    #[test]
    fn steer_measures_the_target_from_the_pawns_feet() {
        let feet = Vec3::new(10.0, 0.0, -5.0);
        let at = pose(feet, 0.0);

        let right = steer(at, feet + facing_target(std::f32::consts::FRAC_PI_2));
        let left = steer(at, feet + facing_target(-std::f32::consts::FRAC_PI_2));

        assert!(
            (right.yaw_delta - max_turn_per_tick()).abs() < 1e-5,
            "{right:?}"
        );
        assert!(
            (left.yaw_delta + max_turn_per_tick()).abs() < 1e-5,
            "{left:?}"
        );
    }

    #[test]
    fn steer_takes_the_short_way_across_the_half_turn_and_from_yaws_beyond_it() {
        let cases = [
            (3.1_f32, -3.1_f32),
            (-3.1, 3.1),
            (4.0, 0.5),
            (-4.0, -0.2),
            (7.0, 0.5),
        ];

        for (yaw, bearing) in cases {
            let walk = steer(pose(Vec3::ZERO, yaw), facing_target(bearing));

            let expected = wrap(bearing - yaw).clamp(-max_turn_per_tick(), max_turn_per_tick());
            assert!(
                (walk.yaw_delta - expected).abs() < 1e-4,
                "yaw {yaw}, bearing {bearing}: {walk:?} vs {expected}"
            );
        }
    }

    fn facing_target(angle: f32) -> Vec3 {
        Vec3::new(angle.sin(), 0.0, angle.cos()) * 3.0
    }

    fn wrap(angle: f32) -> f32 {
        let tau = std::f32::consts::TAU;
        (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
    }

    fn max_turn_per_tick() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let at = pose(Vec3::ZERO, 0.0);

        let right = steer(at, facing_target(std::f32::consts::FRAC_PI_2));
        let left = steer(at, facing_target(-std::f32::consts::FRAC_PI_2));
        let nearly_ahead = steer(at, facing_target(0.01));

        assert!(
            (right.yaw_delta - max_turn_per_tick()).abs() < 1e-5,
            "{right:?}"
        );
        assert!(
            (left.yaw_delta + max_turn_per_tick()).abs() < 1e-5,
            "{left:?}"
        );
        assert!(
            (nearly_ahead.yaw_delta - 0.01).abs() < 1e-4,
            "{nearly_ahead:?}"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let at = pose(Vec3::ZERO, 0.0);

        let ahead = steer(at, facing_target(0.0));
        let inside_cone = steer(at, facing_target(59.0_f32.to_radians()));
        let outside_cone = steer(at, facing_target(61.0_f32.to_radians()));
        let behind = steer(at, facing_target(std::f32::consts::PI));

        assert!((ahead.forward - 1.0).abs() < 1e-6);
        assert!((inside_cone.forward - 1.0).abs() < 1e-6);
        assert!(outside_cone.forward.abs() < 1e-6, "turns in place");
        assert!(behind.forward.abs() < 1e-6, "turns in place");
    }

    proptest! {
        #[test]
        fn steer_takes_the_short_way_round_within_the_turn_rate(
            yaw in -3.0f32..3.0,
            bearing in -3.0f32..3.0,
        ) {
            let diff = wrap(bearing - yaw);
            prop_assume!(diff.abs() > 1e-3 && diff.abs() < 3.1);
            let at = pose(Vec3::ZERO, yaw);

            let walk = steer(at, facing_target(bearing));

            let expected = diff.abs().min(max_turn_per_tick());
            prop_assert!((walk.yaw_delta.abs() - expected).abs() < 1e-4, "{walk:?} vs {expected}");
            prop_assert_eq!(walk.yaw_delta.signum(), diff.signum());
        }
    }
}
