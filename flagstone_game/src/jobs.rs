//! What an unpossessed pawn does: claim a marked block, walk to it, dig it.

use std::f32::consts::{PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward the current waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Radians a pawn turns per second.
pub const TURN_RATE: f32 = TAU;

/// The angle off its facing, in radians, within which a pawn walks rather than turns in place.
pub const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// Route searches one idle pawn may start in a tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Height of the eye above the feet, in metres: the centre of the 2.3 m capsule.
pub const EYE_HEIGHT: f32 = 1.15;

/// Where a pawn is: its feet, and the yaw it faces (0 faces +Z, positive turns toward +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Counts ticks spent without getting closer to the current waypoint.
#[derive(Clone, Debug, PartialEq)]
pub struct StallGuard {
    best_distance: f32,
    idle_ticks: u32,
}

/// What a pawn is doing when nobody possesses it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The marked block this pawn has claimed.
        mark: BlockPos,
        /// Cells to walk through, the pawn's own cell first and a stand cell last.
        route: Vec<BlockPos>,
        /// Index of the waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of the current waypoint.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim` from the eye.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// How to move this tick to head for a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to turn this tick.
    pub yaw_delta: f32,
    /// Move axis along the facing, 0 or 1.
    pub forward: f32,
}

impl Default for StallGuard {
    fn default() -> Self {
        Self {
            best_distance: f32::INFINITY,
            idle_ticks: 0,
        }
    }
}

impl StallGuard {
    /// Records this tick's distance to the waypoint; true once `STALL_TICKS` ticks in a row
    /// brought the pawn no closer.
    fn stalled(&mut self, distance: f32) -> bool {
        if distance < self.best_distance - PROGRESS_EPSILON {
            self.best_distance = distance;
            self.idle_ticks = 0;
        } else {
            self.idle_ticks += 1;
        }
        self.idle_ticks >= STALL_TICKS
    }
}

/// The least a pawn must close in on a waypoint, in metres, for a tick to count as progress.
const PROGRESS_EPSILON: f32 = 1e-3;

/// Lowest stand cell below a block, in cells, and highest above it: the dig stays in reach.
const STAND_BELOW: i32 = 1;
const STAND_ABOVE: i32 = 2;

const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// Whether a pawn standing with its feet in `cell` can dig `block` from there: within one
/// column of it, feet from one below to two above it, and standable.
pub fn is_stand_cell(grid: &VoxelGrid, block: BlockPos, cell: BlockPos) -> bool {
    let off = cell - block;
    off.x.abs() <= 1
        && off.z.abs() <= 1
        && (-STAND_BELOW..=STAND_ABOVE).contains(&off.y)
        && standable(grid, cell)
}

/// Whether any face of `block` borders air.
fn is_exposed(grid: &VoxelGrid, block: BlockPos) -> bool {
    FACES.iter().any(|&face| !grid.is_solid_at(block + face))
}

fn centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

/// Advances `pawn`'s job one tick: an idle pawn claims the nearest mark it can route to, a
/// mining pawn walks its route and finally digs. A dig ends the job and frees the claim; the
/// mark itself goes when the block does.
pub fn step(
    pawn_id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if pawn.job == Job::Idle {
        claim(pawn_id, pawn, pose, marks, grid);
    }
    follow(pawn, pose, marks, grid)
}

/// Starts a job on the nearest unclaimed mark with a way to it, trying at most
/// `ROUTE_ATTEMPTS_PER_TICK` marks. Marks that failed stay in `Pawn::route_tried` until the
/// candidates run out or a route is found, so the next tick carries on past them.
fn claim(pawn_id: PawnId, pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) {
    let feet = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| !pawn.route_tried.contains(&block.to_array()) && is_exposed(grid, block))
        .collect();
    candidates.sort_by_key(|&block| (block - feet).length_squared());
    let all_tried_now = candidates.len() <= ROUTE_ATTEMPTS_PER_TICK;

    for block in candidates.into_iter().take(ROUTE_ATTEMPTS_PER_TICK) {
        let found = route(
            grid,
            feet,
            |cell| is_stand_cell(grid, block, cell),
            block,
            ROUTE_NODE_BUDGET,
        );
        let Some(route) = found else {
            pawn.route_tried.push(block.to_array());
            continue;
        };
        marks.claim(block.to_array(), pawn_id);
        pawn.select_slot(TOOL_SLOT);
        pawn.route_tried.clear();
        pawn.job = Job::Mine {
            mark: block,
            route,
            next: 0,
            stall: StallGuard::default(),
        };
        return;
    }
    if all_tried_now {
        pawn.route_tried.clear();
    }
}

/// Moves a mining pawn along its route; does nothing for an idle one.
fn follow(pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = &mut pawn.job
    else {
        return JobAction::Wait;
    };
    let mark = *mark;
    if !marks.contains(mark.to_array()) {
        pawn.job = Job::Idle;
        return JobAction::Wait;
    }

    if *next < route.len() && horizontal_gap(pose.feet, centre(route[*next])) <= ARRIVE_RADIUS {
        *next += 1;
        *stall = StallGuard::default();
    }
    let Some(&waypoint) = route.get(*next) else {
        return dig(pawn, pose, mark, marks, grid);
    };
    let to = centre(waypoint);
    if !standable(grid, waypoint) || stall.stalled(horizontal_gap(pose.feet, to)) {
        release(pawn, mark, marks);
        return JobAction::Wait;
    }
    JobAction::Walk { to }
}

fn release(pawn: &mut Pawn, mark: BlockPos, marks: &mut MineMarks) {
    marks.release(mark.to_array());
    pawn.job = Job::Idle;
}

/// Ends the job with a dig at the centre of the face of `mark` nearest the eye; a block with
/// no face left open is given up instead.
fn dig(
    pawn: &mut Pawn,
    pose: Pose,
    mark: BlockPos,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
    let middle = mark.as_vec3() + Vec3::splat(0.5);
    let nearest = FACES
        .iter()
        .filter(|&&face| !grid.is_solid_at(mark + face))
        .map(|&face| middle + face.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)));
    release(pawn, mark, marks);
    match nearest {
        Some(aim) => JobAction::Dig { block: mark, aim },
        None => JobAction::Wait,
    }
}

fn horizontal_gap(feet: Vec3, to: Vec3) -> f32 {
    (to.x - feet.x).hypot(to.z - feet.z)
}

/// Turn and walk to head for `to` over a tick of `dt` seconds: turn at `TURN_RATE` (never past
/// the target), and walk too while the target is within `WALK_CONE` of the facing.
pub fn steer(pose: Pose, to: Vec3, dt: f32) -> WalkStep {
    let (dx, dz) = (to.x - pose.feet.x, to.z - pose.feet.z);
    if dx == 0.0 && dz == 0.0 {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let off = (dx.atan2(dz) - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE * dt;
    WalkStep {
        yaw_delta: off.clamp(-max_turn, max_turn),
        forward: if off.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::standable;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const ME: PawnId = PawnId(0);
    const DT: f32 = 1.0 / 60.0;

    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                grid.mutator().place(BlockPos::new(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, Some(7));
    }

    fn marked(positions: &[[i32; 3]]) -> MineMarks {
        let mut marks = MineMarks::default();
        for &pos in positions {
            marks.toggle(pos);
        }
        marks
    }

    fn pose_in(x: i32, y: i32, z: i32) -> Pose {
        Pose {
            feet: Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
            yaw: 0.0,
        }
    }

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn is_mining(pawn: &Pawn) -> bool {
        matches!(pawn.job, Job::Mine { .. })
    }

    fn horizontal(v: Vec3) -> [f32; 2] {
        [v.x, v.z]
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    /// Steps until the pawn digs, moving the pose onto each waypoint it is sent to. Returns the
    /// distinct horizontal targets, the pose at the dig and the dig.
    fn walk_until_dig(
        pawn: &mut Pawn,
        mut pose: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> (Vec<[f32; 2]>, Pose, Option<JobAction>) {
        let mut targets: Vec<[f32; 2]> = Vec::new();
        for _ in 0..40 {
            match step(ME, pawn, pose, marks, grid) {
                JobAction::Walk { to } => {
                    if targets.last() != Some(&horizontal(to)) {
                        targets.push(horizontal(to));
                    }
                    pose.feet.x = to.x;
                    pose.feet.z = to.z;
                }
                dig @ JobAction::Dig { .. } => return (targets, pose, Some(dig)),
                JobAction::Wait => {}
            }
        }
        (targets, pose, None)
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = floor(8, 8);
        let mut pawn = Pawn::default();
        let mut marks = MineMarks::default();

        let action = step(ME, &mut pawn, pose_in(2, 1, 2), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = floor(12, 12);
        for (x, y, z) in [
            (5, 1, 5),
            (5, 2, 5),
            (4, 1, 5),
            (6, 1, 5),
            (5, 1, 4),
            (5, 1, 6),
        ] {
            place(&mut grid, x, y, z);
        }
        place(&mut grid, 10, 1, 10);
        let buried = [5, 1, 5];
        let open = [10, 1, 10];
        assert!(
            is_stand_cell(&grid, BlockPos::new(5, 1, 5), BlockPos::new(4, 2, 5)),
            "the buried block has a reachable stand cell, only its faces are covered"
        );
        let mut marks = marked(&[buried, open]);
        let mut pawn = Pawn::default();

        step(ME, &mut pawn, pose_in(5, 1, 2), &mut marks, &grid);

        assert_eq!(
            claimed_by(&marks, buried),
            None,
            "nothing to dig from outside"
        );
        assert_eq!(
            claimed_by(&marks, open),
            Some(ME),
            "the open mark is taken instead"
        );
    }

    /// Two-block walls round the square x, z in `lo..=hi`: no pawn outside can get in.
    fn fence(grid: &mut VoxelGrid, lo: i32, hi: i32) {
        for a in lo..=hi {
            for y in 1..=2 {
                for (x, z) in [(a, lo), (a, hi), (lo, a), (hi, a)] {
                    place(grid, x, y, z);
                }
            }
        }
    }

    #[test]
    fn a_nearer_unroutable_mark_is_skipped_for_a_reachable_one() {
        let mut grid = floor(12, 12);
        fence(&mut grid, 3, 7);
        place(&mut grid, 5, 1, 5);
        place(&mut grid, 10, 1, 10);
        let fenced = [5, 1, 5];
        let reachable = [10, 1, 10];
        let mut marks = marked(&[fenced, reachable]);
        let mut pawn = Pawn::default();

        step(ME, &mut pawn, pose_in(5, 1, 1), &mut marks, &grid);

        assert_eq!(claimed_by(&marks, fenced), None, "no way in");
        assert_eq!(
            claimed_by(&marks, reachable),
            Some(ME),
            "claimed on the first tick"
        );
    }

    #[test]
    fn the_route_attempts_are_capped_per_tick_and_resume_next_tick() {
        let mut grid = floor(20, 20);
        fence(&mut grid, 3, 8);
        let fenced = [[4, 1, 4], [7, 1, 4], [4, 1, 7], [7, 1, 7]];
        for [x, y, z] in fenced {
            place(&mut grid, x, y, z);
        }
        place(&mut grid, 15, 1, 0);
        let reachable = [15, 1, 0];
        let mut marks = marked(&[fenced[0], fenced[1], fenced[2], fenced[3], reachable]);
        let mut pawn = Pawn::default();
        let pose = pose_in(5, 1, 0);

        step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(pawn.job, Job::Idle, "four failed searches use the tick up");
        assert_eq!(claimed_by(&marks, reachable), None);

        step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(
            claimed_by(&marks, reachable),
            Some(ME),
            "the next tick carries on with the marks not yet tried"
        );
        assert!(is_mining(&pawn));
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = floor(12, 10);
        place(&mut grid, 2, 1, 4);
        place(&mut grid, 7, 1, 4);
        let mut marks = marked(&[[2, 1, 4], [7, 1, 4]]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        step(ME, &mut pawn, pose_in(9, 1, 4), &mut marks, &grid);

        assert_eq!(claimed_by(&marks, [7, 1, 4]), Some(ME));
        assert_eq!(claimed_by(&marks, [2, 1, 4]), None);
        assert!(
            matches!(&pawn.job, Job::Mine { mark, .. } if *mark == BlockPos::new(7, 1, 4)),
            "{:?}",
            pawn.job
        );
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn marks_equally_near_go_to_the_first_in_key_order() {
        let mut grid = floor(10, 10);
        place(&mut grid, 4, 1, 2);
        place(&mut grid, 4, 1, 6);
        let mut marks = marked(&[[4, 1, 6], [4, 1, 2]]);
        let mut pawn = Pawn::default();

        step(ME, &mut pawn, pose_in(4, 1, 4), &mut marks, &grid);

        assert_eq!(claimed_by(&marks, [4, 1, 2]), Some(ME));
        assert_eq!(claimed_by(&marks, [4, 1, 6]), None);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = floor(12, 10);
        place(&mut grid, 6, 1, 4);
        place(&mut grid, 2, 1, 4);
        let mut marks = marked(&[[6, 1, 4], [2, 1, 4]]);
        marks.claim([6, 1, 4], ME);
        let other = PawnId(1);
        let mut pawn = Pawn::default();

        step(other, &mut pawn, pose_in(7, 1, 4), &mut marks, &grid);

        assert_eq!(
            claimed_by(&marks, [6, 1, 4]),
            Some(ME),
            "the nearer mark is taken"
        );
        assert_eq!(claimed_by(&marks, [2, 1, 4]), Some(other));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = floor(10, 10);
        place(&mut grid, 5, 1, 4);
        let mut marks = marked(&[[5, 1, 4]]);
        let mut pawn = Pawn::default();
        let start = pose_in(1, 1, 4);

        let (targets, at_dig, dig) = walk_until_dig(&mut pawn, start, &mut marks, &grid);

        let walked: Vec<[f32; 2]> = targets
            .into_iter()
            .filter(|&t| t != horizontal(start.feet))
            .collect();
        assert_eq!(walked, [[2.5, 4.5], [3.5, 4.5], [4.5, 4.5]]);
        assert_eq!(
            horizontal(at_dig.feet),
            [4.5, 4.5],
            "digs only once it stands there"
        );
        assert!(
            matches!(dig, Some(JobAction::Dig { block, .. }) if block == BlockPos::new(5, 1, 4)),
            "{dig:?}"
        );
    }

    #[test]
    fn a_waypoint_is_reached_within_the_arrive_radius_and_not_before() {
        let mut grid = floor(12, 10);
        place(&mut grid, 8, 1, 4);
        let mut marks = marked(&[[8, 1, 4]]);
        let mut pawn = Pawn::default();
        step(ME, &mut pawn, pose_in(1, 1, 4), &mut marks, &grid);
        let Job::Mine { route, next, .. } = &pawn.job else {
            panic!("expected a claimed mark, got {:?}", pawn.job);
        };
        let current = route[*next];
        let after = route[*next + 1];
        let centre = |cell: BlockPos| [cell.x as f32 + 0.5, cell.z as f32 + 0.5];
        let target = |action: JobAction| match action {
            JobAction::Walk { to } => horizontal(to),
            other => panic!("expected a walk, got {other:?}"),
        };
        let [cx, cz] = centre(current);
        let feet_off = |gap: f32| Pose {
            feet: Vec3::new(cx - gap, current.y as f32, cz),
            yaw: 0.0,
        };

        let outside = step(ME, &mut pawn, feet_off(0.40), &mut marks, &grid);
        let inside = step(ME, &mut pawn, feet_off(0.30), &mut marks, &grid);

        assert_eq!(target(outside), centre(current), "0.40 m is not arrived");
        assert_eq!(target(inside), centre(after), "0.30 m is arrived");
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut grid = floor(10, 10);
        place(&mut grid, 5, 1, 4);
        place(&mut grid, 5, 2, 4);
        place(&mut grid, 5, 1, 5);
        place(&mut grid, 6, 1, 4);
        let mut marks = marked(&[[5, 1, 4]]);
        let mut pawn = Pawn::default();
        let on_the_ground_block = pose_in(6, 2, 4);

        let (_, _, dig) = walk_until_dig(&mut pawn, on_the_ground_block, &mut marks, &grid);

        let Some(JobAction::Dig { block, aim }) = dig else {
            panic!("expected a dig, got {dig:?}");
        };
        assert_eq!(block, BlockPos::new(5, 1, 4));
        assert!(
            near(aim, Vec3::new(5.5, 1.5, 4.0)),
            "the covered top and east faces are skipped, got {aim:?}"
        );
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let walking = || {
            let mut grid = floor(12, 10);
            place(&mut grid, 8, 1, 4);
            let mut marks = marked(&[[8, 1, 4]]);
            let mut pawn = Pawn::default();
            for _ in 0..3 {
                step(ME, &mut pawn, pose_in(1, 1, 4), &mut marks, &grid);
            }
            assert!(is_mining(&pawn), "walking to the mark");
            (grid, marks, pawn)
        };

        let (grid, mut marks, mut pawn) = walking();
        marks.toggle([8, 1, 4]);
        let after_cancel = step(ME, &mut pawn, pose_in(1, 1, 4), &mut marks, &grid);

        assert_eq!(after_cancel, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(
            !marks.contains([8, 1, 4]),
            "a released claim does not bring the mark back"
        );
        assert!(grid.is_solid_at(BlockPos::new(8, 1, 4)), "the block stays");

        let (mut grid, mut marks, mut pawn) = walking();
        grid.mutator().remove(BlockPos::new(8, 1, 4));
        marks.block_removed([8, 1, 4]);
        let after_dug = step(ME, &mut pawn, pose_in(1, 1, 4), &mut marks, &grid);

        assert_eq!(after_dug, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = floor(12, 10);
        place(&mut grid, 8, 1, 4);
        let mut marks = marked(&[[8, 1, 4]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(1, 1, 4);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        assert!(is_mining(&pawn), "claimed on the first tick");
        let mut calls = 1;

        while is_mining(&pawn) && calls < 2 * STALL_TICKS {
            step(ME, &mut pawn, pose, &mut marks, &grid);
            calls += 1;
        }

        assert!(
            (STALL_TICKS..=STALL_TICKS + 2).contains(&calls),
            "released after {calls} ticks"
        );
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains([8, 1, 4]), "the mark stays");
        assert_eq!(claimed_by(&marks, [8, 1, 4]), None);
    }

    #[test]
    fn a_walk_that_keeps_closing_in_is_not_stalled() {
        let mut grid = floor(30, 1);
        place(&mut grid, 20, 1, 0);
        let mut marks = marked(&[[20, 1, 0]]);
        let mut pawn = Pawn::default();
        let mut pose = pose_in(0, 1, 0);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        assert!(is_mining(&pawn), "claimed on the first tick");

        for tick in 0..2 * STALL_TICKS {
            if let JobAction::Walk { to } = step(ME, &mut pawn, pose, &mut marks, &grid) {
                let gap = Vec3::new(to.x - pose.feet.x, 0.0, to.z - pose.feet.z);
                pose.feet += gap.clamp_length_max(0.05);
            }
            assert!(
                is_mining(&pawn),
                "gave up on tick {tick} while still closing in"
            );
        }
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = floor(12, 10);
        place(&mut grid, 8, 1, 4);
        let mut marks = marked(&[[8, 1, 4]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(1, 1, 4);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        let Job::Mine { route, next, .. } = &pawn.job else {
            panic!("expected a claimed mark, got {:?}", pawn.job);
        };
        let ahead = route[*next];
        assert!(standable(&grid, ahead));
        grid.mutator().remove(ahead - BlockPos::Y);

        let action = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains([8, 1, 4]), "the mark stays for another try");
        assert_eq!(claimed_by(&marks, [8, 1, 4]), None);
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let pose = Pose {
            feet: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
        };
        let max_turn = TURN_RATE * DT;
        let right = Vec3::new(10.0, 1.0, 0.0);
        let left = Vec3::new(-10.0, 1.0, 0.0);

        let toward_right = steer(pose, right, DT);
        let toward_left = steer(pose, left, DT);
        let slow_frame = steer(pose, right, 0.5);

        assert!(
            (toward_right.yaw_delta - max_turn).abs() < 1e-4,
            "{toward_right:?}"
        );
        assert!(
            (toward_left.yaw_delta + max_turn).abs() < 1e-4,
            "{toward_left:?}"
        );
        assert!(
            (slow_frame.yaw_delta - std::f32::consts::FRAC_PI_2).abs() < 1e-4,
            "a long frame turns the whole quarter and no further: {slow_frame:?}"
        );
    }

    #[test]
    fn steer_turns_the_short_way_round() {
        let facing = Pose {
            feet: Vec3::ZERO,
            yaw: 3.0,
        };
        let target = Vec3::new((-3.0f32).sin() * 10.0, 0.0, (-3.0f32).cos() * 10.0);

        let step = steer(facing, target, DT);

        assert!(
            step.yaw_delta > 0.0,
            "past +Z behind it is shorter: {step:?}"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let pose = Pose {
            feet: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
        };
        let at = |degrees: f32| {
            let angle = degrees.to_radians();
            Vec3::new(angle.sin() * 5.0, 1.0, angle.cos() * 5.0)
        };

        let ahead = steer(pose, at(0.0), DT);
        let close_enough = steer(pose, at(59.5), DT);
        let too_far_round = steer(pose, at(60.5), DT);

        assert_eq!(ahead.forward, 1.0);
        assert!(ahead.yaw_delta.abs() < 1e-4, "{ahead:?}");
        assert_eq!(close_enough.forward, 1.0, "walks while it turns");
        assert!(close_enough.yaw_delta > 0.0);
        assert_eq!(too_far_round.forward, 0.0, "turns in place");
        assert!(too_far_round.yaw_delta > 0.0);
    }

    #[test]
    fn a_flat_neighbour_is_a_stand_cell() {
        let mut grid = floor(10, 10);
        place(&mut grid, 5, 1, 5);

        assert!(is_stand_cell(
            &grid,
            BlockPos::new(5, 1, 5),
            BlockPos::new(4, 1, 5)
        ));
        assert!(is_stand_cell(
            &grid,
            BlockPos::new(5, 1, 5),
            BlockPos::new(6, 1, 6)
        ));
        assert!(
            is_stand_cell(&grid, BlockPos::new(5, 1, 5), BlockPos::new(5, 2, 5)),
            "on top of the block"
        );
    }

    #[test]
    fn cells_too_far_across_or_not_standable_are_not_stand_cells() {
        let mut grid = floor(10, 10);
        place(&mut grid, 5, 1, 5);
        let block = BlockPos::new(5, 1, 5);

        assert!(
            !is_stand_cell(&grid, block, BlockPos::new(3, 1, 5)),
            "two columns off"
        );
        assert!(
            !is_stand_cell(&grid, block, BlockPos::new(5, 1, 8)),
            "three columns off"
        );
        assert!(
            !is_stand_cell(&grid, block, BlockPos::new(4, 2, 5)),
            "nothing to stand on"
        );
    }

    #[test]
    fn stand_cells_run_from_one_below_to_two_above_the_block() {
        let mut high = floor(10, 10);
        for y in 1..=2 {
            place(&mut high, 4, y, 5);
        }
        let mut too_high = floor(10, 10);
        for y in 1..=3 {
            place(&mut too_high, 4, y, 5);
        }
        let mut low = floor(10, 10);
        place(&mut low, 5, 3, 5);
        place(&mut low, 4, 1, 5);
        let block_high_up = BlockPos::new(5, 3, 5);

        assert!(is_stand_cell(
            &high,
            BlockPos::new(5, 1, 5),
            BlockPos::new(4, 3, 5)
        ));
        assert!(!is_stand_cell(
            &too_high,
            BlockPos::new(5, 1, 5),
            BlockPos::new(4, 4, 5)
        ));
        assert!(
            is_stand_cell(&low, block_high_up, BlockPos::new(4, 2, 5)),
            "one below"
        );
        let mut bare = floor(10, 10);
        place(&mut bare, 5, 3, 5);
        let two_below = BlockPos::new(4, 1, 5);
        assert!(standable(&bare, two_below), "bare floor to stand on");
        assert!(!is_stand_cell(&bare, block_high_up, two_below), "two below");
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            heights in proptest::collection::vec(1i32..=3, 144),
            bx in 3i32..9, by in 0i32..4, bz in 3i32..9,
            dx in -3i32..=3, dz in -3i32..=3, cy in 0i32..8,
        ) {
            let mut grid = VoxelGrid::new(16);
            for x in 0..12 {
                for z in 0..12 {
                    let top = heights[usize::try_from(x * 12 + z).expect("small index")];
                    for y in 0..top {
                        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
                    }
                }
            }
            let block = BlockPos::new(bx, by, bz);
            let cell = BlockPos::new(bx + dx, cy, bz + dz);

            if is_stand_cell(&grid, block, cell) {
                let eye = Vec3::new(cell.x as f32 + 0.5, cell.y as f32 + EYE_HEIGHT, cell.z as f32 + 0.5);
                let centre = block.as_vec3() + Vec3::splat(0.5);
                prop_assert!(eye.distance(centre) <= MINE_REACH, "{eye:?} to {centre:?}");
            }
        }
    }
}
