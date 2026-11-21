//! What an idle pawn does on its own: take a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives its mark up.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second: one turn.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// How far off its facing a target may be, in radians, for the pawn to walk at it.
pub const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// Counts the ticks a walk has made no progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    idle_ticks: u32,
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walking the `route` to the cell it digs `mark` from.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn stands and faces, as the physics reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub eye: Vec3,
    /// Facing in radians; 0 faces +Z and positive turns toward +X.
    pub yaw: f32,
}

/// What a pawn's job asks of it this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward the centre of a cell, `to` at its floor.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: the turn to make and how much of full speed to move at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances `pawn`'s job one tick and says what it does next.
pub fn step(
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    todo!(
        "step {id:?} at {pose:?} on {} marks, {:?}, in a grid of {}",
        marks.iter().count(),
        pawn.job,
        grid.chunk_size()
    )
}

/// The turn and walk that take a pawn at `pose` toward `to`.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let _ = (pose, to);
    WalkStep {
        yaw_delta: 0.0,
        forward: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::standable;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    /// The capsule centre above the feet, where the dig starts.
    const EYE_HEIGHT: f32 = 1.15;

    fn pose_at(x: f32, z: f32) -> Pose {
        pose_with(Vec3::new(x, 1.0, z), 0.0)
    }

    fn pose_with(feet: Vec3, yaw: f32) -> Pose {
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw,
        }
    }

    fn put(grid: &mut VoxelGrid, pos: [i32; 3]) {
        grid.mutator().place(BlockPos::from_array(pos), 1, None);
    }

    fn lay_ground(grid: &mut VoxelGrid, xs: std::ops::Range<i32>, zs: std::ops::Range<i32>) {
        for x in xs {
            for z in zs.clone() {
                put(grid, [x, 0, z]);
            }
        }
    }

    /// Ground over `0..16` by `0..16` with `blocks` standing on it, each marked.
    fn world(blocks: &[[i32; 3]]) -> (VoxelGrid, MineMarks) {
        let mut grid = VoxelGrid::new(16);
        lay_ground(&mut grid, 0..16, 0..16);
        let mut marks = MineMarks::default();
        for &block in blocks {
            put(&mut grid, block);
            marks.toggle(block);
        }
        (grid, marks)
    }

    fn mined_mark(pawn: &Pawn) -> [i32; 3] {
        match &pawn.job {
            Job::Mine { mark, .. } => *mark,
            Job::Idle => panic!("the pawn has no job"),
        }
    }

    fn unclaimed(marks: &MineMarks) -> Vec<[i32; 3]> {
        marks.unclaimed().collect()
    }

    fn holder(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(marked, _)| *marked == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn step_at(pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
        step(pawn, PawnId(0), pose, marks, grid)
    }

    /// Steps a pawn that teleports to each waypoint until it digs, and returns the waypoints
    /// it was sent to and the dig with the feet it was made from.
    fn walk_and_dig(
        pawn: &mut Pawn,
        start: Vec3,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> (Vec<Vec3>, Option<(BlockPos, Vec3, Vec3)>) {
        let mut feet = start;
        let mut waypoints = Vec::new();
        for _ in 0..60 {
            match step_at(pawn, pose_with(feet, 0.0), marks, grid) {
                JobAction::Wait => break,
                JobAction::Walk { to } => {
                    waypoints.push(to);
                    feet = Vec3::new(to.x, feet.y, to.z);
                }
                JobAction::Dig { block, aim } => return (waypoints, Some((block, aim, feet))),
            }
        }
        (waypoints, None)
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let (grid, mut marks) = world(&[]);
        let mut pawn = Pawn::default();

        let action = step_at(&mut pawn, pose_at(3.5, 3.5), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let (mut grid, mut marks) = world(&[[5, 1, 5]]);
        for block in [[4, 1, 5], [6, 1, 5], [5, 1, 4], [5, 1, 6], [5, 2, 5]] {
            put(&mut grid, block);
        }
        let mut pawn = Pawn::default();
        let pose = pose_at(1.5, 5.5);

        let alone = step_at(&mut pawn, pose, &mut marks, &grid);
        let job_alone = pawn.job.clone();
        let stand_cell_is_reachable = standable(&grid, BlockPos::new(4, 2, 5));
        put(&mut grid, [12, 1, 12]);
        marks.toggle([12, 1, 12]);
        step_at(&mut pawn, pose, &mut marks, &grid);

        assert!(
            stand_cell_is_reachable,
            "setup: a cell beside the buried block can be stood on"
        );
        assert_eq!(alone, JobAction::Wait, "a buried block cannot be dug");
        assert_eq!(job_alone, Job::Idle);
        assert_eq!(
            mined_mark(&pawn),
            [12, 1, 12],
            "the pawn takes the exposed block over the nearer buried one"
        );
        assert_eq!(unclaimed(&marks), [[5, 1, 5]]);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let (grid, mut marks) = world(&[[2, 1, 12], [12, 1, 6]]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        let action = step_at(&mut pawn, pose_at(15.5, 6.5), &mut marks, &grid);

        let JobAction::Walk { to } = action else {
            panic!("expected a walk, got {action:?}");
        };
        assert!(
            (to.x - 15.5).abs() <= 1.0 && (to.z - 6.5).abs() < 1e-4,
            "the first step is along the way to the block: {to:?}"
        );
        assert_eq!(
            mined_mark(&pawn),
            [12, 1, 6],
            "the nearer mark, though its key is the later one"
        );
        let Job::Mine { route, .. } = &pawn.job else {
            panic!("the pawn has no job");
        };
        assert!(!route.is_empty());
        assert!(
            route.iter().all(|c| standable(&grid, *c)),
            "the route is over ground: {route:?}"
        );
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
        assert_eq!(holder(&marks, [12, 1, 6]), Some(PawnId(0)));
        assert_eq!(unclaimed(&marks), [[2, 1, 12]]);
    }

    #[test]
    fn marks_at_equal_distance_are_taken_in_key_order() {
        let (grid, mut marks) = world(&[[11, 1, 6], [5, 1, 6]]);
        let mut pawn = Pawn::default();

        step_at(&mut pawn, pose_at(8.5, 6.5), &mut marks, &grid);

        assert_eq!(mined_mark(&pawn), [5, 1, 6], "the lower key wins the tie");
        assert_eq!(unclaimed(&marks), [[11, 1, 6]]);
    }

    /// Four exposed marks across a gap, each nearer than the one block a pawn can walk to.
    fn gap_world() -> (VoxelGrid, MineMarks) {
        let mut grid = VoxelGrid::new(16);
        lay_ground(&mut grid, 0..16, 0..10);
        lay_ground(&mut grid, 10..16, 11..14);
        let mut marks = MineMarks::default();
        for block in [
            [10, 1, 12],
            [11, 1, 12],
            [12, 1, 12],
            [13, 1, 12],
            [3, 1, 6],
        ] {
            put(&mut grid, block);
            marks.toggle(block);
        }
        (grid, marks)
    }

    #[test]
    fn at_most_four_routes_are_tried_a_tick() {
        let (grid, mut marks) = gap_world();
        let mut pawn = Pawn::default();
        let pose = pose_at(11.5, 8.5);

        let first = step_at(&mut pawn, pose, &mut marks, &grid);
        let after_first = pawn.job.clone();
        let mut claimed_on = None;
        for tick in 2..=4 {
            step_at(&mut pawn, pose, &mut marks, &grid);
            if matches!(pawn.job, Job::Mine { .. }) {
                claimed_on = Some(tick);
                break;
            }
        }

        assert_eq!(first, JobAction::Wait, "four failed routes use the tick");
        assert_eq!(after_first, Job::Idle);
        assert_eq!(unclaimed(&marks).len(), 5, "nothing is claimed on tick 1");
        assert!(claimed_on.is_some(), "a later tick reaches the fifth mark");
        assert_eq!(mined_mark(&pawn), [3, 1, 6]);
    }

    #[test]
    fn an_unreachable_nearer_mark_is_skipped_for_a_reachable_one() {
        let mut grid = VoxelGrid::new(16);
        lay_ground(&mut grid, 0..16, 0..10);
        lay_ground(&mut grid, 10..14, 11..14);
        let mut marks = MineMarks::default();
        for block in [[12, 1, 12], [3, 1, 6]] {
            put(&mut grid, block);
            marks.toggle(block);
        }
        let mut pawn = Pawn::default();

        step_at(&mut pawn, pose_at(11.5, 8.5), &mut marks, &grid);

        assert_eq!(
            mined_mark(&pawn),
            [3, 1, 6],
            "the island across the gap is nearer and has the later key but cannot be walked to"
        );
        assert_eq!(unclaimed(&marks), [[12, 1, 12]]);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let (grid, mut marks) = world(&[[8, 1, 5]]);
        let mut pawn = Pawn::default();

        let (waypoints, dug) = walk_and_dig(&mut pawn, Vec3::new(1.5, 1.0, 5.5), &mut marks, &grid);

        let (block, _, feet) = dug.expect("the pawn digs once it has walked");
        assert_eq!(block, BlockPos::new(8, 1, 5));
        assert_eq!(
            (feet.x.floor(), feet.z.floor()),
            (7.0, 5.0),
            "it digs from the nearest stand cell, beside the block"
        );
        assert!(waypoints.len() >= 5, "a walk of six cells: {waypoints:?}");
        for pair in waypoints.windows(2) {
            assert!(
                (pair[1].z - 5.5).abs() < 1e-4 && pair[1].x >= pair[0].x,
                "straight on: {pair:?}"
            );
            assert!(
                (pair[1] - pair[0]).length() <= 1.0 + 1e-4,
                "one cell at a time: {pair:?}"
            );
        }
        let last = waypoints.last().expect("it walked");
        assert!(
            (last.x - 7.5).abs() < 1e-4,
            "it ends at the stand cell: {last:?}"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let cases = [
            (
                Vec3::new(5.5, 1.0, 5.5),
                Vec3::new(6.0, 1.5, 5.5),
                "from the west",
            ),
            (
                Vec3::new(7.5, 1.0, 5.5),
                Vec3::new(7.0, 1.5, 5.5),
                "from the east",
            ),
            (
                Vec3::new(6.5, 1.0, 4.5),
                Vec3::new(6.5, 1.5, 5.0),
                "from the south",
            ),
        ];

        for (feet, expected, side) in cases {
            let (grid, mut marks) = world(&[[6, 1, 5]]);
            let mut pawn = Pawn::default();

            let (_, dug) = walk_and_dig(&mut pawn, feet, &mut marks, &grid);

            let (block, aim, _) = dug.unwrap_or_else(|| panic!("{side}: no dig"));
            assert_eq!(block, BlockPos::new(6, 1, 5), "{side}");
            assert!(
                (aim - expected).length() < 1e-3,
                "{side}: aimed at {aim:?}, not {expected:?}"
            );
        }
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 2i32..14,
            bz in 2i32..14,
            sx in 0i32..16,
            sz in 0i32..16,
        ) {
            prop_assume!(sx != bx || sz != bz);
            let (grid, mut marks) = world(&[[bx, 1, bz]]);
            let mut pawn = Pawn::default();
            let start = Vec3::new(sx as f32 + 0.5, 1.0, sz as f32 + 0.5);

            let (_, dug) = walk_and_dig(&mut pawn, start, &mut marks, &grid);

            let (block, aim, feet) = dug.expect("a block on open ground is dug");
            prop_assert_eq!(block, BlockPos::new(bx, 1, bz));
            let eye = feet + Vec3::Y * EYE_HEIGHT;
            prop_assert!(
                (aim - eye).length() <= MINE_REACH,
                "aim {aim:?} from eye {eye:?} is past the reach"
            );
            prop_assert!(
                (feet.x.floor() as i32 - bx).abs() <= 1 && (feet.z.floor() as i32 - bz).abs() <= 1,
                "stand cell {feet:?} is not beside the block"
            );
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let (grid, mut marks) = world(&[[8, 1, 5]]);
        let mut pawn = Pawn::default();
        let pose = pose_at(1.5, 5.5);
        let walking = step_at(&mut pawn, pose, &mut marks, &grid);
        marks.toggle([8, 1, 5]);

        let action = step_at(&mut pawn, pose, &mut marks, &grid);

        assert!(matches!(walking, JobAction::Walk { .. }), "{walking:?}");
        assert_eq!(action, JobAction::Wait, "nothing is marked any more");
        assert_eq!(pawn.job, Job::Idle);
        assert!(!marks.contains([8, 1, 5]));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let (mut grid, mut marks) = world(&[[8, 1, 5]]);
        let mut pawn = Pawn::default();
        let pose = pose_at(1.5, 5.5);
        step_at(&mut pawn, pose, &mut marks, &grid);
        grid.mutator().remove(BlockPos::new(8, 1, 5));
        marks.block_removed([8, 1, 5]);

        let action = step_at(&mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(!marks.contains([8, 1, 5]));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let (grid, mut marks) = world(&[[8, 1, 5]]);
        let pose = pose_at(1.5, 5.5);
        let mut first = Pawn::default();
        let mut second = Pawn::default();
        step(&mut first, PawnId(0), pose, &mut marks, &grid);

        let action = step(&mut second, PawnId(1), pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(second.job, Job::Idle);
        assert_eq!(holder(&marks, [8, 1, 5]), Some(PawnId(0)));
        assert_eq!(mined_mark(&first), [8, 1, 5]);
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let (grid, mut marks) = world(&[[8, 1, 5]]);
        let mut pawn = Pawn::default();
        let pose = pose_at(1.5, 5.5);

        let mut released_at = None;
        for tick in 1..=STALL_TICKS + 10 {
            step_at(&mut pawn, pose, &mut marks, &grid);
            if pawn.job == Job::Idle {
                released_at = Some(tick);
                break;
            }
        }

        let tick = released_at.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS..=STALL_TICKS + 2).contains(&tick),
            "gave up on tick {tick}, not after {STALL_TICKS} without progress"
        );
        assert!(marks.contains([8, 1, 5]), "the mark stays");
        assert_eq!(
            unclaimed(&marks),
            [[8, 1, 5]],
            "and is free for a pawn to take"
        );
    }

    #[test]
    fn steady_progress_never_stalls_the_walk() {
        let mut grid = VoxelGrid::new(16);
        lay_ground(&mut grid, 0..26, 0..11);
        put(&mut grid, [23, 1, 5]);
        let mut marks = MineMarks::default();
        marks.toggle([23, 1, 5]);
        let mut pawn = Pawn::default();
        let mut feet = Vec3::new(1.5, 1.0, 5.5);

        let mut dug = false;
        for tick in 0..2000 {
            match step_at(&mut pawn, pose_with(feet, 0.0), &mut marks, &grid) {
                JobAction::Walk { to } => {
                    let toward = Vec3::new(to.x - feet.x, 0.0, to.z - feet.z);
                    feet += toward.clamp_length_max(0.05);
                }
                JobAction::Dig { .. } => {
                    dug = true;
                    break;
                }
                JobAction::Wait => panic!("gave up on tick {tick} while still walking"),
            }
        }

        assert!(dug, "a 20 m walk at 0.05 m a tick ends in a dig");
        assert!(
            feet.x > 20.0,
            "the walk covered more than {STALL_TICKS} ticks of ground: {feet:?}"
        );
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let (mut grid, mut marks) = world(&[[5, 1, 5]]);
        let mut pawn = Pawn::default();
        let pose = pose_at(1.5, 5.5);
        step_at(&mut pawn, pose, &mut marks, &grid);
        for z in 0..16 {
            grid.mutator().remove(BlockPos::new(2, 0, z));
        }

        for _ in 0..3 {
            step_at(&mut pawn, pose, &mut marks, &grid);
        }

        assert_eq!(pawn.job, Job::Idle, "the way is gone");
        assert!(marks.contains([5, 1, 5]), "the mark stays");
        assert_eq!(unclaimed(&marks), [[5, 1, 5]]);
    }

    /// One turn a second at 60 ticks a second, in radians a tick.
    const MAX_TURN: f32 = std::f32::consts::TAU / 60.0;

    /// The target `distance` away at `angle` from +Z, on the level of `pose`.
    fn target_at(pose: Pose, angle: f32) -> Vec3 {
        pose.feet + Vec3::new(angle.sin(), 0.0, angle.cos()) * 5.0
    }

    fn shortest_turn(from: f32, to: f32) -> f32 {
        (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let max = MAX_TURN;
        let facing = pose_at(2.5, 2.5);

        let right = steer(facing, target_at(facing, std::f32::consts::FRAC_PI_2));
        let left = steer(facing, target_at(facing, -std::f32::consts::FRAC_PI_2));
        let close = steer(facing, target_at(facing, 0.01));
        let across_the_seam = steer(
            pose_with(Vec3::new(2.5, 1.0, 2.5), std::f32::consts::PI - 0.05),
            target_at(facing, -std::f32::consts::PI + 0.05),
        );

        assert!((right.yaw_delta - max).abs() < 1e-5, "{right:?} vs {max}");
        assert!((left.yaw_delta + max).abs() < 1e-5, "{left:?} vs {max}");
        assert!(
            (close.yaw_delta - 0.01).abs() < 1e-4,
            "a small error is turned out in one tick, not overshot: {close:?}"
        );
        assert!(
            (across_the_seam.yaw_delta - 0.1).abs() < 1e-4,
            "the short way round the half turn: {across_the_seam:?}"
        );
    }

    proptest! {
        #[test]
        fn steer_never_turns_past_the_turn_rate_and_toward_the_nearer_side(
            yaw in -3.0f32..3.0,
            angle in -3.0f32..3.0,
        ) {
            let diff = shortest_turn(yaw, angle);
            prop_assume!((diff.abs() - std::f32::consts::PI).abs() > 0.05 && diff.abs() > 1e-3);
            let pose = pose_with(Vec3::new(2.5, 1.0, 2.5), yaw);

            let walk = steer(pose, target_at(pose, angle));

            prop_assert!(walk.yaw_delta.abs() <= MAX_TURN + 1e-6, "{walk:?}");
            prop_assert!(
                walk.yaw_delta * diff > 0.0,
                "turned {} when the target is {diff} away",
                walk.yaw_delta
            );
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let facing = pose_at(2.5, 2.5);
        let cases = [
            (0.0, 1.0),
            (0.5, 1.0),
            (-0.5, 1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
            (1.1, 0.0),
            (-1.1, 0.0),
            (std::f32::consts::FRAC_PI_2, 0.0),
            (3.0, 0.0),
        ];

        for (angle, forward) in cases {
            let walk = steer(facing, target_at(facing, angle));

            assert_eq!(walk.forward, forward, "target {angle} rad off the facing");
        }
    }
}
