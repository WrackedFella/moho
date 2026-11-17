//! The pawn's job loop: claim a marked block, route to it over the ground, walk the
//! route and dig from inside the player's mining reach.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it, m.
pub const ARRIVE_RADIUS: f32 = 0.35;
/// Ticks without progress toward a waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;
/// Fastest turn, radians per second: one full turn a second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;
/// Route searches an idle pawn starts in one tick.
pub const MAX_ROUTE_ATTEMPTS: usize = 4;

/// Where a pawn stands and which way it faces: yaw 0 faces +Z, positive yaw turns toward +X.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Tracks whether a walk toward a waypoint is still getting closer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard;

/// What a pawn is doing.
#[derive(Debug, Default)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the feet position at the centre of a route cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of turning and walking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Turn this tick, radians.
    pub yaw_delta: f32,
    /// Move axis along the facing, `0.0` to `1.0`.
    pub forward: f32,
}

/// Whether feet in `cell` are inside the window a pawn digs `block` from: within one column
/// of it in x and z, and from one cell below to two above it.
pub fn is_stand_cell(_block: BlockPos, _cell: BlockPos) -> bool {
    todo!()
}

/// Advances `who`'s `job` one tick: claims, routes, walks and digs. See the module docs.
pub fn step(
    _job: &mut Job,
    _who: PawnId,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!()
}

/// The turn and walk that take a pawn at `pose` toward `to`.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const PAWN: PawnId = PawnId(0);
    /// The pawn's eye: the capsule centre, half the capsule's height above the feet.
    const EYE_HEIGHT: f32 = 1.15;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// The feet position at the centre of `cell`.
    fn centre(cell: BlockPos) -> Vec3 {
        Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
    }

    fn flat_ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for z in 0..depth {
                grid.mutator().place(cell(x, 0, z), 0, None);
            }
        }
        grid
    }

    struct Rig {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
        pose: Pose,
    }

    impl Rig {
        fn on(grid: VoxelGrid, standing: BlockPos) -> Self {
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::Idle,
                pose: Pose {
                    feet: centre(standing),
                    yaw: 0.0,
                },
            }
        }

        fn mark(&mut self, block: BlockPos) {
            self.grid.mutator().place(block, 0, None);
            self.marks.toggle(block.to_array());
        }

        fn step(&mut self) -> JobAction {
            step(
                &mut self.job,
                PAWN,
                &mut self.pawn,
                self.pose,
                &mut self.marks,
                &self.grid,
            )
        }

        /// Steps, teleporting onto each waypoint asked for, until the pawn digs. Returns the
        /// waypoints walked and the dig.
        fn walk_and_dig(&mut self) -> (Vec<Vec3>, JobAction) {
            let mut walked = Vec::new();
            for _ in 0..200 {
                match self.step() {
                    JobAction::Wait => {}
                    JobAction::Walk { to } => {
                        walked.push(to);
                        self.pose.feet = to;
                    }
                    dig @ JobAction::Dig { .. } => return (walked, dig),
                }
            }
            panic!("the pawn never dug; walked {walked:?}");
        }

        fn claimed_by(&self, block: BlockPos) -> Option<PawnId> {
            self.marks
                .iter()
                .find(|(pos, _)| *pos == block.to_array())
                .expect("the mark exists")
                .1
                .claimed_by
        }
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut rig = Rig::on(flat_ground(8, 8), cell(2, 1, 2));

        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(rig.job, Job::Idle));
    }

    /// Ground three blocks thick, `y` in `-2..=0`, so a block can sit buried in the middle layer.
    fn thick_ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..width {
            for y in -2..=0 {
                for z in 0..depth {
                    grid.mutator().place(cell(x, y, z), 0, None);
                }
            }
        }
        grid
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut rig = Rig::on(thick_ground(10, 10), cell(1, 1, 1));
        let buried = cell(5, -1, 5);
        rig.marks.toggle(buried.to_array());
        assert!(
            crate::routing::standable(&rig.grid, cell(5, 1, 5)),
            "setup: a stand cell on the surface above it is open"
        );

        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(rig.job, Job::Idle));
        assert_eq!(rig.claimed_by(buried), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(12, 1, 1));
        rig.mark(cell(2, 1, 1));
        rig.mark(cell(9, 1, 1));
        rig.mark(cell(13, 1, 4));
        rig.pawn.select_slot(3);
        assert_eq!(rig.pawn.equipped_tool, None, "setup: the tool is put away");

        rig.step();

        assert!(
            matches!(rig.job, Job::Mine { mark, .. } if mark == [9, 1, 1]),
            "{:?}",
            rig.job
        );
        assert_eq!(rig.claimed_by(cell(9, 1, 1)), Some(PAWN));
        assert_eq!(rig.claimed_by(cell(2, 1, 1)), None, "the far mark is left");
        assert_eq!(rig.claimed_by(cell(13, 1, 4)), None, "so is the later key");
        assert_eq!(rig.pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(rig.pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn equally_near_marks_go_to_the_lower_key() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(6, 1, 3));
        rig.mark(cell(8, 1, 3));
        rig.mark(cell(4, 1, 3));

        rig.step();

        assert!(
            matches!(rig.job, Job::Mine { mark, .. } if mark == [4, 1, 3]),
            "{:?}",
            rig.job
        );
    }

    /// Ground `x` in `0..14` and, past a one-column gap, an island `x` in `15..24`, all `z` in
    /// `0..6`; nothing walks across the gap.
    fn ground_and_island() -> VoxelGrid {
        let mut grid = flat_ground(14, 6);
        for x in 15..24 {
            for z in 0..6 {
                grid.mutator().place(cell(x, 0, z), 0, None);
            }
        }
        grid
    }

    #[test]
    fn an_unroutable_nearer_mark_falls_through_to_a_farther_reachable_one() {
        let mut rig = Rig::on(ground_and_island(), cell(12, 1, 1));
        rig.mark(cell(16, 1, 2));
        rig.mark(cell(2, 1, 1));

        rig.step();

        assert!(
            matches!(rig.job, Job::Mine { mark, .. } if mark == [2, 1, 1]),
            "{:?}",
            rig.job
        );
        assert_eq!(
            rig.claimed_by(cell(16, 1, 2)),
            None,
            "the island mark is not held"
        );
    }

    #[test]
    fn a_tick_tries_at_most_the_route_attempt_limit_of_marks() {
        let mut rig = Rig::on(ground_and_island(), cell(12, 1, 1));
        for x in 16..16 + MAX_ROUTE_ATTEMPTS as i32 + 1 {
            rig.mark(cell(x, 1, 1));
        }
        rig.mark(cell(2, 1, 1));

        let first = rig.step();

        assert_eq!(first, JobAction::Wait, "every attempt this tick failed");
        assert!(matches!(rig.job, Job::Idle));
        let mut claimed_on = None;
        for tick in 2..=12 {
            rig.step();
            if matches!(rig.job, Job::Mine { .. }) {
                claimed_on = Some(tick);
                break;
            }
        }
        assert!(claimed_on.is_some(), "the reachable mark is never reached");
        assert!(
            matches!(rig.job, Job::Mine { mark, .. } if mark == [2, 1, 1]),
            "{:?}",
            rig.job
        );
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        let block = cell(6, 1, 3);
        rig.mark(block);

        let (walked, dig) = rig.walk_and_dig();

        let JobAction::Dig { block: dug, .. } = dig else {
            unreachable!("walk_and_dig returns a dig");
        };
        assert_eq!(dug, block);
        let mut previous = cell(1, 1, 1);
        for to in &walked {
            let step = crate::routing::feet_cell(*to);
            assert_eq!(*to, centre(step), "waypoints are cell centres");
            assert!(crate::routing::standable(&rig.grid, step), "{step}");
            let sideways = (step.x - previous.x).abs() + (step.z - previous.z).abs();
            assert_eq!(sideways, 1, "{previous} to {step}");
            previous = step;
        }
        assert!(is_stand_cell(block, previous), "dug from {previous}");
        assert_eq!(walked.len(), 5, "the nearest stand cell by route length");
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let block = cell(6, 1, 1);
        let cases = [
            (cell(5, 1, 1), Vec3::new(6.0, 1.5, 1.5), "west"),
            (cell(7, 1, 1), Vec3::new(7.0, 1.5, 1.5), "east"),
            (cell(6, 1, 2), Vec3::new(6.5, 1.5, 2.0), "south"),
        ];

        for (standing, face, side) in cases {
            let mut rig = Rig::on(flat_ground(12, 5), standing);
            rig.mark(block);
            let eye = centre(standing) + Vec3::Y * EYE_HEIGHT;

            let (walked, dig) = rig.walk_and_dig();

            let JobAction::Dig { block: dug, aim } = dig else {
                unreachable!("walk_and_dig returns a dig");
            };
            assert!(walked.is_empty(), "{side}: already on a stand cell");
            assert_eq!(dug, block);
            assert!(
                aim.abs_diff_eq(face, 1e-4),
                "{side}: aimed at {aim} from {eye}, the face centre is {face}"
            );
        }
    }

    #[test]
    fn the_dig_aims_at_the_top_face_from_a_cell_a_block_above_the_blocks_level() {
        let block = cell(6, 1, 1);
        let mut rig = Rig::on(flat_ground(12, 5), cell(5, 2, 1));
        rig.grid.mutator().place(cell(5, 1, 1), 0, None);
        rig.mark(block);

        let (walked, dig) = rig.walk_and_dig();

        let JobAction::Dig { block: dug, aim } = dig else {
            unreachable!("walk_and_dig returns a dig");
        };
        assert!(walked.is_empty(), "already on a stand cell");
        assert_eq!(dug, block);
        let top = Vec3::new(6.5, 2.0, 1.5);
        assert!(
            aim.abs_diff_eq(top, 1e-4),
            "the west face is covered by the platform; aimed at {aim}, the top is {top}"
        );
    }

    #[test]
    fn every_stand_cell_is_within_the_mining_reach() {
        let block = cell(-7, 3, 12);

        for dx in -3..=3 {
            for dy in -3..=4 {
                for dz in -3..=3 {
                    let feet = cell(block.x + dx, block.y + dy, block.z + dz);
                    let in_window = dx.abs() <= 1 && dz.abs() <= 1 && (-1..=2).contains(&dy);

                    let stands = is_stand_cell(block, feet);

                    assert_eq!(stands, in_window, "offset ({dx}, {dy}, {dz})");
                    if stands {
                        let eye = centre(feet) + Vec3::Y * EYE_HEIGHT;
                        let farthest = (0..8)
                            .map(|corner| {
                                block.as_vec3()
                                    + Vec3::new(
                                        (corner & 1) as f32,
                                        ((corner >> 1) & 1) as f32,
                                        ((corner >> 2) & 1) as f32,
                                    )
                            })
                            .map(|point| point.distance(eye))
                            .fold(0.0, f32::max);
                        assert!(
                            farthest <= MINE_REACH,
                            "{farthest} m from {feet} to {block}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        rig.mark(cell(8, 1, 1));
        let JobAction::Walk { to } = rig.step() else {
            panic!("the pawn should be walking");
        };
        rig.pose.feet = to;
        assert!(matches!(rig.job, Job::Mine { .. }), "setup: a job runs");

        rig.marks.toggle([8, 1, 1]);
        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(rig.job, Job::Idle));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        rig.mark(cell(8, 1, 1));
        let JobAction::Walk { to } = rig.step() else {
            panic!("the pawn should be walking");
        };
        rig.pose.feet = to;

        rig.grid.mutator().remove(cell(8, 1, 1));
        rig.marks.block_removed([8, 1, 1]);
        let action = rig.step();

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(rig.job, Job::Idle));
        assert!(!rig.marks.contains([8, 1, 1]));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        rig.mark(cell(8, 1, 1));
        rig.step();
        let mut second = Job::Idle;
        let mut second_pawn = Pawn::default();

        let action = step(
            &mut second,
            PawnId(1),
            &mut second_pawn,
            rig.pose,
            &mut rig.marks,
            &rig.grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(second, Job::Idle));
        assert_eq!(
            rig.claimed_by(cell(8, 1, 1)),
            Some(PAWN),
            "still the first's"
        );
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        let block = cell(10, 1, 1);
        rig.mark(block);

        let mut released_after = None;
        for tick in 1..=STALL_TICKS + 10 {
            rig.step();
            if matches!(rig.job, Job::Idle) {
                released_after = Some(tick);
                break;
            }
        }

        let tick = released_after.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS..=STALL_TICKS + 2).contains(&tick),
            "gave up after {tick} ticks"
        );
        assert!(rig.marks.contains(block.to_array()), "the mark stays");
        assert_eq!(rig.claimed_by(block), None, "and is free to claim");
    }

    #[test]
    fn a_slow_but_steady_walk_is_not_stalled() {
        let mut rig = Rig::on(flat_ground(12, 4), cell(1, 1, 1));
        rig.mark(cell(6, 1, 1));
        let crawl = 0.004;
        let mut ticks = 0;

        let dug = loop {
            ticks += 1;
            assert!(ticks < 20_000, "the pawn never arrived");
            match rig.step() {
                JobAction::Wait => {}
                JobAction::Walk { to } => {
                    let flat = Vec3::new(to.x - rig.pose.feet.x, 0.0, to.z - rig.pose.feet.z);
                    let moved = flat.clamp_length_max(crawl);
                    rig.pose.feet =
                        Vec3::new(rig.pose.feet.x + moved.x, to.y, rig.pose.feet.z + moved.z);
                }
                JobAction::Dig { block, .. } => break block,
            }
            assert!(
                matches!(rig.job, Job::Mine { .. }),
                "gave up after {ticks} ticks while still walking"
            );
        };

        assert_eq!(dug, cell(6, 1, 1));
        assert!(
            ticks > 2 * STALL_TICKS,
            "the walk outlasted one stall window"
        );
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut rig = Rig::on(flat_ground(14, 6), cell(1, 1, 1));
        let block = cell(8, 1, 1);
        rig.mark(block);
        let JobAction::Walk { to } = rig.step() else {
            panic!("the pawn should be walking");
        };
        let waypoint = crate::routing::feet_cell(to);

        rig.grid.mutator().remove(waypoint - BlockPos::Y);
        rig.step();

        assert!(matches!(rig.job, Job::Idle));
        assert!(rig.marks.contains(block.to_array()), "the mark stays");
        assert_eq!(rig.claimed_by(block), None);
    }

    /// The pose at the origin facing `yaw`.
    fn facing(yaw: f32) -> Pose {
        Pose {
            feet: Vec3::ZERO,
            yaw,
        }
    }

    /// A point 5 m from the origin on `bearing`, where bearing 0 is +Z and positive turns toward +X.
    fn on_bearing(bearing: f32) -> Vec3 {
        Vec3::new(bearing.sin(), 0.0, bearing.cos()) * 5.0
    }

    fn wrapped(angle: f32) -> f32 {
        let turn = std::f32::consts::TAU;
        let a = angle.rem_euclid(turn);
        if a > std::f32::consts::PI {
            a - turn
        } else {
            a
        }
    }

    fn max_turn() -> f32 {
        TURN_RATE / crate::TICK_HZ as f32
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let quarter = std::f32::consts::FRAC_PI_2;

        let right = steer(facing(0.0), on_bearing(quarter));
        let left = steer(facing(0.0), on_bearing(-quarter));
        let nearly = steer(facing(0.0), on_bearing(0.01));
        let around_the_back = steer(facing(3.0), on_bearing(-3.0));

        assert_eq!(TURN_RATE, std::f32::consts::TAU, "one turn a second");
        assert!((right.yaw_delta - max_turn()).abs() < 1e-5, "{right:?}");
        assert!((left.yaw_delta + max_turn()).abs() < 1e-5, "{left:?}");
        assert!(
            (nearly.yaw_delta - 0.01).abs() < 1e-5,
            "no overshoot: {nearly:?}"
        );
        assert!(
            (around_the_back.yaw_delta - max_turn()).abs() < 1e-5,
            "the short way is through +-pi: {around_the_back:?}"
        );
    }

    proptest! {
        #[test]
        fn steer_closes_the_gap_by_the_turn_rate_without_overshooting(
            yaw in -7.0f32..7.0,
            bearing in -7.0f32..7.0,
        ) {
            let before = wrapped(bearing - yaw).abs();

            let step = steer(facing(yaw), on_bearing(bearing));

            let after = wrapped(bearing - (yaw + step.yaw_delta)).abs();
            let expected = (before - max_turn()).max(0.0);
            prop_assert!(step.yaw_delta.abs() <= max_turn() + 1e-5, "{step:?}");
            prop_assert!((after - expected).abs() < 1e-3, "{before} -> {after}, wanted {expected}");
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let cases = [
            (0.0_f32, 1.0_f32),
            (30.0, 1.0),
            (-30.0, 1.0),
            (55.0, 1.0),
            (-55.0, 1.0),
            (65.0, 0.0),
            (-65.0, 0.0),
            (90.0, 0.0),
            (-90.0, 0.0),
            (120.0, 0.0),
            (180.0, 0.0),
        ];

        for (degrees, forward) in cases {
            let step = steer(facing(0.0), on_bearing(degrees.to_radians()));

            assert_eq!(
                step.forward, forward,
                "target {degrees} degrees off the facing"
            );
        }
    }
}
