//! A pawn's own work: taking a marked block, walking to it and digging it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

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
pub fn is_stand_cell(_block: BlockPos, _cell: BlockPos) -> bool {
    false
}

/// Advances the job of pawn `id` by one tick.
///
/// An idle pawn claims the nearest reachable unclaimed mark, equips its tool and starts walking;
/// a mining pawn walks its route and digs from the stand cell. A job whose mark is gone, whose
/// next waypoint is no longer standable, or which has stalled for [`STALL_TICKS`] is released
/// (the mark stays) and the pawn goes idle.
pub fn step(
    _job: &mut Job,
    _id: PawnId,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    JobAction::Wait
}

/// Turns toward `to` at [`TURN_RATE`] (at most one tick's worth) and walks at full speed once
/// within 60 degrees of facing it, else turns in place.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    WalkStep {
        yaw_delta: 0.0,
        forward: 0.0,
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
        let mut grid = ground();
        for x in 4..=6 {
            for y in 1..=3 {
                for z in -1..=1 {
                    place(&mut grid, &[[x, y, z]]);
                }
            }
        }
        let mut marks = marked(&[[5, 2, 0]]);
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
        assert_eq!(claimed_by(&marks, [5, 2, 0]), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = ground();
        place(&mut grid, &[[9, 1, 0], [5, 1, 0]]);
        let mut marks = marked(&[[9, 1, 0], [5, 1, 0]]);
        let mut job = Job::Idle;
        let mut pawn = Pawn::default();
        pawn.select_slot(3);

        step(
            &mut job,
            ME,
            &mut pawn,
            at(cell(0, 1, 0)),
            &mut marks,
            &grid,
        );

        assert_eq!(mining(&job), Some([5, 1, 0]));
        assert_eq!(claimed_by(&marks, [5, 1, 0]), Some(ME));
        assert_eq!(claimed_by(&marks, [9, 1, 0]), None);
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
            error in -3.0f32..3.0,
            distance in 1.0f32..10.0,
        ) {
            let per_tick = TURN_RATE / TICK_HZ as f32;
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

    #[test]
    fn steer_turns_the_short_way_round() {
        let per_tick = TURN_RATE / TICK_HZ as f32;
        let facing_north = Pose {
            feet: Vec3::ZERO,
            yaw: 0.0,
        };

        let left = steer(facing_north, Vec3::new(-5.0, 0.0, 0.0));
        let right = steer(facing_north, Vec3::new(5.0, 0.0, 0.0));
        let behind = steer(facing_north, Vec3::new(0.0, 0.0, -5.0));

        assert!((left.yaw_delta + per_tick).abs() < 1e-5, "{left:?}");
        assert!((right.yaw_delta - per_tick).abs() < 1e-5, "{right:?}");
        assert!(
            (behind.yaw_delta.abs() - per_tick).abs() < 1e-5,
            "{behind:?}"
        );
        assert_eq!(
            (left.forward, right.forward, behind.forward),
            (0.0, 0.0, 0.0)
        );
    }
}
