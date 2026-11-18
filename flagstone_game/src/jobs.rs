//! A pawn's own work: the mine job loop and the steering that walks its route.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// Horizontal distance from a waypoint centre, in world units, at which a pawn counts as
/// having reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// Fastest turn, in radians per second: one full turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Counts ticks a walking pawn has made no progress toward its waypoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    best_distance: Option<f32>,
    stalled_ticks: u32,
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
    _id: PawnId,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!("step")
}

/// Turns toward `to` at no more than `TURN_RATE` per tick and walks at full speed once
/// within 60 degrees of facing it, else turns in place.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    todo!("steer")
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
            for y in 1..=3 {
                for z in -1..=1 {
                    grid.mutator().place(cell(x, y, z), 1, None);
                }
            }
        }
        marks.toggle([3, 2, 0]);
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
        assert_eq!(claimed_by(&marks, [3, 2, 0]), None);
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

        for _ in 0..(2 * STALL_TICKS) {
            if let JobAction::Walk { to } = step(PawnId(0), &mut pawn, at, &mut marks, &grid) {
                let toward = (to - at.feet).normalize_or_zero() * 0.01;
                at = pose(at.feet + toward, at.yaw);
            }
        }

        assert!(matches!(pawn.job, Job::Mine { .. }), "still walking");
        assert_eq!(claimed_by(&marks, [7, 1, 0]), Some(PawnId(0)));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let (mut grid, mut marks) = world_with_marks(&[[3, 1, 0]]);
        let mut pawn = Pawn::default();
        let start = standing_in(cell(0, 1, 0));
        let waypoint = loop {
            match step(PawnId(0), &mut pawn, start, &mut marks, &grid) {
                JobAction::Walk { to } if feet_cell(to) != cell(0, 1, 0) => break feet_cell(to),
                JobAction::Walk { .. } => {}
                other => panic!("expected a walk, got {other:?}"),
            }
        };
        grid.mutator()
            .place(cell(waypoint.x, waypoint.y + 2, waypoint.z), 1, None);

        let action = step(PawnId(0), &mut pawn, start, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[3, 1, 0]]);
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
