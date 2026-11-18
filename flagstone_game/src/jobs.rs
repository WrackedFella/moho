//! What an unpossessed pawn does: claim a marked block, walk to it, dig it.

use std::f32::consts::TAU;

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

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
#[derive(Clone, Debug, Default, PartialEq)]
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

/// Whether a pawn standing with its feet in `cell` can dig `block` from there.
pub fn is_stand_cell(_grid: &VoxelGrid, _block: BlockPos, _cell: BlockPos) -> bool {
    false
}

/// Advances `pawn`'s job one tick.
pub fn step(
    _pawn_id: PawnId,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    JobAction::Wait
}

/// Turn and walk to head for `to` over a tick of `dt` seconds.
pub fn steer(_pose: Pose, _to: Vec3, _dt: f32) -> WalkStep {
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
        for x in 4..=6 {
            for y in 1..=3 {
                for z in 4..=6 {
                    place(&mut grid, x, y, z);
                }
            }
        }
        place(&mut grid, 10, 1, 10);
        let buried = [5, 2, 5];
        let open = [10, 1, 10];
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
        let mut grid = floor(12, 10);
        place(&mut grid, 8, 1, 4);
        let mut marks = marked(&[[8, 1, 4]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(1, 1, 4);
        for _ in 0..3 {
            step(ME, &mut pawn, pose, &mut marks, &grid);
        }
        assert!(is_mining(&pawn), "walking to the mark");
        marks.toggle([8, 1, 4]);

        let action = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(
            !marks.contains([8, 1, 4]),
            "a released claim does not bring the mark back"
        );
        assert!(grid.is_solid_at(BlockPos::new(8, 1, 4)), "the block stays");
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = floor(12, 10);
        place(&mut grid, 8, 1, 4);
        let mut marks = marked(&[[8, 1, 4]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(1, 1, 4);
        for _ in 0..3 {
            step(ME, &mut pawn, pose, &mut marks, &grid);
        }
        assert!(is_mining(&pawn), "walking to the mark");
        grid.mutator().remove(BlockPos::new(8, 1, 4));
        marks.block_removed([8, 1, 4]);

        let action = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
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
        let pose = pose_in(0, 1, 0);
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
        let pose = pose_in(0, 1, 0);
        let at = |degrees: f32| {
            let angle = degrees.to_radians();
            Vec3::new(angle.sin() * 5.0, 1.0, angle.cos() * 5.0)
        };

        let ahead = steer(pose, at(0.0), DT);
        let close_enough = steer(pose, at(59.0), DT);
        let too_far_round = steer(pose, at(61.0), DT);

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
        assert!(
            !is_stand_cell(&low, block_high_up, BlockPos::new(4, 1, 5)),
            "two below"
        );
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
