//! The pawn job loop: claim a marked block, walk the route to a stand cell, dig.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// Horizontal distance from a waypoint centre at which a pawn counts as having reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second (one full turn per second).
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Route searches an idle pawn may run in one tick.
pub const MAX_ROUTE_ATTEMPTS: usize = 4;

/// Height of the pawn's eye above its feet: the capsule centre.
pub const EYE_HEIGHT: f32 = 1.15;

/// Where a pawn is: feet position and heading (forward is `(sin yaw, 0, cos yaw)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// Watches a walk for lack of progress.
#[derive(Clone, Debug, Default)]
pub struct StallGuard {
    pub best_distance: Option<f32>,
    pub ticks_without_progress: u32,
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

/// What the glue should do with the pawn this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of a route cell at feet height.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming the mining action at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of turning and walking, in plain numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to turn this tick.
    pub yaw_delta: f32,
    /// Walking speed as a fraction of full, `0.0..=1.0`.
    pub forward: f32,
}

/// Advances `pawn`'s job by one tick.
pub fn step(
    _id: PawnId,
    _job: &mut Job,
    _pawn: &mut Pawn,
    _pose: Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!("SG-F4-03: jobs::step")
}

/// Turn toward `to` at [`TURN_RATE`] and walk once within 60 degrees of facing it.
pub fn steer(_pose: Pose, _to: Vec3) -> WalkStep {
    todo!("SG-F4-03: jobs::steer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::standable;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const GROUND: u32 = 1;
    const ME: PawnId = PawnId(0);

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn key(pos: BlockPos) -> [i32; 3] {
        pos.to_array()
    }

    fn at(x: f32, y: f32, z: f32) -> Pose {
        Pose {
            feet: Vec3::new(x, y, z),
            yaw: 0.0,
        }
    }

    /// The centre of a cell at feet height.
    fn centre(c: BlockPos) -> Vec3 {
        Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5)
    }

    /// One block of ground over `x` and `z` in `0..=15`.
    fn flat_ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(15, 0, 15), GROUND);
        grid
    }

    struct Scene {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
    }

    impl Scene {
        fn new(grid: VoxelGrid) -> Self {
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::Idle,
            }
        }

        /// A block placed on the ground and marked for mining.
        fn mark(&mut self, pos: BlockPos) {
            self.grid.mutator().place(pos, GROUND, None);
            self.marks.toggle(key(pos));
        }

        fn step(&mut self, pose: Pose) -> JobAction {
            self.step_as(ME, pose)
        }

        fn step_as(&mut self, id: PawnId, pose: Pose) -> JobAction {
            step(
                id,
                &mut self.job,
                &mut self.pawn,
                pose,
                &mut self.marks,
                &self.grid,
            )
        }

        fn route(&self) -> Vec<BlockPos> {
            match &self.job {
                Job::Mine { route, .. } => route.clone(),
                Job::Idle => panic!("the pawn has no job"),
            }
        }

        fn claimed_by(&self, pos: BlockPos) -> Option<PawnId> {
            self.marks
                .iter()
                .find(|(p, _)| *p == key(pos))
                .and_then(|(_, mark)| mark.claimed_by)
        }

        fn is_idle(&self) -> bool {
            matches!(self.job, Job::Idle)
        }
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, 1e-4),
            "expected {expected}, got {actual}"
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut scene = Scene::new(flat_ground());
        scene.pawn.select_slot(1);

        let action = scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(
            scene.pawn.equipped_tool, None,
            "nothing to dig, nothing equipped"
        );
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(5, 0, 1), cell(7, 2, 3), GROUND);
        let mut scene = Scene::new(grid);
        scene.marks.toggle(key(cell(6, 1, 2)));

        let action = scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
    }

    #[test]
    fn an_enclosed_mark_is_skipped_for_a_farther_exposed_one() {
        let mut grid = flat_ground();
        grid.mutator()
            .fill_region(cell(5, 0, 1), cell(7, 2, 3), GROUND);
        let mut scene = Scene::new(grid);
        scene.marks.toggle(key(cell(6, 1, 2)));
        scene.mark(cell(11, 1, 2));

        scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert_eq!(scene.claimed_by(cell(11, 1, 2)), Some(ME));
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(12, 1, 2));
        scene.mark(cell(6, 1, 2));
        scene.pawn.select_slot(1);

        scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(6, 1, 2)), Some(ME));
        assert_eq!(scene.claimed_by(cell(12, 1, 2)), None);
        assert_eq!(scene.pawn.selected_slot, TOOL_SLOT);
        assert_eq!(scene.pawn.equipped_tool, Some(STARTING_TOOL));
        match &scene.job {
            Job::Mine {
                mark, route, next, ..
            } => {
                assert_eq!(*mark, cell(6, 1, 2));
                assert_eq!(*next, 0);
                assert_eq!(
                    *route,
                    [
                        cell(1, 1, 2),
                        cell(2, 1, 2),
                        cell(3, 1, 2),
                        cell(4, 1, 2),
                        cell(5, 1, 2)
                    ]
                );
            }
            Job::Idle => panic!("the pawn should have taken the mark"),
        }
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.marks.claim(key(cell(6, 1, 2)), ME);

        let alone = scene.step_as(PawnId(1), at(0.5, 1.0, 2.5));

        assert_eq!(alone, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), Some(ME));

        scene.mark(cell(12, 1, 2));
        scene.step_as(PawnId(1), at(0.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(12, 1, 2)), Some(PawnId(1)));
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), Some(ME));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));
        let route = scene.route();
        assert_eq!(route.len(), 5);

        let far = scene.step(at(0.5, 1.0, 2.5));
        let short_of_it = scene.step(at(1.0, 1.0, 2.5));
        let arrived = scene.step(at(1.2, 1.0, 2.5));

        assert_eq!(
            far,
            JobAction::Walk {
                to: centre(route[0])
            }
        );
        assert_eq!(
            short_of_it,
            JobAction::Walk {
                to: centre(route[0])
            }
        );
        assert_eq!(
            arrived,
            JobAction::Walk {
                to: centre(route[1])
            }
        );
        for pair in route.windows(2).skip(1) {
            let action = scene.step(Pose {
                feet: centre(pair[0]),
                yaw: 0.0,
            });
            assert_eq!(
                action,
                JobAction::Walk {
                    to: centre(pair[1])
                }
            );
        }
        let last = *route.last().expect("route has cells");
        let dig = scene.step(Pose {
            feet: centre(last),
            yaw: 0.0,
        });
        assert_eq!(last, cell(5, 1, 2));
        match dig {
            JobAction::Dig { block, .. } => assert_eq!(block, cell(6, 1, 2)),
            other => panic!("expected a dig, got {other:?}"),
        }
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.grid.mutator().place(cell(5, 1, 2), GROUND, None);
        scene.job = Job::Mine {
            mark: cell(6, 1, 2),
            route: vec![],
            next: 0,
            stall: StallGuard::default(),
        };
        let cases = [
            (
                at(6.5, 1.0, 0.5),
                Vec3::new(6.5, 1.5, 2.0),
                "from the south",
            ),
            (
                at(6.5, 1.0, 4.5),
                Vec3::new(6.5, 1.5, 3.0),
                "from the north",
            ),
            (
                at(4.5, 1.0, 2.5),
                Vec3::new(6.5, 2.0, 2.5),
                "from the covered west side, the top is nearest",
            ),
        ];

        for (pose, face, case) in cases {
            let action = scene.step(pose);

            match action {
                JobAction::Dig { block, aim } => {
                    assert_eq!(block, cell(6, 1, 2), "{case}");
                    assert_near(aim, face);
                }
                other => panic!("{case}: expected a dig, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));
        scene.step(at(2.5, 1.0, 2.5));

        scene.marks.toggle(key(cell(6, 1, 2)));
        let action = scene.step(at(2.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert!(scene.grid.is_solid_at(cell(6, 1, 2)), "the block stays");
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));

        scene.grid.mutator().remove(cell(6, 1, 2));
        scene.marks.block_removed(key(cell(6, 1, 2)));
        let action = scene.step(at(2.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        let stuck = at(0.5, 1.0, 2.5);

        let mut ticks = 0;
        while ticks == 0 || !scene.is_idle() {
            scene.step(stuck);
            ticks += 1;
            assert!(ticks <= STALL_TICKS + 2, "the pawn never gave up");
            if ticks == STALL_TICKS / 2 {
                assert!(!scene.is_idle(), "gave up too early");
                assert_eq!(scene.claimed_by(cell(6, 1, 2)), Some(ME));
            }
        }

        assert!(ticks >= STALL_TICKS - 1, "gave up after {ticks} ticks");
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert!(scene.marks.contains(key(cell(6, 1, 2))), "the mark stays");
    }

    #[test]
    fn a_pawn_that_keeps_closing_in_does_not_stall() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));

        for tick in 0..(STALL_TICKS * 2) {
            scene.step(at(0.5 + 0.003 * tick as f32, 1.0, 2.5));
        }

        assert!(!scene.is_idle());
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), Some(ME));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));
        let first = scene.route()[0];

        scene
            .grid
            .mutator()
            .place(first + cell(0, 2, 0), GROUND, None);
        let action = scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert!(scene.marks.contains(key(cell(6, 1, 2))), "the mark stays");
    }

    /// An 8x8 heightmap: column `(x, z)` is solid for `y` in `0..height`.
    fn terrain(heights: &[u8]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..8 {
            for z in 0..8 {
                let h = i32::from(heights[(x * 8 + z) as usize]);
                grid.mutator()
                    .fill_region(cell(x, 0, z), cell(x, h - 1, z), GROUND);
            }
        }
        grid
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            heights in proptest::collection::vec(1u8..=2, 64),
            bx in 1i32..7, bz in 1i32..7,
        ) {
            let mut grid = terrain(&heights);
            let top = i32::from(heights[(bx * 8 + bz) as usize]) - 1;
            let block = cell(bx, top, bz);
            let start = cell(0, i32::from(heights[0]), 0);
            grid.mutator().place(block, GROUND, None);
            let mut scene = Scene::new(grid);
            scene.marks.toggle(key(block));

            scene.step(at(0.5, start.y as f32, 0.5));
            scene.step(at(0.5, start.y as f32, 0.5));

            prop_assert!(!scene.is_idle(), "a reachable mark was not claimed");
            let stand = scene.route().last().copied().unwrap_or(start);
            prop_assert!(standable(&scene.grid, stand));
            prop_assert!((stand.x - block.x).abs() <= 1 && (stand.z - block.z).abs() <= 1);
            prop_assert!((-1..=2).contains(&(stand.y - block.y)), "{stand} against {block}");
            let eye = Vec3::new(stand.x as f32 + 0.5, stand.y as f32 + EYE_HEIGHT, stand.z as f32 + 0.5);
            let centre_of_block = block.as_vec3() + Vec3::splat(0.5);
            prop_assert!(eye.distance(centre_of_block) <= MINE_REACH);
        }
    }

    fn per_tick_turn() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    /// A point `distance` away from the origin along heading `yaw`.
    fn along(yaw: f32, distance: f32) -> Vec3 {
        Vec3::new(yaw.sin(), 0.0, yaw.cos()) * distance
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let facing = at(0.0, 1.0, 0.0);
        let limit = per_tick_turn();
        assert_close(limit, std::f32::consts::TAU / 60.0);

        let right = steer(facing, along(std::f32::consts::FRAC_PI_2, 4.0));
        let left = steer(facing, along(-std::f32::consts::FRAC_PI_2, 4.0));
        let behind = steer(facing, along(std::f32::consts::PI - 0.01, 4.0));
        let slight = steer(facing, along(0.02, 4.0));
        let ahead = steer(facing, along(0.0, 4.0));
        let wrapped = steer(
            Pose {
                feet: Vec3::ZERO,
                yaw: 3.0,
            },
            along(-3.0, 4.0),
        );

        assert_close(right.yaw_delta, limit);
        assert_close(left.yaw_delta, -limit);
        assert_close(behind.yaw_delta, limit);
        assert_close(slight.yaw_delta, 0.02);
        assert_close(ahead.yaw_delta, 0.0);
        assert_close(wrapped.yaw_delta, limit);
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let facing = at(0.0, 1.0, 0.0);
        let sixty = 60.0_f32.to_radians();

        let ahead = steer(facing, along(0.0, 4.0));
        let inside = steer(facing, along(sixty - 0.05, 4.0));
        let outside = steer(facing, along(sixty + 0.05, 4.0));
        let side = steer(facing, along(-std::f32::consts::FRAC_PI_2, 4.0));
        let behind = steer(facing, along(std::f32::consts::PI, 4.0));

        assert_close(ahead.forward, 1.0);
        assert_close(inside.forward, 1.0);
        assert_close(outside.forward, 0.0);
        assert_close(side.forward, 0.0);
        assert_close(behind.forward, 0.0);
    }
}
