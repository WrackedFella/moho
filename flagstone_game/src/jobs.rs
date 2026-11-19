//! The job loop of a pawn that is not possessed: claim a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// Horizontal distance from a waypoint centre at which the pawn counts as having arrived.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives the job up.
pub const STALL_TICKS: u32 = 120;

/// Turning speed in radians per second: one full turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Detects a walk that is making no progress.
#[derive(Clone, Debug, Default)]
pub struct StallGuard {
    /// Ticks since the pawn last got closer to its waypoint.
    pub ticks: u32,
    /// The closest the pawn has been to its waypoint.
    pub best_distance: f32,
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default)]
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

/// Where a pawn is and which way it faces, from the physics character.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub feet: Vec3,
    /// The eye: the capsule centre.
    pub eye: Vec3,
    /// Heading in radians; forward is `(sin yaw, 0, cos yaw)`.
    pub yaw: f32,
}

/// What the pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward the centre of a waypoint cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: turn by `yaw_delta` radians and walk at `forward` (0 to 1) of full speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances the job of pawn `id` by one tick.
pub fn step(
    _id: PawnId,
    _job: &mut Job,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!("jobs::step")
}

/// Turns toward `to` at [`TURN_RATE`] and walks once within 60 degrees of facing it.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    todo!("jobs::steer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::MINE_REACH;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    const FLOOR_EXTENT: i32 = 12;
    const EYE_HEIGHT: f32 = 1.15;
    const ME: PawnId = PawnId(0);

    fn flat_world() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -FLOOR_EXTENT..=FLOOR_EXTENT {
            for z in -FLOOR_EXTENT..=FLOOR_EXTENT {
                grid.mutator().place(BlockPos::new(x, 0, z), 1, None);
            }
        }
        grid
    }

    fn pose_at(cell: BlockPos) -> Pose {
        let feet = Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5);
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    fn facing(yaw: f32) -> Pose {
        Pose {
            yaw,
            ..pose_at(BlockPos::new(0, 1, 0))
        }
    }

    fn mark_block(grid: &mut VoxelGrid, marks: &mut MineMarks, block: BlockPos) {
        grid.mutator().place(block, 1, Some(7));
        marks.toggle(block.to_array());
    }

    fn mine_job(job: &Job) -> (BlockPos, &[BlockPos], usize) {
        match job {
            Job::Mine {
                mark, route, next, ..
            } => (*mark, route.as_slice(), *next),
            Job::Idle => panic!("expected a mining job"),
        }
    }

    fn claimed_by(marks: &MineMarks, block: BlockPos) -> Option<PawnId> {
        marks
            .iter()
            .find(|(pos, _)| *pos == block.to_array())
            .expect("the block is marked")
            .1
            .claimed_by
    }

    fn start_job(
        grid: &VoxelGrid,
        marks: &mut MineMarks,
        pawn: &mut Pawn,
        from: BlockPos,
    ) -> (Job, JobAction) {
        let mut job = Job::Idle;
        let action = step(ME, &mut job, pawn, pose_at(from), marks, grid);
        (job, action)
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < 1e-3,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = flat_world();
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let (job, action) = start_job(&grid, &mut marks, &mut pawn, BlockPos::new(0, 1, 0));

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert_eq!(pawn.equipped_tool, None, "an idle pawn changes nothing");
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = flat_world();
        for x in 4..=6 {
            for y in 1..=3 {
                for z in -1..=1 {
                    grid.mutator().place(BlockPos::new(x, y, z), 1, None);
                }
            }
        }
        let buried = BlockPos::new(5, 2, 0);
        let mut marks = MineMarks::default();
        marks.toggle(buried.to_array());
        let mut pawn = Pawn::default();

        let (job, action) = start_job(&grid, &mut marks, &mut pawn, BlockPos::new(0, 1, 0));

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert_eq!(claimed_by(&marks, buried), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let near = BlockPos::new(4, 1, 0);
        let far = BlockPos::new(9, 1, 0);
        mark_block(&mut grid, &mut marks, far);
        mark_block(&mut grid, &mut marks, near);
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let (job, action) = start_job(&grid, &mut marks, &mut pawn, BlockPos::new(0, 1, 0));

        let (mark, route, _) = mine_job(&job);
        assert_eq!(mark, near);
        assert!(!route.is_empty());
        assert_eq!(claimed_by(&marks, near), Some(ME));
        assert_eq!(claimed_by(&marks, far), None);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert!(
            matches!(action, JobAction::Wait | JobAction::Walk { .. }),
            "the block is too far to dig from the start: {action:?}"
        );
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let block = BlockPos::new(5, 1, 0);
        mark_block(&mut grid, &mut marks, block);
        let mut pawn = Pawn::default();
        let start = BlockPos::new(0, 1, 0);
        let mut pose = pose_at(start);
        let mut job = Job::Idle;
        let mut walks = 0;

        let dug =
            (0..60).find_map(
                |_| match step(ME, &mut job, &mut pawn, pose, &mut marks, &grid) {
                    JobAction::Wait => None,
                    JobAction::Walk { to } => {
                        walks += 1;
                        assert!(
                            (to.x.fract() - 0.5).abs() < 1e-3,
                            "x is a cell centre: {to}"
                        );
                        assert!(
                            (to.z.fract() - 0.5).abs() < 1e-3,
                            "z is a cell centre: {to}"
                        );
                        pose.feet.x = to.x;
                        pose.feet.z = to.z;
                        pose.eye = pose.feet + Vec3::Y * EYE_HEIGHT;
                        None
                    }
                    JobAction::Dig { block, .. } => Some(block),
                },
            );

        assert_eq!(dug, Some(block));
        assert!(walks >= 3, "walked {walks} waypoints");
        let stand = crate::routing::feet_cell(pose.feet);
        assert!((stand.x - block.x).abs() <= 1 && (stand.z - block.z).abs() <= 1);
        assert!((-1..=2).contains(&(stand.y - block.y)));
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let block = BlockPos::new(4, 1, 0);
        let cases = [
            (BlockPos::new(3, 1, 0), Vec3::new(4.0, 1.5, 0.5)),
            (BlockPos::new(4, 1, -1), Vec3::new(4.5, 1.5, 0.0)),
        ];

        for (stand, expected_aim) in cases {
            let mut grid = flat_world();
            let mut marks = MineMarks::default();
            mark_block(&mut grid, &mut marks, block);
            assert!(marks.claim(block.to_array(), ME));
            let mut pawn = Pawn::default();
            let mut job = Job::Mine {
                mark: block,
                route: vec![stand],
                next: 1,
                stall: StallGuard::default(),
            };

            let action = step(ME, &mut job, &mut pawn, pose_at(stand), &mut marks, &grid);

            match action {
                JobAction::Dig { block: dug, aim } => {
                    assert_eq!(dug, block);
                    assert_near(aim, expected_aim);
                }
                other => panic!("expected a dig from {stand}, got {other:?}"),
            }
        }
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in -9..=9i32, bz in -9..=9i32, stacked in any::<bool>(),
        ) {
            prop_assume!(bx.abs().max(bz.abs()) >= 2);
            let mut grid = flat_world();
            let mut marks = MineMarks::default();
            let block = BlockPos::new(bx, if stacked { 2 } else { 1 }, bz);
            if stacked {
                grid.mutator().place(BlockPos::new(bx, 1, bz), 1, None);
            }
            mark_block(&mut grid, &mut marks, block);
            let mut pawn = Pawn::default();

            let (job, _) = start_job(&grid, &mut marks, &mut pawn, BlockPos::new(0, 1, 0));

            let (mark, route, _) = mine_job(&job);
            prop_assert_eq!(mark, block);
            let stand = *route.last().expect("a route to a stand cell");
            prop_assert!((stand.x - block.x).abs() <= 1 && (stand.z - block.z).abs() <= 1);
            prop_assert!((-1..=2).contains(&(stand.y - block.y)));
            let eye = Vec3::new(stand.x as f32 + 0.5, stand.y as f32 + EYE_HEIGHT, stand.z as f32 + 0.5);
            let centre = block.as_vec3() + Vec3::splat(0.5);
            prop_assert!(eye.distance(centre) <= MINE_REACH);
        }
    }

    fn walking_job(grid: &mut VoxelGrid, marks: &mut MineMarks) -> (Job, Pawn, BlockPos) {
        let block = BlockPos::new(9, 1, 0);
        mark_block(grid, marks, block);
        let mut pawn = Pawn::default();
        let (job, _) = start_job(grid, marks, &mut pawn, BlockPos::new(0, 1, 0));
        assert!(matches!(job, Job::Mine { .. }));
        (job, pawn, block)
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let (mut job, mut pawn, block) = walking_job(&mut grid, &mut marks);
        marks.toggle(block.to_array());

        let action = step(
            ME,
            &mut job,
            &mut pawn,
            pose_at(BlockPos::new(1, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(
            grid.is_solid_at(block),
            "cancelling does not remove the block"
        );
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let (mut job, mut pawn, block) = walking_job(&mut grid, &mut marks);
        grid.mutator().remove(block);
        marks.block_removed(block.to_array());

        let action = step(
            ME,
            &mut job,
            &mut pawn,
            pose_at(BlockPos::new(1, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(!marks.contains(block.to_array()));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let block = BlockPos::new(4, 1, 0);
        mark_block(&mut grid, &mut marks, block);
        assert!(marks.claim(block.to_array(), PawnId(1)));
        let mut pawn = Pawn::default();

        let (job, action) = start_job(&grid, &mut marks, &mut pawn, BlockPos::new(0, 1, 0));

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert_eq!(claimed_by(&marks, block), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let (mut job, mut pawn, block) = walking_job(&mut grid, &mut marks);
        let stuck = pose_at(BlockPos::new(0, 1, 0));
        let budget = STALL_TICKS as usize;

        let released_at = (1..=budget + 3).find(|_| {
            let action = step(ME, &mut job, &mut pawn, stuck, &mut marks, &grid);
            matches!(job, Job::Idle) && {
                assert_eq!(action, JobAction::Wait);
                true
            }
        });

        let tick = released_at.expect("a pawn that never moves gives up");
        assert!(tick + 1 >= budget, "gave up after {tick} ticks, too early");
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, block), None);
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = flat_world();
        let mut marks = MineMarks::default();
        let (mut job, mut pawn, block) = walking_job(&mut grid, &mut marks);
        let (_, route, next) = mine_job(&job);
        let waypoint = route[next];
        grid.mutator().remove(waypoint - BlockPos::Y);

        let action = step(
            ME,
            &mut job,
            &mut pawn,
            pose_at(BlockPos::new(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(job, Job::Idle));
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert_eq!(claimed_by(&marks, block), None);
    }

    fn toward_yaw(yaw: f32) -> Vec3 {
        let feet = facing(0.0).feet;
        feet + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 5.0
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let max_turn = TURN_RATE / TICK_HZ as f32;

        let left_of_target = steer(facing(0.0), toward_yaw(FRAC_PI_2));
        let right_of_target = steer(facing(0.0), toward_yaw(-FRAC_PI_2));
        let behind = steer(facing(0.0), toward_yaw(PI));
        let nearly_there = steer(facing(0.0), toward_yaw(0.01));
        let across_the_wrap = steer(facing(3.0), toward_yaw(-3.0));

        assert!(
            (left_of_target.yaw_delta - max_turn).abs() < 1e-4,
            "clamped to the rate"
        );
        assert!((right_of_target.yaw_delta + max_turn).abs() < 1e-4);
        assert!(behind.yaw_delta.abs() <= max_turn + 1e-4);
        assert!(
            behind.yaw_delta.abs() > 0.0,
            "turns in place when facing away"
        );
        assert!(
            (nearly_there.yaw_delta - 0.01).abs() < 1e-4,
            "does not overshoot"
        );
        assert!(
            across_the_wrap.yaw_delta > 0.0,
            "takes the short way around"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let walks = |off_by: f32| steer(facing(0.0), toward_yaw(off_by)).forward;

        assert!((walks(0.0) - 1.0).abs() < 1e-4, "full speed when facing it");
        assert!((walks(30f32.to_radians()) - 1.0).abs() < 1e-4);
        assert!((walks(-50f32.to_radians()) - 1.0).abs() < 1e-4);
        assert_eq!(walks(70f32.to_radians()), 0.0);
        assert_eq!(walks(FRAC_PI_2), 0.0);
        assert_eq!(walks(PI), 0.0);
    }
}
