//! What an idle pawn does on its own: claim a marked block, walk to it, dig it.
//!
//! The loop reads the marks, the grid and the pawn's pose and answers with one action per
//! tick; the caller applies the action through the pawn's own movement and mining, so the AI
//! has no powers a possessed pawn lacks.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How near the feet must come to a waypoint's centre, horizontally, to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks a pawn may go without closing on its waypoint before it gives its mark up.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// How far above the feet the capsule centre, where the eye is, sits.
pub const EYE_HEIGHT: f32 = 1.15;

/// Marks an idle pawn tries to route to in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn is: its feet and which way it faces (yaw 0 faces +Z, positive turns toward +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Progress bookkeeping for a walk, so a pawn that is stuck gives up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    /// The closest the feet have been to the current waypoint.
    pub closest: f32,
    /// Ticks since the feet last came closer.
    pub ticks_without_progress: u32,
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
pub fn within_stand_window(_block: BlockPos, _cell: BlockPos) -> bool {
    todo!("the stand window around a block")
}

/// Advances the job of pawn `id` by one tick.
///
/// An idle pawn claims the nearest marked block it can route to, equips its tool and starts
/// walking. A mining pawn walks its route and then digs. A pawn whose mark is gone, whose
/// waypoint can no longer be stood on, or that stops making progress, drops its job; the
/// mark stays for others unless it is gone.
pub fn step(
    _id: PawnId,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!("the job loop")
}

/// Turns toward `to` at [`TURN_RATE`] and walks once within 60 degrees of facing it.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    todo!("steering toward a waypoint")
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
