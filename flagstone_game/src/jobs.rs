//! What an idle pawn does on its own: claim a marked block, walk to it, dig it.
//!
//! The loop reads the marks, the grid and the pawn's pose and answers with one action per
//! tick; the caller applies the action through the pawn's own movement and mining, so the AI
//! has no powers a possessed pawn lacks.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use std::f32::consts::{FRAC_PI_3, PI, TAU};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};
use crate::selection::PAWN_HALF_EXTENTS;

/// How near the feet must come to a waypoint's centre, horizontally, to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks a pawn may go without closing on its waypoint before it gives its mark up.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second.
pub const TURN_RATE: f32 = TAU;

/// How far off its heading a pawn may be and still walk, in radians.
const WALK_ARC: f32 = FRAC_PI_3;

/// How far above the feet the capsule centre, where the eye is, sits.
pub const EYE_HEIGHT: f32 = PAWN_HALF_EXTENTS.y;

/// Marks an idle pawn tries to route to in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn is: its feet and which way it faces (yaw 0 faces +Z, positive turns toward +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Progress bookkeeping for a walk, so a pawn that is stuck gives up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StallGuard {
    /// The closest the feet have been to the current waypoint.
    pub closest: f32,
    /// Ticks since the feet last came closer.
    pub ticks_without_progress: u32,
}

impl Default for StallGuard {
    /// A guard that has seen no progress yet and counts no idle ticks.
    fn default() -> Self {
        Self {
            closest: f32::INFINITY,
            ticks_without_progress: 0,
        }
    }
}

/// A pawn's current job.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walk `route` to a cell the block can be dug from, then dig `mark`.
    Mine {
        mark: BlockPos,
        route: Vec<BlockPos>,
        /// The waypoint being walked to; `route.len()` once all are reached.
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward the centre of a waypoint cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming from the eye at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: how far to turn and how much of full speed to walk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to turn this tick; positive turns toward +X.
    pub yaw_delta: f32,
    /// Fraction of full walking speed, `0.0..=1.0`.
    pub forward: f32,
}

/// Whether a pawn with its feet in `cell` can dig `block`: within one column of it in x and
/// z, and with the feet from one below to two above it. Every such cell is inside the mining
/// reach.
pub fn within_stand_window(block: BlockPos, cell: BlockPos) -> bool {
    let offset = cell - block;
    offset.x.abs() <= 1 && offset.z.abs() <= 1 && (-1..=2).contains(&offset.y)
}

/// Whether a pawn standing in `cell` should dig `block`: inside the window, but not on the
/// block itself, where digging would take the ground from under its feet.
fn can_dig_from(block: BlockPos, cell: BlockPos) -> bool {
    within_stand_window(block, cell) && cell != block + BlockPos::Y
}

/// The six faces of a block: the neighbour a face looks onto.
const FACES: [BlockPos; 6] = [
    BlockPos::new(1, 0, 0),
    BlockPos::new(-1, 0, 0),
    BlockPos::new(0, 1, 0),
    BlockPos::new(0, -1, 0),
    BlockPos::new(0, 0, 1),
    BlockPos::new(0, 0, -1),
];

/// Whether `block` exists and has an air neighbour to dig it from.
fn is_exposed(grid: &VoxelGrid, block: BlockPos) -> bool {
    grid.is_solid_at(block) && FACES.iter().any(|&face| !grid.is_solid_at(block + face))
}

/// The horizontal distance from `feet` to the centre of `cell`.
fn horizontal_gap(feet: Vec3, cell: BlockPos) -> f32 {
    let centre = waypoint_centre(cell);
    (feet.x - centre.x).hypot(feet.z - centre.z)
}

/// Where a pawn standing in `cell` has its feet: the cell's middle, on its floor.
fn waypoint_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

/// Advances the job of pawn `id` by one tick.
///
/// An idle pawn claims the nearest marked block it can route to, equips its tool and starts
/// walking. A mining pawn walks its route and then digs. A pawn whose mark is gone, whose
/// waypoint can no longer be stood on, or that stops making progress, drops its job; the
/// mark stays for others unless it is gone.
///
/// A returned [`JobAction::Dig`] ends the job: the claim is released and the pawn is idle again,
/// so a dig that removes some other block leaves the mark for the next claim.
pub fn step(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if pawn.job == Job::Idle && !claim_nearest(id, pawn, pose, marks, grid) {
        return JobAction::Wait;
    }
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = &mut pawn.job
    else {
        return JobAction::Wait;
    };
    let pos = mark.to_array();

    let held = marks.claimed_by(pos) == Some(id);
    let waypoints_stand = route[*next..].iter().all(|&cell| standable(grid, cell));
    if !held || !waypoints_stand {
        return drop_job(pawn, marks, pos);
    }

    let arrived = route[*next..]
        .iter()
        .take_while(|&&waypoint| horizontal_gap(pose.feet, waypoint) <= ARRIVE_RADIUS)
        .count();
    if arrived > 0 {
        *next += arrived;
        *stall = StallGuard::default();
    }
    let Some(&waypoint) = route.get(*next) else {
        return dig(pawn, marks, pos, pose, grid);
    };
    let gap = horizontal_gap(pose.feet, waypoint);
    if gap < stall.closest {
        stall.closest = gap;
        stall.ticks_without_progress = 0;
    } else {
        stall.ticks_without_progress += 1;
    }
    if stall.ticks_without_progress >= STALL_TICKS {
        return drop_job(pawn, marks, pos);
    }
    JobAction::Walk {
        to: waypoint_centre(waypoint),
    }
}

/// Gives the job up and frees the claim; the mark stays for whoever claims it next.
fn drop_job(pawn: &mut Pawn, marks: &mut MineMarks, pos: [i32; 3]) -> JobAction {
    pawn.job = Job::Idle;
    marks.release(pos);
    JobAction::Wait
}

/// Ends the job with a dig at the exposed face of `pos` nearest the eye, or gives it up if the
/// block has since been buried or is gone.
fn dig(
    pawn: &mut Pawn,
    marks: &mut MineMarks,
    pos: [i32; 3],
    pose: Pose,
    grid: &VoxelGrid,
) -> JobAction {
    let block = BlockPos::from_array(pos);
    if !can_dig_from(block, feet_cell(pose.feet)) {
        return drop_job(pawn, marks, pos);
    }
    let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
    let aim = grid
        .is_solid_at(block)
        .then(|| {
            FACES
                .iter()
                .filter(|&&face| !grid.is_solid_at(block + face))
                .map(|&face| block.as_vec3() + Vec3::splat(0.5) + face.as_vec3() * 0.5)
                .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
        })
        .flatten();
    drop_job(pawn, marks, pos);
    aim.map_or(JobAction::Wait, |aim| JobAction::Dig { block, aim })
}

/// Starts a job on the nearest marked block with an exposed face that a route leads to,
/// trying at most [`ROUTE_ATTEMPTS_PER_TICK`] marks. Returns whether it did.
fn claim_nearest(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> bool {
    let feet = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .into_iter()
        .map(BlockPos::from_array)
        .filter(|&block| is_exposed(grid, block))
        .collect();
    // Stable, so marks equally near stay in key order.
    candidates.sort_by_key(|&block| (block - feet).length_squared());
    for block in candidates.into_iter().take(ROUTE_ATTEMPTS_PER_TICK) {
        let found = route(
            grid,
            feet,
            |cell| can_dig_from(block, cell),
            block,
            ROUTE_NODE_BUDGET,
        );
        if let Some(route) = found
            && marks.claim(block.to_array(), id)
        {
            pawn.select_slot(TOOL_SLOT);
            pawn.job = Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            };
            return true;
        }
    }
    false
}

/// Turns toward `to` at [`TURN_RATE`] and walks once within 60 degrees of facing it.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let gap = to - pose.feet;
    if gap.x.hypot(gap.z) < f32::EPSILON {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let wanted = gap.x.atan2(gap.z) - pose.yaw;
    let turn = (wanted + PI).rem_euclid(TAU) - PI;
    let most = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: turn.clamp(-most, most),
        forward: if turn.abs() <= WALK_ARC { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::MINE_REACH;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const PAWN: PawnId = PawnId(0);
    const NEAR: [i32; 3] = [8, 1, 5];
    const FAR: [i32; 3] = [13, 1, 5];

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn centre(c: BlockPos) -> Vec3 {
        Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5)
    }

    fn standing_in(c: BlockPos) -> Pose {
        Pose {
            feet: centre(c),
            yaw: 0.0,
        }
    }

    /// A floor one block thick at y 0 over x and z `0..=15`.
    fn floor() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(15, 0, 15), 1);
        grid
    }

    /// A floor with `blocks` standing on it, each one marked.
    fn marked(blocks: &[[i32; 3]]) -> (VoxelGrid, MineMarks) {
        let mut grid = floor();
        let mut marks = MineMarks::default();
        for &[x, y, z] in blocks {
            grid.mutator().place(cell(x, y, z), 1, Some(7));
            marks.toggle([x, y, z]);
        }
        (grid, marks)
    }

    fn holder(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn tick(pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
        step(PAWN, pawn, pose, marks, grid)
    }

    fn assert_walks_to(action: JobAction, waypoint: BlockPos) {
        let JobAction::Walk { to } = action else {
            panic!("expected a walk to {waypoint}, got {action:?}");
        };
        assert!(
            (to.x - (waypoint.x as f32 + 0.5)).abs() < 1e-4
                && (to.z - (waypoint.z as f32 + 0.5)).abs() < 1e-4,
            "walks to {to}, not the centre of {waypoint}"
        );
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = floor();
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        let action = tick(&mut pawn, standing_in(cell(4, 1, 5)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(pawn.selected_slot, 3, "an idle pawn keeps what it holds");
        assert_eq!(pawn.equipped_tool, None);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(15, 2, 15), 1);
        grid.mutator().place(cell(12, 3, 5), 1, None);
        let buried = [5, 1, 5];
        let open = [12, 3, 5];
        let pose = standing_in(cell(4, 3, 5));

        let mut only_buried = MineMarks::default();
        only_buried.toggle(buried);
        let mut pawn = Pawn::default();
        let alone = tick(&mut pawn, pose, &mut only_buried, &grid);
        let mut both = MineMarks::default();
        both.toggle(buried);
        both.toggle(open);
        let mut other = Pawn::default();
        let with_choice = tick(&mut other, pose, &mut both, &grid);

        assert_eq!(alone, JobAction::Wait, "nothing to dig from outside");
        assert_eq!(holder(&only_buried, buried), None);
        assert_eq!(pawn.job, Job::Idle);
        assert!(
            matches!(with_choice, JobAction::Walk { .. }),
            "{with_choice:?}"
        );
        assert_eq!(holder(&both, buried), None, "the buried mark is skipped");
        assert_eq!(holder(&both, open), Some(PAWN), "for the open one");
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let (grid, mut marks) = marked(&[FAR, NEAR]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        // Nearer to the pawn is the mark that comes later in key order.
        let action = tick(&mut pawn, standing_in(cell(15, 1, 5)), &mut marks, &grid);

        assert_walks_to(action, cell(14, 1, 5));
        assert_eq!(holder(&marks, FAR), Some(PAWN));
        assert_eq!(holder(&marks, NEAR), None, "one job, one mark");
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, crate::pawn::TOOL_SLOT);
        let Job::Mine {
            mark, route, next, ..
        } = &pawn.job
        else {
            panic!("the pawn took a job: {:?}", pawn.job);
        };
        assert_eq!(*mark, cell(FAR[0], FAR[1], FAR[2]));
        assert_eq!(*route, [cell(14, 1, 5)]);
        assert_eq!(*next, 0);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let (grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();
        let mut pose = standing_in(cell(4, 1, 5));
        let mut actions = Vec::new();

        for _ in 0..10 {
            let action = tick(&mut pawn, pose, &mut marks, &grid);
            actions.push(action);
            match action {
                JobAction::Walk { to } => pose.feet = Vec3::new(to.x, 1.0, to.z),
                JobAction::Dig { .. } | JobAction::Wait => break,
            }
        }

        assert_eq!(actions.len(), 4, "three waypoints and the dig: {actions:?}");
        for (action, x) in actions.iter().zip(5..=7) {
            assert_walks_to(*action, cell(x, 1, 5));
        }
        let JobAction::Dig { block, .. } = actions[3] else {
            panic!("the last action digs: {:?}", actions[3]);
        };
        assert_eq!(block, cell(NEAR[0], NEAR[1], NEAR[2]));
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let (mut grid, mut marks) = marked(&[NEAR]);
        // Covers the face of the marked block that the eye is closest to.
        grid.mutator().place(cell(7, 1, 5), 1, None);
        let pose = Pose {
            feet: Vec3::new(7.5, 1.0, 4.9),
            yaw: 0.0,
        };
        let mut pawn = Pawn::default();

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        let JobAction::Dig { block, aim } = action else {
            panic!("the pawn already stands where it can dig: {action:?}");
        };
        assert_eq!(block, cell(NEAR[0], NEAR[1], NEAR[2]));
        let top_face = Vec3::new(8.5, 2.0, 5.5);
        assert!(
            aim.distance(top_face) < 1e-4,
            "aims at {aim}, not the centre of the top face {top_face}"
        );
    }

    #[test]
    fn the_stand_window_is_one_column_around_and_one_below_to_two_above() {
        let block = cell(10, 20, -4);
        let inside = [
            cell(10, 20, -4),
            cell(9, 19, -5),
            cell(11, 22, -3),
            cell(10, 21, -3),
        ];
        let outside = [
            cell(12, 20, -4),
            cell(8, 20, -4),
            cell(10, 20, -2),
            cell(10, 20, -6),
            cell(10, 18, -4),
            cell(10, 23, -4),
        ];

        for c in inside {
            assert!(within_stand_window(block, c), "{c} can dig {block}");
        }
        for c in outside {
            assert!(!within_stand_window(block, c), "{c} cannot dig {block}");
        }
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            x in 4.0f32..13.0,
            z in 1.0f32..10.0,
        ) {
            let (grid, mut marks) = marked(&[NEAR]);
            let block = cell(NEAR[0], NEAR[1], NEAR[2]);
            let pose = Pose { feet: Vec3::new(x, 1.0, z), yaw: 0.0 };
            let mut pawn = Pawn::default();

            let action = tick(&mut pawn, pose, &mut marks, &grid);

            let feet = cell(x.floor() as i32, 1, z.floor() as i32);
            if let JobAction::Dig { .. } = action {
                let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
                prop_assert!(
                    within_stand_window(block, feet),
                    "dug from {feet}, outside the window of {block}"
                );
                prop_assert!(
                    eye.distance(block.as_vec3() + Vec3::splat(0.5)) <= MINE_REACH,
                    "dug from {eye}, beyond the reach of {block}"
                );
            }
            if within_stand_window(block, feet) && feet != block {
                prop_assert!(
                    matches!(action, JobAction::Dig { .. }),
                    "standing at {feet} the pawn does not dig: {action:?}"
                );
            }
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let (grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();
        let pose = standing_in(cell(4, 1, 5));
        let walking = tick(&mut pawn, pose, &mut marks, &grid);
        marks.toggle(NEAR);

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        assert!(matches!(walking, JobAction::Walk { .. }), "{walking:?}");
        assert_eq!(action, JobAction::Wait, "nothing left to walk to");
        assert_eq!(pawn.job, Job::Idle);
        assert!(!marks.contains(NEAR), "cancelled marks stay cancelled");
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let (mut grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();
        let pose = standing_in(cell(4, 1, 5));
        tick(&mut pawn, pose, &mut marks, &grid);
        grid.mutator().remove(cell(NEAR[0], NEAR[1], NEAR[2]));
        marks.block_removed(NEAR);

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_nearer_mark_with_no_route_is_passed_over_for_a_farther_open_one() {
        let enclosed = [6, 1, 5];
        let (mut grid, mut marks) = marked(&[enclosed, FAR]);
        // Two-high walls round the nearer mark: it has exposed faces but nowhere to dig from.
        for x in 3..=9 {
            for z in 2..=8 {
                if x == 3 || x == 9 || z == 2 || z == 8 {
                    for y in 1..=2 {
                        grid.mutator().place(cell(x, y, z), 1, None);
                    }
                }
            }
        }
        let mut pawn = Pawn::default();

        let action = tick(&mut pawn, standing_in(cell(1, 1, 5)), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(holder(&marks, enclosed), None, "no way to it");
        assert_eq!(holder(&marks, FAR), Some(PAWN), "so the open one is taken");
    }

    #[test]
    fn a_pawn_standing_on_its_mark_digs_from_a_neighbouring_cell() {
        let (grid, mut marks) = marked(&[[5, 1, 5]]);
        let mut pawn = Pawn::default();
        let mut pose = standing_in(cell(5, 2, 5));
        let mut dug_from = None;

        for _ in 0..10 {
            let action = tick(&mut pawn, pose, &mut marks, &grid);
            match action {
                JobAction::Walk { to } => pose.feet = to,
                JobAction::Dig { .. } => {
                    dug_from = Some(feet_cell(pose.feet));
                    break;
                }
                JobAction::Wait => panic!("the pawn gave up"),
            }
        }

        let from = dug_from.expect("the pawn digs after stepping off its mark");
        assert_ne!(from, cell(5, 2, 5), "digging the ground under its own feet");
        assert!(within_stand_window(cell(5, 1, 5), from), "{from}");
    }

    #[test]
    fn a_dig_ends_the_job_and_leaves_the_mark_marked_and_free() {
        let (mut grid, mut marks) = marked(&[NEAR]);
        // Where the pawn digs from, so the dig is the first action.
        grid.mutator().place(cell(7, 1, 4), 1, None);
        let mut pawn = Pawn::default();

        let action = tick(&mut pawn, standing_in(cell(7, 2, 4)), &mut marks, &grid);

        assert!(matches!(action, JobAction::Dig { .. }), "{action:?}");
        assert_eq!(pawn.job, Job::Idle, "the job ends on any dig");
        assert!(
            marks.contains(NEAR),
            "the mark stays until its block is gone"
        );
        assert_eq!(holder(&marks, NEAR), None, "free to be claimed again");
    }

    #[test]
    fn a_pawn_and_a_mark_at_negative_coordinates_find_each_other() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(-8, 0, -8), cell(7, 0, 7), 1);
        let block = [-6, 1, -1];
        grid.mutator().place(cell(-6, 1, -1), 1, Some(7));
        let mut marks = MineMarks::default();
        marks.toggle(block);
        let mut pawn = Pawn::default();
        let pose = Pose {
            feet: Vec3::new(-3.2, 1.0, -0.4),
            yaw: 0.0,
        };

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        assert_walks_to(action, cell(-5, 1, -1));
        assert_eq!(holder(&marks, block), Some(PAWN));
        let Job::Mine { route, .. } = &pawn.job else {
            panic!("the pawn took a job: {:?}", pawn.job);
        };
        assert_eq!(*route, [cell(-5, 1, -1)]);
    }

    #[test]
    fn of_two_equally_near_marks_the_lower_key_wins_every_time() {
        let low = [6, 1, 5];
        let high = [10, 1, 5];

        for _ in 0..100 {
            let (grid, mut marks) = marked(&[high, low]);
            let mut pawn = Pawn::default();

            tick(&mut pawn, standing_in(cell(8, 1, 5)), &mut marks, &grid);

            assert_eq!(holder(&marks, low), Some(PAWN));
            assert_eq!(holder(&marks, high), None);
        }
    }

    #[test]
    fn a_block_open_on_one_face_only_is_exposed_whichever_face_it_is() {
        let block = cell(8, 4, 5);
        let buried = || {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(block, 1, None);
            for face in FACES {
                grid.mutator().place(block + face, 1, None);
            }
            grid
        };
        assert!(!is_exposed(&buried(), block), "setup: nothing is open");

        for face in FACES {
            let mut grid = buried();
            grid.mutator().remove(block + face);

            assert!(is_exposed(&grid, block), "open only toward {face}");
        }
    }

    #[test]
    fn the_dig_aims_at_the_nearest_face_from_every_side() {
        let block = cell(8, 4, 5);
        let cases = [
            (
                "+x",
                cell(9, 4, 5),
                Vec3::new(9.5, 4.0, 5.5),
                Vec3::new(9.0, 4.5, 5.5),
            ),
            (
                "-x",
                cell(7, 4, 5),
                Vec3::new(7.5, 4.0, 5.5),
                Vec3::new(8.0, 4.5, 5.5),
            ),
            (
                "+z",
                cell(8, 4, 6),
                Vec3::new(8.5, 4.0, 6.5),
                Vec3::new(8.5, 4.5, 6.0),
            ),
            (
                "-z",
                cell(8, 4, 4),
                Vec3::new(8.5, 4.0, 4.5),
                Vec3::new(8.5, 4.5, 5.0),
            ),
            (
                "+y",
                cell(9, 5, 6),
                Vec3::new(9.5, 5.0, 6.5),
                Vec3::new(8.5, 5.0, 5.5),
            ),
            (
                "-y",
                cell(8, 3, 5),
                Vec3::new(8.5, 3.0, 5.5),
                Vec3::new(8.5, 4.0, 5.5),
            ),
        ];

        for (side, stand, feet, expected) in cases {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(block, 1, None);
            let mut marks = MineMarks::default();
            marks.toggle(block.to_array());
            let mut pawn = Pawn::default();
            let pose = Pose { feet, yaw: 0.0 };
            assert_eq!(feet_cell(feet), stand, "setup: {side}");

            let action = dig(&mut pawn, &mut marks, block.to_array(), pose, &grid);

            let JobAction::Dig { aim, .. } = action else {
                panic!("{side}: {action:?}");
            };
            assert!(
                aim.distance(expected) < 1e-4,
                "{side}: aims at {aim}, not {expected}"
            );
        }
    }

    #[test]
    fn a_waypoint_exactly_at_the_arrival_radius_is_reached() {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(-8, 0, 0), cell(7, 0, 10), 1);
        let block = cell(-4, 1, 5);
        grid.mutator().place(block, 1, Some(7));
        let mut marks = MineMarks::default();
        marks.toggle(block.to_array());
        marks.claim(block.to_array(), PAWN);
        let mut pawn = Pawn {
            job: Job::Mine {
                mark: block,
                route: vec![cell(-1, 1, 5), cell(-2, 1, 5), cell(-3, 1, 5)],
                next: 0,
                stall: StallGuard::default(),
            },
            ..Pawn::default()
        };
        // Exactly ARRIVE_RADIUS from the first waypoint's centre at x -0.5.
        let pose = Pose {
            feet: Vec3::new(-0.5 + ARRIVE_RADIUS, 1.0, 5.5),
            yaw: 0.0,
        };
        assert_eq!(horizontal_gap(pose.feet, cell(-1, 1, 5)), ARRIVE_RADIUS);

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        assert_walks_to(action, cell(-2, 1, 5));
    }

    #[test]
    fn steer_stands_still_only_when_the_target_is_at_the_feet() {
        let pose = Pose {
            feet: Vec3::ZERO,
            yaw: 0.0,
        };
        let cases = [
            (0.0, false),
            (f32::EPSILON / 2.0, false),
            (f32::EPSILON, true),
            (0.01, true),
        ];

        for (distance, turns) in cases {
            let walk = steer(pose, Vec3::new(distance, 0.0, 0.0));

            assert_eq!(
                walk.yaw_delta != 0.0,
                turns,
                "target {distance} m away: {walk:?}"
            );
        }
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let (grid, mut marks) = marked(&[NEAR]);
        let mut first = Pawn::default();
        let mut second = Pawn::default();
        let pose = standing_in(cell(4, 1, 5));
        tick(&mut first, pose, &mut marks, &grid);

        let action = step(PawnId(1), &mut second, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait, "the only mark is taken");
        assert_eq!(second.job, Job::Idle);
        assert_eq!(holder(&marks, NEAR), Some(PAWN), "the first pawn keeps it");
        assert!(matches!(first.job, Job::Mine { .. }));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let (grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();
        let pose = standing_in(cell(4, 1, 5));
        let mut released_at = None;

        for ticks in 1..=STALL_TICKS + 5 {
            let action = tick(&mut pawn, pose, &mut marks, &grid);
            if pawn.job == Job::Idle {
                assert_eq!(action, JobAction::Wait, "the tick that gives up");
                released_at = Some(ticks);
                break;
            }
            assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        }

        let released_at = released_at.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS..=STALL_TICKS + 3).contains(&released_at),
            "gave up after {released_at} ticks"
        );
        assert!(marks.contains(NEAR), "the mark is still there");
        assert_eq!(holder(&marks, NEAR), None, "and free for another pawn");
    }

    #[test]
    fn a_walk_that_makes_progress_is_not_a_stall() {
        let (grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();

        // The first waypoint is a metre away; each 0.2 m closer is progress, short of arriving.
        for x in [4.5, 4.7, 4.9, 5.1] {
            let pose = Pose {
                feet: Vec3::new(x, 1.0, 5.5),
                yaw: 0.0,
            };
            for ticks in 0..STALL_TICKS - 20 {
                let action = tick(&mut pawn, pose, &mut marks, &grid);

                assert!(
                    matches!(action, JobAction::Walk { .. }) && pawn.job != Job::Idle,
                    "gave up at x {x} after {ticks} ticks: {action:?}"
                );
            }
        }

        assert!(matches!(pawn.job, Job::Mine { .. }), "{:?}", pawn.job);
        assert_eq!(holder(&marks, NEAR), Some(PAWN));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let (mut grid, mut marks) = marked(&[NEAR]);
        let mut pawn = Pawn::default();
        let pose = standing_in(cell(4, 1, 5));
        tick(&mut pawn, pose, &mut marks, &grid);
        grid.mutator().remove(cell(5, 0, 5));

        let action = tick(&mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains(NEAR), "the mark stays");
        assert_eq!(holder(&marks, NEAR), None, "but is free");
    }

    const MAX_TURN: f32 = TURN_RATE / TICK_HZ as f32;

    fn heading(yaw: f32) -> Vec3 {
        Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 5.0
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let cases = [
            // (facing, direction of the target, expected turn)
            (0.0, std::f32::consts::FRAC_PI_2, MAX_TURN),
            (0.0, -std::f32::consts::FRAC_PI_2, -MAX_TURN),
            (0.0, 0.001, 0.001),
            (1.0, 0.9, -0.1),
            (3.0, -3.0, MAX_TURN),
            (-3.0, 3.0, -MAX_TURN),
            (3.0, 3.05, 0.05),
        ];

        for (facing, direction, expected) in cases {
            let pose = Pose {
                feet: Vec3::ZERO,
                yaw: facing,
            };

            let walk = steer(pose, heading(direction));

            assert!(
                (walk.yaw_delta - expected).abs() < 1e-4,
                "facing {facing} toward {direction}: turns {} not {expected}",
                walk.yaw_delta
            );
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let sixty = std::f32::consts::FRAC_PI_3;
        let cases = [
            (0.0, 1.0),
            (sixty - 0.05, 1.0),
            (-(sixty - 0.05), 1.0),
            (sixty + 0.05, 0.0),
            (-(sixty + 0.05), 0.0),
            (std::f32::consts::FRAC_PI_2, 0.0),
            (std::f32::consts::PI, 0.0),
        ];

        for (off_by, expected) in cases {
            let pose = Pose {
                feet: Vec3::new(2.0, 1.0, 2.0),
                yaw: 0.0,
            };

            let walk = steer(pose, pose.feet + heading(off_by));

            assert_eq!(walk.forward, expected, "target {off_by} rad off the facing");
        }
    }
}
