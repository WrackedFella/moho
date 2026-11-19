//! The pawn's job loop: picks a marked block, walks to it and digs, from sim state alone.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing;
use crate::selection::PAWN_HALF_EXTENTS;

/// Horizontal distance, in metres, within which a pawn has reached a waypoint.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Turning speed in radians per second: one full turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Route searches an idle pawn may run in one tick.
pub const MAX_ROUTE_ATTEMPTS: usize = 4;

/// Where a pawn is: its feet and which way it faces (yaw 0 faces +Z, positive turns toward +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Detects a walk that has stopped getting closer to its waypoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StallGuard {
    best_distance: f32,
    ticks_without_progress: u32,
}

impl Default for StallGuard {
    fn default() -> Self {
        Self {
            best_distance: f32::INFINITY,
            ticks_without_progress: 0,
        }
    }
}

impl StallGuard {
    /// Records this tick's distance to the waypoint; true once `STALL_TICKS` ticks in a row
    /// have brought the pawn no closer than its best so far.
    fn stalled(&mut self, distance: f32) -> bool {
        if distance < self.best_distance - PROGRESS_EPSILON {
            self.best_distance = distance;
            self.ticks_without_progress = 0;
        } else {
            self.ticks_without_progress += 1;
        }
        self.ticks_without_progress >= STALL_TICKS
    }
}

/// The least approach, in metres, that counts as progress toward a waypoint.
const PROGRESS_EPSILON: f32 = 1e-3;

/// Angle off the heading within which a pawn walks rather than turns in place.
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// What a pawn is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        mark: BlockPos,
        /// The cells to walk through, excluding the one the pawn started in.
        route: Vec<BlockPos>,
        /// The waypoint being walked to; `route.len()` once at the stand cell.
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn should do this tick; the caller applies it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, a point on the ground.
    Walk {
        to: Vec3,
    },
    /// Dig `block`, aiming the ray from the eye at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// A tick's worth of walking: turn by `yaw_delta` radians, move `forward` (0 to 1) of full speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances `pawn.job` one tick for pawn `id` at `pose` and says what to do about it.
/// Claims and releases marks in `marks` as the job starts and ends.
pub fn step(
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(pawn.job, Job::Idle) {
        pawn.job = claim_nearest(pawn, id, pose, marks, grid);
    }
    let Job::Mine {
        mark,
        route,
        mut next,
        mut stall,
    } = std::mem::take(&mut pawn.job)
    else {
        return JobAction::Wait;
    };

    let block = mark;
    let give_up = |marks: &mut MineMarks| {
        marks.release(block.to_array());
        JobAction::Wait
    };
    if !marks.contains(block.to_array()) || !grid.is_solid_at(block) {
        return give_up(marks);
    }

    if next < route.len() {
        let distance = horizontal_distance(pose.feet, waypoint_centre(route[next]));
        if distance <= ARRIVE_RADIUS {
            next += 1;
            stall = StallGuard::default();
        }
    }

    if let Some(&waypoint) = route.get(next) {
        let to = waypoint_centre(waypoint);
        if !routing::standable(grid, waypoint) || stall.stalled(horizontal_distance(pose.feet, to))
        {
            return give_up(marks);
        }
        pawn.job = Job::Mine {
            mark,
            route,
            next,
            stall,
        };
        return JobAction::Walk { to };
    }

    let Some(aim) = nearest_exposed_face(grid, block, pose.feet + Vec3::Y * PAWN_HALF_EXTENTS.y)
    else {
        return give_up(marks);
    };
    // The dig is the job's last act; a block that survives it (the ray hit a neighbour) is
    // offered again on the next idle tick.
    marks.release(block.to_array());
    JobAction::Dig { block, aim }
}

/// Starts a job on the nearest claimable mark that has a route, or stays idle. The tool is
/// equipped through the same slot selection the player has.
fn claim_nearest(
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Job {
    let feet = routing::feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| grid.is_solid_at(block) && is_exposed(grid, block))
        .collect();
    candidates.sort_by_key(|&block| (block - feet).length_squared());

    for block in candidates.into_iter().take(MAX_ROUTE_ATTEMPTS) {
        let is_stand_cell = |cell: BlockPos| {
            (cell.x - block.x).abs() <= 1
                && (cell.z - block.z).abs() <= 1
                && (block.y - 1..=block.y + 2).contains(&cell.y)
                && routing::standable(grid, cell)
        };
        let Some(route) =
            routing::route(grid, feet, is_stand_cell, block, routing::ROUTE_NODE_BUDGET)
        else {
            continue;
        };
        if marks.claim(block.to_array(), id) {
            pawn.select_slot(TOOL_SLOT);
            return Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            };
        }
    }
    Job::Idle
}

const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

fn is_exposed(grid: &VoxelGrid, block: BlockPos) -> bool {
    FACES.iter().any(|&face| !grid.is_solid_at(block + face))
}

/// The centre of the block's face that borders air and lies nearest `eye`.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Option<Vec3> {
    let centre = block.as_vec3() + Vec3::splat(0.5);
    FACES
        .iter()
        .filter(|&&face| !grid.is_solid_at(block + face))
        .map(|&face| centre + face.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

/// The point on the ground at the middle of `cell`'s feet level.
fn waypoint_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

/// Turns toward `to` at [`TURN_RATE`] (clamped to one tick) and walks at full speed once
/// within 60 degrees of facing it, else turns in place.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let heading = (to.x - pose.feet.x).atan2(to.z - pose.feet.z);
    let off = (heading - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: off.clamp(-max_turn, max_turn),
        forward: if off.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::selection::PAWN_HALF_EXTENTS;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const STONE: u32 = 1;
    const ME: PawnId = PawnId(0);
    const EPS: f32 = 1e-3;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn floor(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                grid.mutator().place(cell(x, 0, z), STONE, None);
            }
        }
        grid
    }

    fn standing_in(x: i32, y: i32, z: i32) -> Pose {
        Pose {
            feet: Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
            yaw: 0.0,
        }
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
            .find(|(pos, _)| *pos == block.to_array())
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn eye(pose: Pose) -> Vec3 {
        pose.feet + Vec3::Y * PAWN_HALF_EXTENTS.y
    }

    fn is_idle(pawn: &Pawn) -> bool {
        matches!(pawn.job, Job::Idle)
    }

    /// Steps the job, moving the pawn onto each waypoint it is told to walk to, until it digs.
    /// Returns every action taken, the last being the dig when one happened.
    fn walk_until_dig(
        pawn: &mut Pawn,
        mut pose: Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> (Vec<JobAction>, Pose) {
        let mut actions = Vec::new();
        for _ in 0..200 {
            let action = step(pawn, ME, pose, marks, grid);
            actions.push(action);
            match action {
                JobAction::Walk { to } => pose.feet = Vec3::new(to.x, pose.feet.y, to.z),
                JobAction::Dig { .. } => break,
                JobAction::Wait => {}
            }
        }
        (actions, pose)
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = floor(10, 10);
        let mut pawn = Pawn::default();
        let mut marks = MineMarks::default();

        let action = step(&mut pawn, ME, standing_in(1, 1, 1), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&pawn));
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = floor(12, 12);
        for x in 4..=6 {
            for z in 4..=6 {
                grid.mutator().place(cell(x, 1, z), STONE, None);
            }
        }
        grid.mutator().place(cell(5, 2, 5), STONE, None);
        let buried = cell(5, 1, 5);
        let mut marks = marked(&[buried]);
        let mut pawn = Pawn::default();

        let action = step(&mut pawn, ME, standing_in(1, 1, 1), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&pawn));
        assert_eq!(claimed_by(&marks, buried), None);
        assert!(marks.contains(buried.to_array()), "the mark stays");
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = floor(20, 8);
        let far = cell(6, 1, 4);
        let near = cell(14, 1, 4);
        let sealed = cell(17, 1, 2);
        grid.mutator().place(far, STONE, None);
        grid.mutator().place(near, STONE, None);
        for x in 16..=18 {
            for z in 1..=3 {
                for y in 1..=4 {
                    grid.mutator().place(cell(x, y, z), STONE, None);
                }
            }
        }
        for y in 2..=4 {
            grid.mutator().remove(cell(17, y, 2));
        }
        let mut marks = marked(&[far, near, sealed]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        assert_eq!(pawn.equipped_tool, None);

        step(&mut pawn, ME, standing_in(19, 1, 4), &mut marks, &grid);

        assert_eq!(claimed_by(&marks, near), Some(ME));
        assert_eq!(claimed_by(&marks, far), None);
        assert_eq!(
            claimed_by(&marks, sealed),
            None,
            "nearer, but no route reaches it"
        );
        assert!(matches!(&pawn.job, Job::Mine { mark, .. } if *mark == near));
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn equidistant_marks_are_taken_in_key_order() {
        let mut grid = floor(20, 3);
        let low = cell(6, 1, 1);
        let high = cell(14, 1, 1);
        grid.mutator().place(low, STONE, None);
        grid.mutator().place(high, STONE, None);
        let mut marks = marked(&[high, low]);
        let mut pawn = Pawn::default();

        step(&mut pawn, ME, standing_in(10, 1, 1), &mut marks, &grid);

        assert_eq!(claimed_by(&marks, low), Some(ME));
        assert_eq!(claimed_by(&marks, high), None);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();

        let (actions, _) = walk_until_dig(&mut pawn, standing_in(0, 1, 1), &mut marks, &grid);

        let walked: Vec<(f32, f32)> = actions
            .iter()
            .filter_map(|a| match a {
                JobAction::Walk { to } => Some((to.x, to.z)),
                _ => None,
            })
            .collect();
        let expected: Vec<(f32, f32)> = (1..=5).map(|x| (x as f32 + 0.5, 1.5)).collect();
        assert_eq!(walked.len(), expected.len(), "{actions:?}");
        for (got, want) in walked.iter().zip(&expected) {
            assert!(
                (got.0 - want.0).abs() < EPS && (got.1 - want.1).abs() < EPS,
                "{walked:?}"
            );
        }
        assert!(
            matches!(actions.last(), Some(JobAction::Dig { block, .. }) if *block == target),
            "{actions:?}"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();

        let (actions, pose) = walk_until_dig(&mut pawn, standing_in(0, 1, 1), &mut marks, &grid);

        let Some(JobAction::Dig { aim, .. }) = actions.last().copied() else {
            panic!("the pawn never dug: {actions:?}");
        };
        let west_face_centre = Vec3::new(6.0, 1.5, 1.5);
        assert!((pose.feet.x - 5.5).abs() < EPS, "digs from the west cell");
        assert!(
            (aim - west_face_centre).length() < EPS,
            "aim {aim} is not the west face centre; the eye is at {}",
            eye(pose)
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            (bx, bz) in (4..12i32, 4..12i32),
            stack in 1..=2i32,
            (sx, sz) in (0..3i32, 0..3i32),
        ) {
            let mut grid = floor(16, 16);
            for y in 1..=stack {
                grid.mutator().place(cell(bx, y, bz), STONE, None);
            }
            let target = cell(bx, stack, bz);
            let mut marks = marked(&[target]);
            let mut pawn = Pawn::default();

            let (actions, pose) =
                walk_until_dig(&mut pawn, standing_in(sx, 1, sz), &mut marks, &grid);

            let Some(JobAction::Dig { block, aim }) = actions.last().copied() else {
                return Err(TestCaseError::fail(format!("never dug: {actions:?}")));
            };
            prop_assert_eq!(block, target);
            let centre = target.as_vec3() + Vec3::splat(0.5);
            prop_assert!((eye(pose) - centre).length() <= MINE_REACH);
            prop_assert!((eye(pose) - aim).length() <= MINE_REACH);
            let (dx, dz) = ((pose.feet.x.floor() as i32 - bx).abs(), (pose.feet.z.floor() as i32 - bz).abs());
            prop_assert!(dx <= 1 && dz <= 1, "stand cell {:?} is not beside {target}", pose.feet);
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = floor(12, 3);
        let target = cell(8, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();
        let mut pose = standing_in(0, 1, 1);
        for _ in 0..3 {
            if let JobAction::Walk { to } = step(&mut pawn, ME, pose, &mut marks, &grid) {
                pose.feet = Vec3::new(to.x, pose.feet.y, to.z);
            }
        }
        assert!(
            matches!(pawn.job, Job::Mine { .. }),
            "the pawn is on the job"
        );

        marks.toggle(target.to_array());
        let action = step(&mut pawn, ME, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&pawn));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = floor(12, 3);
        let target = cell(8, 1, 1);
        let other = cell(10, 1, 1);
        grid.mutator().place(target, STONE, None);
        grid.mutator().place(other, STONE, None);
        let mut marks = marked(&[target, other]);
        let mut pawn = Pawn::default();
        let pose = standing_in(0, 1, 1);
        step(&mut pawn, ME, pose, &mut marks, &grid);
        assert_eq!(claimed_by(&marks, target), Some(ME));

        grid.mutator().remove(target);
        marks.block_removed(target.to_array());
        let action = step(&mut pawn, ME, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&pawn));

        step(&mut pawn, ME, pose, &mut marks, &grid);

        assert_eq!(claimed_by(&marks, other), Some(ME));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        marks.claim(target.to_array(), PawnId(1));
        let mut second = Pawn::default();

        let action = step(&mut second, ME, standing_in(0, 1, 1), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&second));
        assert_eq!(claimed_by(&marks, target), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = floor(12, 3);
        let target = cell(8, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();
        let pose = standing_in(0, 1, 1);

        let mut ticks = 0;
        while ticks < STALL_TICKS + 5 {
            step(&mut pawn, ME, pose, &mut marks, &grid);
            ticks += 1;
            if is_idle(&pawn) {
                break;
            }
            assert_eq!(claimed_by(&marks, target), Some(ME));
        }

        assert!(is_idle(&pawn), "gave up within the stall window");
        assert!(ticks > STALL_TICKS - 5, "gave up after only {ticks} ticks");
        assert!(marks.contains(target.to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, target), None);
    }

    #[test]
    fn steady_progress_never_stalls() {
        let mut grid = floor(48, 3);
        let target = cell(44, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();
        let mut pose = standing_in(0, 1, 1);

        let mut dug = false;
        for tick in 0..1000 {
            match step(&mut pawn, ME, pose, &mut marks, &grid) {
                JobAction::Walk { to } => {
                    let toward = Vec3::new(to.x - pose.feet.x, 0.0, to.z - pose.feet.z);
                    let advance = toward.length().min(0.1);
                    pose.feet += toward.normalize_or_zero() * advance;
                }
                JobAction::Dig { block, .. } => {
                    assert_eq!(block, target);
                    dug = true;
                    break;
                }
                JobAction::Wait => assert!(
                    !is_idle(&pawn),
                    "gave up at tick {tick} while still making progress"
                ),
            }
        }

        assert!(dug, "the pawn never reached the block");
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = floor(12, 3);
        let target = cell(8, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();
        let pose = standing_in(0, 1, 1);
        step(&mut pawn, ME, pose, &mut marks, &grid);
        assert_eq!(claimed_by(&marks, target), Some(ME));

        grid.mutator().remove(cell(1, 0, 1));
        let action = step(&mut pawn, ME, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert!(is_idle(&pawn));
        assert!(marks.contains(target.to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, target), None);
    }

    #[test]
    fn stall_guard_trips_on_exactly_the_stall_tick_without_progress() {
        let mut guard = StallGuard::default();

        assert!(!guard.stalled(5.0), "the first distance is progress");
        for tick in 1..STALL_TICKS {
            assert!(!guard.stalled(5.0), "tripped early at {tick}");
        }

        assert!(guard.stalled(5.0));
    }

    #[test]
    fn stall_guard_progress_restarts_the_count() {
        let mut guard = StallGuard::default();
        guard.stalled(10.0);
        for _ in 1..STALL_TICKS {
            assert!(!guard.stalled(10.0));
        }

        assert!(!guard.stalled(9.99), "a closer approach is progress");
        for tick in 1..STALL_TICKS {
            assert!(!guard.stalled(9.99), "tripped early at {tick}");
        }

        assert!(guard.stalled(9.99));
    }

    #[test]
    fn stall_guard_ignores_an_approach_of_only_the_progress_epsilon() {
        let mut guard = StallGuard::default();
        guard.stalled(5.0);
        let barely = 5.0 - PROGRESS_EPSILON;
        for tick in 1..STALL_TICKS {
            assert!(!guard.stalled(barely), "tripped early at {tick}");
        }

        assert!(guard.stalled(barely), "that approach is not progress");
    }

    #[test]
    fn stall_guard_never_trips_while_the_distance_keeps_shrinking() {
        let mut guard = StallGuard::default();

        for tick in 0..(STALL_TICKS * 3) {
            let distance = 100.0 - tick as f32 * 0.01;
            assert!(!guard.stalled(distance), "tripped at {tick}");
        }
    }

    #[test]
    fn a_pawn_already_beside_the_mark_digs_on_its_first_step() {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();

        let action = step(&mut pawn, ME, standing_in(5, 1, 1), &mut marks, &grid);

        let JobAction::Dig { block, aim } = action else {
            panic!("expected a dig, got {action:?}");
        };
        assert_eq!(block, target);
        assert!((aim - Vec3::new(6.0, 1.5, 1.5)).length() < EPS, "{aim}");
    }

    fn walk_after_claiming(feet_on_second_step: Vec3) -> JobAction {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();
        step(&mut pawn, ME, standing_in(0, 1, 1), &mut marks, &grid);
        let pose = Pose {
            feet: feet_on_second_step,
            yaw: 0.0,
        };

        step(&mut pawn, ME, pose, &mut marks, &grid)
    }

    fn walks_to(action: JobAction, x: f32) -> bool {
        matches!(action, JobAction::Walk { to } if (to.x - x).abs() < EPS && (to.z - 1.5).abs() < EPS)
    }

    #[test]
    fn a_pawn_just_inside_the_arrival_radius_moves_on_to_the_next_waypoint() {
        let inside = ARRIVE_RADIUS - 0.01;

        let along_x = walk_after_claiming(Vec3::new(1.5 - inside, 1.0, 1.5));
        let along_z = walk_after_claiming(Vec3::new(1.5, 1.0, 1.5 + inside));

        assert!(walks_to(along_x, 2.5), "{along_x:?}");
        assert!(walks_to(along_z, 2.5), "{along_z:?}");
    }

    #[test]
    fn a_pawn_just_outside_the_arrival_radius_keeps_walking_to_the_same_waypoint() {
        let outside = ARRIVE_RADIUS + 0.01;

        let along_x = walk_after_claiming(Vec3::new(1.5 - outside, 1.0, 1.5));
        let along_z = walk_after_claiming(Vec3::new(1.5, 1.0, 1.5 + outside));

        assert!(walks_to(along_x, 1.5), "{along_x:?}");
        assert!(walks_to(along_z, 1.5), "{along_z:?}");
    }

    #[test]
    fn the_dig_aim_is_measured_from_an_eye_above_the_feet() {
        let mut grid = floor(12, 3);
        let target = cell(6, 1, 1);
        grid.mutator().place(target, STONE, None);
        let mut marks = marked(&[target]);
        let mut pawn = Pawn {
            job: Job::Mine {
                mark: target,
                route: Vec::new(),
                next: 0,
                stall: StallGuard::default(),
            },
            ..Pawn::default()
        };
        // Eye at y 2.65: the top face centre (6.5, 2.0, 1.5) is nearer than the west one
        // (6.0, 1.5, 1.5). An eye at the feet, below them or at a different height picks west.
        let pose = Pose {
            feet: Vec3::new(5.5, 1.5, 1.5),
            yaw: 0.0,
        };

        let action = step(&mut pawn, ME, pose, &mut marks, &grid);

        let JobAction::Dig { aim, .. } = action else {
            panic!("expected a dig, got {action:?}");
        };
        assert!((aim - Vec3::new(6.5, 2.0, 1.5)).length() < EPS, "{aim}");
    }

    #[test]
    fn a_pawn_stands_on_ground_two_blocks_above_the_mark_when_that_is_the_nearest_stand_cell() {
        let mut grid = VoxelGrid::new(16);
        for x in 0..12 {
            for z in 0..3 {
                for y in 0..=2 {
                    grid.mutator().place(cell(x, y, z), STONE, None);
                }
            }
        }
        let target = cell(6, 1, 1);
        grid.mutator().remove(cell(6, 2, 1));
        let mut marks = marked(&[target]);
        let mut pawn = Pawn::default();

        let (actions, pose) = walk_until_dig(&mut pawn, standing_in(0, 3, 1), &mut marks, &grid);

        assert!(
            matches!(actions.last(), Some(JobAction::Dig { block, .. }) if *block == target),
            "{actions:?}"
        );
        assert!(
            (pose.feet.x - 5.5).abs() < EPS,
            "digs from the plateau beside the pit, not from inside it: {:?}",
            pose.feet
        );
    }

    #[test]
    fn a_block_is_exposed_through_any_one_open_face() {
        for open in FACES {
            let mut grid = VoxelGrid::new(16);
            let block = cell(5, 5, 5);
            grid.mutator().place(block, STONE, None);
            for face in FACES {
                if face != open {
                    grid.mutator().place(block + face, STONE, None);
                }
            }

            assert!(is_exposed(&grid, block), "open face {open}");
        }
    }

    #[test]
    fn a_block_walled_in_on_all_sides_is_not_exposed() {
        let mut grid = VoxelGrid::new(16);
        let block = cell(5, 5, 5);
        grid.mutator().place(block, STONE, None);
        for face in FACES {
            grid.mutator().place(block + face, STONE, None);
        }

        assert!(!is_exposed(&grid, block));
    }

    #[test]
    fn the_aim_is_the_centre_of_the_only_open_face() {
        let block = cell(5, 5, 5);
        let centre = Vec3::new(5.5, 5.5, 5.5);
        for open in FACES {
            let mut grid = VoxelGrid::new(16);
            grid.mutator().place(block, STONE, None);
            for face in FACES {
                if face != open {
                    grid.mutator().place(block + face, STONE, None);
                }
            }
            let eye = Vec3::new(1.0, 12.0, -3.0);

            let aim = nearest_exposed_face(&grid, block, eye);

            let want = centre + open.as_vec3() * 0.5;
            assert!(
                aim.is_some_and(|aim| (aim - want).length() < EPS),
                "open face {open}: {aim:?}"
            );
        }
    }

    #[test]
    fn the_aim_is_the_open_face_nearest_the_eye_on_each_axis_and_side() {
        let block = cell(5, 5, 5);
        let mut grid = VoxelGrid::new(16);
        grid.mutator().place(block, STONE, None);
        let centre = Vec3::new(5.5, 5.5, 5.5);

        for face in FACES {
            let eye = centre + face.as_vec3() * 20.0;

            let aim = nearest_exposed_face(&grid, block, eye);

            let want = centre + face.as_vec3() * 0.5;
            assert!(
                aim.is_some_and(|aim| (aim - want).length() < EPS),
                "eye toward {face}: {aim:?}"
            );
        }
    }

    #[test]
    fn a_walled_in_block_has_no_aim() {
        let mut grid = VoxelGrid::new(16);
        let block = cell(5, 5, 5);
        grid.mutator().place(block, STONE, None);
        for face in FACES {
            grid.mutator().place(block + face, STONE, None);
        }

        assert_eq!(nearest_exposed_face(&grid, block, Vec3::ZERO), None);
    }

    fn steer_toward(feet: Vec3, yaw: f32, angle_from_north: f32) -> WalkStep {
        let to = feet + Vec3::new(angle_from_north.sin(), 0.0, angle_from_north.cos()) * 5.0;
        steer(Pose { feet, yaw }, to)
    }

    #[test]
    fn steer_aims_from_the_pawns_own_position_not_the_origin() {
        let feet_cases = [
            Vec3::new(10.0, 0.0, 20.0),
            Vec3::new(-8.0, 0.0, -15.0),
            Vec3::new(-30.0, 0.0, 12.0),
            Vec3::new(14.0, 0.0, -25.0),
        ];
        // (yaw, target bearing, expected yaw_delta, expected forward)
        let max = max_turn_per_tick();
        let tau = std::f32::consts::TAU;
        let cases = [
            (0.0, 0.05, 0.05, 1.0),
            (0.0, -0.05, -0.05, 1.0),
            (1.0, 1.05, 0.05, 1.0),
            (-1.0, -1.05, -0.05, 1.0),
            (0.0, 2.0, max, 0.0),
            (0.0, -2.0, -max, 0.0),
            (0.0, 3.04, max, 0.0),
            (0.0, -3.04, -max, 0.0),
            (3.0, -3.2, tau - 6.2, 1.0),
            (-3.0, 3.2, 6.2 - tau, 1.0),
            (2.0, -2.0, tau - 4.0, 0.0),
        ];

        for feet in feet_cases {
            for (yaw, bearing, delta, forward) in cases {
                let got = steer_toward(feet, yaw, bearing);

                let expected_delta = delta.clamp(-max, max);
                assert!(
                    (got.yaw_delta - expected_delta).abs() < 1e-4,
                    "feet {feet}, yaw {yaw}, bearing {bearing}: {got:?}, want {expected_delta}"
                );
                assert_eq!(
                    got.forward, forward,
                    "feet {feet}, yaw {yaw}, bearing {bearing}"
                );
            }
        }
    }

    fn facing_target(angle_from_ahead: f32) -> Vec3 {
        Vec3::new(angle_from_ahead.sin(), 0.0, angle_from_ahead.cos()) * 5.0
    }

    fn max_turn_per_tick() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let at = |yaw| Pose {
            feet: Vec3::ZERO,
            yaw,
        };

        let right = steer(at(0.0), facing_target(1.2));
        let left = steer(at(0.0), facing_target(-1.2));
        let small = steer(at(0.0), facing_target(0.01));
        let wrapped = steer(at(3.0), facing_target(-3.0));
        let ahead = steer(at(0.5), facing_target(0.5));

        assert!(
            (right.yaw_delta - max_turn_per_tick()).abs() < EPS * 0.1,
            "{right:?}"
        );
        assert!(
            (left.yaw_delta + max_turn_per_tick()).abs() < EPS * 0.1,
            "{left:?}"
        );
        assert!(
            (small.yaw_delta - 0.01).abs() < EPS * 0.1,
            "no overshoot: {small:?}"
        );
        assert!(
            (wrapped.yaw_delta - max_turn_per_tick()).abs() < EPS * 0.1,
            "turns the short way round: {wrapped:?}"
        );
        assert!(ahead.yaw_delta.abs() < EPS * 0.1, "{ahead:?}");
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let pose = Pose {
            feet: Vec3::ZERO,
            yaw: 0.0,
        };

        let ahead = steer(pose, facing_target(0.0));
        let within = steer(pose, facing_target(50f32.to_radians()));
        let beyond = steer(pose, facing_target(70f32.to_radians()));
        let behind = steer(pose, facing_target(std::f32::consts::PI));

        assert_eq!(ahead.forward, 1.0);
        assert_eq!(within.forward, 1.0);
        assert_eq!(beyond.forward, 0.0);
        assert!(beyond.yaw_delta > 0.0, "still turns in place");
        assert_eq!(behind.forward, 0.0);
        assert!(behind.yaw_delta.abs() > 0.0);
    }
}
