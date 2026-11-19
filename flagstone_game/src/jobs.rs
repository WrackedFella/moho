//! A pawn's own work: claiming a marked block, walking to it and digging it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How close, horizontally, the feet must get to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward the current waypoint before a walk is given up.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second: one full turn.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// What a pawn is doing on its own account.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The marked block the pawn holds the claim on.
        mark: [i32; 3],
        /// Standable cells from the pawn's feet to the stand cell.
        route: Vec<BlockPos>,
        /// Index of the waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// Counts ticks a walk has gone without getting closer to its waypoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    pub ticks: u32,
}

/// Where a pawn stands and looks, from the physics character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    /// The capsule centre, from where a dig is aimed.
    pub eye: Vec3,
    /// Facing in radians; 0 faces +Z and positive turns toward +X.
    pub yaw: f32,
}

/// What the pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of a waypoint cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block`, aiming at `aim` on its surface.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: turn by `yaw_delta` radians and move `forward`, from -1 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances the job of pawn `id` one tick and says what it does. Claims, releases and
/// equips as the job requires; the grid is only read.
pub fn step(
    _id: PawnId,
    _job: &mut Job,
    _pawn: &mut Pawn,
    _pose: &Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!()
}

/// How to move from `pose` toward `to` in one tick.
pub fn steer(_pose: &Pose, _to: Vec3) -> WalkStep {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::colony::MineMark;
    use crate::pawn::MINE_REACH;
    use crate::selection::PAWN_HALF_EXTENTS;
    use proptest::prelude::*;
    use std::ops::RangeInclusive;

    const ID: PawnId = PawnId(0);
    const FREE: Option<MineMark> = Some(MineMark { claimed_by: None });

    fn held_by(pawn: PawnId) -> Option<MineMark> {
        Some(MineMark {
            claimed_by: Some(pawn),
        })
    }

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    /// A one-block-thick floor at `y = 0`, so feet stand in cells at `y = 1`.
    fn floor(xs: RangeInclusive<i32>, zs: RangeInclusive<i32>) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in xs {
            for z in zs.clone() {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    /// A pawn standing at the centre of `cell`, facing +Z.
    fn standing_in(cell: BlockPos) -> Pose {
        let feet = Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5);
        Pose {
            feet,
            eye: feet + Vec3::Y * PAWN_HALF_EXTENTS.y,
            yaw: 0.0,
        }
    }

    fn horizontal(v: Vec3) -> (f32, f32) {
        (v.x, v.z)
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    /// One pawn, its marks and the world it works in.
    struct Rig {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
        pose: Pose,
    }

    impl Rig {
        fn new(grid: VoxelGrid, standing: BlockPos) -> Self {
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::Idle,
                pose: standing_in(standing),
            }
        }

        fn mark(&mut self, x: i32, y: i32, z: i32) {
            self.marks.toggle([x, y, z]);
        }

        fn step(&mut self) -> JobAction {
            step(
                ID,
                &mut self.job,
                &mut self.pawn,
                &self.pose,
                &mut self.marks,
                &self.grid,
            )
        }

        /// Steps once and carries the pawn to where a walk asked it to go.
        fn step_and_walk(&mut self) -> JobAction {
            let action = self.step();
            if let JobAction::Walk { to } = action {
                self.pose.feet.x = to.x;
                self.pose.feet.z = to.z;
                self.pose.eye = self.pose.feet + Vec3::Y * PAWN_HALF_EXTENTS.y;
            }
            action
        }

        /// Steps, walking as asked, until the pawn digs; the actions that were not waits.
        fn work_until_the_dig(&mut self, limit: usize) -> Vec<JobAction> {
            let mut actions = Vec::new();
            for _ in 0..limit {
                let action = self.step_and_walk();
                if action != JobAction::Wait {
                    actions.push(action);
                }
                if matches!(action, JobAction::Dig { .. }) {
                    break;
                }
            }
            actions
        }

        /// The mark on `pos`, if there is one.
        fn claim_of(&self, pos: [i32; 3]) -> Option<MineMark> {
            self.marks
                .iter()
                .find(|(p, _)| *p == pos)
                .map(|(_, mark)| *mark)
        }

        fn mined_mark(&self) -> Option<[i32; 3]> {
            match &self.job {
                Job::Mine { mark, .. } => Some(*mark),
                Job::Idle => None,
            }
        }
    }

    /// A flat floor with a mark on the surface block at `(5, 0, 2)`, pawn at `(2, 1, 2)`.
    fn rig_with_a_near_mark() -> Rig {
        let mut rig = Rig::new(floor(0..=16, 0..=8), cell(2, 1, 2));
        rig.mark(5, 0, 2);
        rig
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut rig = Rig::new(floor(0..=10, 0..=10), cell(2, 1, 2));
        rig.pawn.select_slot(3);

        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(rig.job, Job::Idle);
        assert_eq!(
            rig.pawn.equipped_tool, None,
            "nothing to dig, no tool drawn"
        );
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = VoxelGrid::new(16);
        for x in 3..=7 {
            for z in 3..=7 {
                for y in 0..=2 {
                    place(&mut grid, x, y, z);
                }
            }
        }
        let mut rig = Rig::new(grid, cell(4, 3, 4));
        rig.mark(5, 1, 5);

        let buried = rig.step();

        assert_eq!(buried, JobAction::Wait);
        assert_eq!(rig.job, Job::Idle);
        assert_eq!(rig.claim_of([5, 1, 5]), FREE, "the mark stays free");

        rig.mark(7, 2, 7);
        rig.step();

        assert_eq!(
            rig.mined_mark(),
            Some([7, 2, 7]),
            "the exposed mark is taken though the buried one is nearer"
        );
        assert_eq!(rig.claim_of([5, 1, 5]), FREE);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut rig = rig_with_a_near_mark();
        rig.mark(12, 0, 2);
        rig.pawn.select_slot(3);

        rig.step();

        let Job::Mine { mark, route, .. } = &rig.job else {
            panic!("expected a mine job, got {:?}", rig.job);
        };
        assert_eq!(*mark, [5, 0, 2], "the nearer mark");
        assert_eq!(route.last(), Some(&cell(4, 1, 2)), "the nearest stand cell");
        assert_eq!(rig.claim_of([5, 0, 2]), held_by(ID));
        assert_eq!(rig.claim_of([12, 0, 2]), FREE);
        assert_eq!(rig.pawn.selected_slot, crate::pawn::TOOL_SLOT);
        assert_eq!(rig.pawn.equipped_tool, Some(crate::tools::STARTING_TOOL));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut rig = rig_with_a_near_mark();

        let actions = rig.work_until_the_dig(60);

        let walks: Vec<(f32, f32)> = actions
            .iter()
            .filter_map(|a| match a {
                JobAction::Walk { to } => Some(horizontal(*to)),
                _ => None,
            })
            .collect();
        let mut waypoints = walks.clone();
        waypoints.dedup();
        assert_eq!(waypoints, [(3.5, 2.5), (4.5, 2.5)]);
        let Some(JobAction::Dig { block, .. }) = actions.last() else {
            panic!("the pawn never dug: {actions:?}");
        };
        assert_eq!(*block, cell(5, 0, 2));
        assert_eq!(
            horizontal(rig.pose.feet),
            (4.5, 2.5),
            "it digs from the stand cell, not before"
        );
    }

    #[test]
    fn a_walking_pawn_that_makes_progress_never_stalls() {
        let mut rig = Rig::new(floor(0..=200, 0..=4), cell(0, 1, 2));
        rig.mark(170, 0, 2);

        let actions = rig.work_until_the_dig(1000);

        assert!(
            matches!(actions.last(), Some(JobAction::Dig { .. })),
            "a walk of 170 ticks gave up: {:?}",
            actions.last()
        );
    }

    fn raised_block_rig(standing: BlockPos) -> Rig {
        let mut rig = Rig::new(floor(0..=10, 0..=8), standing);
        place(&mut rig.grid, 5, 1, 2);
        rig.mark(5, 1, 2);
        rig
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut west = raised_block_rig(cell(4, 1, 2));
        let mut south = raised_block_rig(cell(5, 1, 3));

        let from_the_west = west.work_until_the_dig(20);
        let from_the_south = south.work_until_the_dig(20);

        let Some(JobAction::Dig { block, aim }) = from_the_west.last() else {
            panic!("no dig from the west: {from_the_west:?}");
        };
        assert_eq!(*block, cell(5, 1, 2));
        assert!(
            near(*aim, Vec3::new(5.0, 1.5, 2.5)),
            "west face, got {aim:?}"
        );
        let Some(JobAction::Dig { block, aim }) = from_the_south.last() else {
            panic!("no dig from the south: {from_the_south:?}");
        };
        assert_eq!(*block, cell(5, 1, 2));
        assert!(
            near(*aim, Vec3::new(5.5, 1.5, 3.0)),
            "south face, got {aim:?}"
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 4..=11i32,
            bz in 4..=11i32,
            sx in 0..=15i32,
            sz in 0..=15i32,
        ) {
            let mut rig = Rig::new(floor(0..=15, 0..=15), cell(sx, 1, sz));
            rig.mark(bx, 0, bz);

            let actions = rig.work_until_the_dig(200);

            let Some(JobAction::Dig { block, aim }) = actions.last() else {
                return Err(TestCaseError::fail(format!("never dug: {actions:?}")));
            };
            prop_assert_eq!(*block, cell(bx, 0, bz));
            let stand = crate::routing::feet_cell(rig.pose.feet);
            prop_assert!((stand.x - bx).abs() <= 1 && (stand.z - bz).abs() <= 1, "stood at {stand:?}");
            prop_assert!((-1..=2).contains(&stand.y), "stood at {stand:?}");
            prop_assert!(
                (*aim - rig.pose.eye).length() <= MINE_REACH,
                "eye {:?} to aim {aim:?}",
                rig.pose.eye
            );
            let centre = Vec3::new(bx as f32 + 0.5, 0.5, bz as f32 + 0.5);
            prop_assert!((*aim - centre).length() <= 0.5 + 1e-4, "aim {aim:?} is off the block");
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut rig = rig_with_a_near_mark();
        let walking: Vec<JobAction> = (0..2).map(|_| rig.step_and_walk()).collect();
        assert!(
            walking.iter().any(|a| matches!(a, JobAction::Walk { .. })),
            "setup: the pawn is walking: {walking:?}"
        );

        rig.marks.toggle([5, 0, 2]);
        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(rig.job, Job::Idle);
        assert!(!rig.marks.contains([5, 0, 2]));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut rig = rig_with_a_near_mark();
        rig.mark(12, 0, 2);
        rig.step_and_walk();
        assert_eq!(rig.mined_mark(), Some([5, 0, 2]), "setup: claimed");

        rig.grid.mutator().remove(cell(5, 0, 2));
        rig.marks.block_removed([5, 0, 2]);
        let actions: Vec<JobAction> = (0..3).map(|_| rig.step_and_walk()).collect();

        assert!(
            actions.iter().all(|a| !matches!(a, JobAction::Dig { .. })),
            "dug a block that is gone: {actions:?}"
        );
        assert_eq!(
            rig.mined_mark(),
            Some([12, 0, 2]),
            "the pawn moved on to the remaining mark"
        );
        assert_eq!(rig.claim_of([5, 0, 2]), None);
        assert_eq!(rig.claim_of([12, 0, 2]), held_by(ID));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut rig = rig_with_a_near_mark();
        assert!(rig.marks.claim([5, 0, 2], PawnId(1)));
        rig.pawn.select_slot(3);

        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(rig.job, Job::Idle);
        assert_eq!(rig.claim_of([5, 0, 2]), held_by(PawnId(1)), "still held");
        assert_eq!(rig.pawn.equipped_tool, None);
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut rig = rig_with_a_near_mark();
        let mut released_at = None;

        for tick in 1..=STALL_TICKS + 5 {
            let action = rig.step();
            assert!(
                !matches!(action, JobAction::Dig { .. }),
                "dug without arriving, tick {tick}"
            );
            if rig.job == Job::Idle {
                released_at = Some(tick);
                break;
            }
        }

        let tick = released_at.expect("a pawn that never moves gives up");
        assert!(
            tick > STALL_TICKS / 2 && tick <= STALL_TICKS + 2,
            "gave up at tick {tick}"
        );
        assert!(rig.marks.contains([5, 0, 2]), "the mark stays");
        assert_eq!(rig.claim_of([5, 0, 2]), FREE, "the claim does not");
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut rig = Rig::new(floor(2..=10, 2..=2), cell(2, 1, 2));
        rig.mark(5, 0, 2);
        rig.step_and_walk();
        assert_eq!(rig.mined_mark(), Some([5, 0, 2]), "setup: claimed");

        rig.grid.mutator().remove(cell(3, 0, 2));
        rig.grid.mutator().remove(cell(4, 0, 2));
        let actions: Vec<JobAction> = (0..5).map(|_| rig.step_and_walk()).collect();

        assert_eq!(rig.job, Job::Idle);
        for action in &actions {
            if let JobAction::Walk { to } = action {
                assert!(to.x < 3.0, "walked toward a cell with no ground: {to:?}");
            }
        }
        assert!(rig.marks.contains([5, 0, 2]), "the mark stays");
        assert_eq!(rig.claim_of([5, 0, 2]), FREE);
    }

    fn facing(yaw: f32) -> Pose {
        Pose {
            yaw,
            ..standing_in(cell(0, 1, 0))
        }
    }

    /// A point five metres from the origin cell's feet, `bearing` radians from +Z toward +X.
    fn bearing_target(bearing: f32) -> Vec3 {
        let feet = standing_in(cell(0, 1, 0)).feet;
        feet + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * 5.0
    }

    fn max_turn() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        assert!((TURN_RATE - TAU).abs() < 1e-6, "one turn per second");
        let small = 5.0_f32.to_radians();

        let right = steer(&facing(0.0), bearing_target(FRAC_PI_2)).yaw_delta;
        let left = steer(&facing(0.0), bearing_target(-FRAC_PI_2)).yaw_delta;
        let behind = steer(&facing(0.0), bearing_target(PI)).yaw_delta;
        let nudge = steer(&facing(0.0), bearing_target(small)).yaw_delta;
        let nudge_left = steer(&facing(0.0), bearing_target(-small)).yaw_delta;
        let across_the_seam = steer(&facing(3.0), bearing_target(-3.0)).yaw_delta;
        let across_the_seam_short = steer(&facing(3.1), bearing_target(-3.1)).yaw_delta;
        let ahead = steer(&facing(0.4), bearing_target(0.4)).yaw_delta;

        assert!(
            (right - max_turn()).abs() < 1e-5,
            "a quarter turn is clamped: {right}"
        );
        assert!((left + max_turn()).abs() < 1e-5, "{left}");
        assert!(behind.abs() > max_turn() - 1e-5 && behind.abs() < max_turn() + 1e-5);
        assert!(
            (nudge - small).abs() < 1e-4,
            "a small error is closed exactly: {nudge}"
        );
        assert!((nudge_left + small).abs() < 1e-4, "{nudge_left}");
        assert!(
            (across_the_seam - max_turn()).abs() < 1e-5,
            "the short way round from 3.0 to -3.0 is up: {across_the_seam}"
        );
        assert!(
            (across_the_seam_short - (TAU - 6.2)).abs() < 1e-4,
            "{across_the_seam_short}"
        );
        assert!(ahead.abs() < 1e-5, "already facing it: {ahead}");
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let at = |bearing_degrees: f32| {
            steer(&facing(0.0), bearing_target(bearing_degrees.to_radians())).forward
        };

        assert_eq!(at(0.0), 1.0, "dead ahead");
        assert_eq!(at(59.0), 1.0);
        assert_eq!(at(-59.0), 1.0);
        assert_eq!(at(61.0), 0.0, "turn in place");
        assert_eq!(at(-61.0), 0.0);
        assert_eq!(at(180.0), 0.0);
    }
}
