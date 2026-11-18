//! What an idle pawn does on its own: take a marked block, walk to it over the ground and
//! dig it with the pawn's own mining action.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::Pawn;

/// How close the feet must be to a waypoint's centre, horizontally, to count as there.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives its claim up.
pub const STALL_TICKS: u32 = 120;

/// How fast a pawn turns toward its target, in radians per second: one turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walking `route` to dig the block `mark`; `next` indexes the waypoint being walked to.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// Counts ticks of a walk that is going nowhere.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    ticks: u32,
}

/// Where a pawn stands and which way it faces; yaw 0 faces +Z and turns toward +X.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PawnPose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// What the pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of the current waypoint cell.
    Walk {
        to: Vec3,
    },
    /// Dig `block` along the line from the eye to `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of turning and walking toward a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to turn this tick, positive toward +X.
    pub yaw_delta: f32,
    /// Move axis, 0 (stand) to 1 (full speed).
    pub forward: f32,
}

/// Advances `job` one tick for the pawn at `pose`: claims, walks and digs as the marks and
/// the grid allow.
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn step(
    _job: &mut Job,
    _pawn: &mut Pawn,
    _pose: PawnPose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    todo!()
}

/// Turns `pose` toward `to` at [`TURN_RATE`], walking at full speed once within 60 degrees
/// of facing it and turning in place until then. Height is ignored.
#[expect(clippy::todo, reason = "stub until implemented")]
pub fn steer(_pose: PawnPose, _to: Vec3) -> WalkStep {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, PawnId, TOOL_SLOT};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;
    use std::f32::consts::{PI, TAU};

    const GROUND: u32 = 1;
    /// Half the 2.3 m capsule above the feet: where the eye is.
    const EYE_HEIGHT: f32 = 1.15;
    const START: Vec3 = Vec3::new(0.5, 1.0, 1.5);

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn flat(x: i32, z: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(x, 0, z), GROUND);
        grid
    }

    struct Scene {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
        pose: PawnPose,
    }

    impl Scene {
        /// Ground x 0..=12, z 0..=2 with a plain block on the ground at each mark; the pawn
        /// idles on cell (0, 1, 1).
        fn new(marked: &[[i32; 3]]) -> Self {
            let mut grid = flat(12, 2);
            for &[x, y, z] in marked {
                grid.mutator().place(cell(x, y, z), GROUND, None);
            }
            Self::on(grid, marked)
        }

        fn on(grid: VoxelGrid, marked: &[[i32; 3]]) -> Self {
            let mut marks = MineMarks::default();
            for &pos in marked {
                marks.toggle(pos);
            }
            Self {
                grid,
                marks,
                pawn: Pawn::default(),
                job: Job::Idle,
                pose: PawnPose {
                    feet: START,
                    yaw: 0.0,
                },
            }
        }

        fn tick(&mut self) -> JobAction {
            step(
                &mut self.job,
                &mut self.pawn,
                self.pose,
                &mut self.marks,
                &self.grid,
            )
        }

        fn waypoint(&self) -> BlockPos {
            let Job::Mine { route, next, .. } = &self.job else {
                panic!("the pawn has no route: {:?}", self.job);
            };
            route[*next]
        }

        /// Puts the feet on the current waypoint, as if the walk had arrived.
        fn arrive(&mut self) {
            let c = self.waypoint();
            self.pose.feet = Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5);
        }

        /// Ticks, arriving at each waypoint walked to, until the pawn digs. Returns the
        /// distinct horizontal waypoint targets walked to, and the dig.
        fn walk_to_the_dig(&mut self) -> (Vec<[f32; 2]>, JobAction) {
            let mut targets: Vec<[f32; 2]> = Vec::new();
            for _ in 0..200 {
                match self.tick() {
                    JobAction::Walk { to } => {
                        if targets.last() != Some(&[to.x, to.z]) {
                            targets.push([to.x, to.z]);
                        }
                        self.arrive();
                    }
                    JobAction::Wait => {}
                    dig @ JobAction::Dig { .. } => return (targets, dig),
                }
            }
            panic!("the pawn never dug; job {:?}", self.job);
        }

        fn claimed_by(&self, pos: [i32; 3]) -> Option<PawnId> {
            self.marks
                .iter()
                .find(|(p, _)| *p == pos)
                .expect("the block is marked")
                .1
                .claimed_by
        }
    }

    fn eye(feet: Vec3) -> Vec3 {
        feet + Vec3::Y * EYE_HEIGHT
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "{what}: expected {expected}, got {actual}"
        );
    }

    fn assert_vec_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 1e-4,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut scene = Scene::new(&[]);
        scene.pawn.select_slot(3);

        let action = scene.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(scene.job, Job::Idle);
        assert_eq!(
            scene.pawn.equipped_tool, None,
            "an idle pawn touches nothing"
        );
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut scene = Scene::new(&[[3, 1, 1]]);
        for neighbour in [[2, 1, 1], [4, 1, 1], [3, 1, 0], [3, 1, 2], [3, 2, 1]] {
            let [x, y, z] = neighbour;
            scene.grid.mutator().place(cell(x, y, z), GROUND, None);
        }

        let action = scene.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(scene.job, Job::Idle);
        assert_eq!(scene.claimed_by([3, 1, 1]), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = flat(10, 1);
        grid.mutator()
            .fill_region(cell(0, 0, 4), cell(2, 0, 5), GROUND);
        let marked = [[9, 1, 0], [1, 1, 4], [5, 1, 1]];
        for [x, y, z] in marked {
            grid.mutator().place(cell(x, y, z), GROUND, None);
        }
        let mut scene = Scene::on(grid, &marked);
        scene.pawn.select_slot(3);

        scene.tick();

        let Job::Mine { mark, .. } = &scene.job else {
            panic!("the pawn should have taken a mark: {:?}", scene.job);
        };
        assert_eq!(
            *mark,
            [5, 1, 1],
            "nearer [1, 1, 4] is on an unreachable island"
        );
        assert_eq!(scene.claimed_by([5, 1, 1]), Some(PawnId(0)));
        assert_eq!(scene.claimed_by([1, 1, 4]), None);
        assert_eq!(scene.claimed_by([9, 1, 0]), None);
        assert_eq!(scene.pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(scene.pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut scene = Scene::new(&[[6, 1, 1]]);

        let (targets, dig) = scene.walk_to_the_dig();

        assert_eq!(
            targets,
            [[1.5, 1.5], [2.5, 1.5], [3.5, 1.5], [4.5, 1.5], [5.5, 1.5]]
        );
        assert_eq!(
            dig,
            JobAction::Dig {
                block: cell(6, 1, 1),
                aim: Vec3::new(6.0, 1.5, 1.5),
            }
        );
        assert_vec_near(scene.pose.feet, Vec3::new(5.5, 1.0, 1.5));
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let aim_from = |feet: Vec3, covered: Option<[i32; 3]>| {
            let mut scene = Scene::new(&[[3, 1, 0]]);
            if let Some([x, y, z]) = covered {
                scene.grid.mutator().place(cell(x, y, z), GROUND, None);
            }
            scene.pose.feet = feet;
            scene.marks.claim([3, 1, 0], PawnId(0));
            scene.job = Job::Mine {
                mark: [3, 1, 0],
                route: vec![],
                next: 0,
                stall: StallGuard::default(),
            };
            scene.tick()
        };
        let block = cell(3, 1, 0);

        let from_west = aim_from(Vec3::new(0.5, 1.0, 0.5), None);
        let from_east = aim_from(Vec3::new(6.5, 1.0, 0.5), None);
        let west_face_covered = aim_from(Vec3::new(0.5, 1.0, 0.5), Some([2, 1, 0]));

        assert_eq!(
            from_west,
            JobAction::Dig {
                block,
                aim: Vec3::new(3.0, 1.5, 0.5)
            }
        );
        assert_eq!(
            from_east,
            JobAction::Dig {
                block,
                aim: Vec3::new(4.0, 1.5, 0.5)
            }
        );
        assert_eq!(
            west_face_covered,
            JobAction::Dig {
                block,
                aim: Vec3::new(3.5, 2.0, 0.5)
            },
            "a face against another block is not exposed"
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 2i32..=11,
            by in 1i32..=2,
            bz in 0i32..=2,
        ) {
            let mut scene = Scene::new(&[]);
            scene.grid.mutator().fill_region(cell(bx, 1, bz), cell(bx, by, bz), GROUND);
            scene.marks.toggle([bx, by, bz]);

            let (_, dig) = scene.walk_to_the_dig();

            let JobAction::Dig { block, aim } = dig else { unreachable!("walk_to_the_dig returns a dig") };
            prop_assert_eq!(block, cell(bx, by, bz));
            let distance = (aim - eye(scene.pose.feet)).length();
            prop_assert!(
                distance <= MINE_REACH,
                "digging from {:?} aims {} m away", scene.pose.feet, distance
            );
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut scene = Scene::new(&[[8, 1, 1]]);
        scene.tick();
        scene.arrive();
        scene.tick();
        assert!(matches!(scene.job, Job::Mine { .. }), "the pawn is walking");

        scene.marks.toggle([8, 1, 1]);
        let action = scene.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(scene.job, Job::Idle);
        assert!(
            scene.grid.is_solid_at(cell(8, 1, 1)),
            "the block is not dug"
        );
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut scene = Scene::new(&[[8, 1, 1]]);
        scene.tick();
        scene.arrive();
        scene.tick();

        scene.grid.mutator().remove(cell(8, 1, 1));
        scene.marks.block_removed([8, 1, 1]);
        let action = scene.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(scene.job, Job::Idle);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut only = Scene::new(&[[5, 1, 1]]);
        only.marks.claim([5, 1, 1], PawnId(1));

        let action = only.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(only.job, Job::Idle);
        assert_eq!(only.claimed_by([5, 1, 1]), Some(PawnId(1)));

        let mut two = Scene::new(&[[5, 1, 1], [9, 1, 1]]);
        two.marks.claim([5, 1, 1], PawnId(1));

        two.tick();

        let Job::Mine { mark, .. } = &two.job else {
            panic!("the pawn should take the free mark: {:?}", two.job);
        };
        assert_eq!(*mark, [9, 1, 1]);
        assert_eq!(two.claimed_by([5, 1, 1]), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut scene = Scene::new(&[[8, 1, 1]]);

        let mut released = None;
        for tick in 1..=STALL_TICKS + 10 {
            let action = scene.tick();
            if scene.job == Job::Idle {
                released = Some((tick, action));
                break;
            }
        }

        let (tick, action) = released.expect("a walk that goes nowhere is given up");
        assert!(
            (STALL_TICKS..=STALL_TICKS + 2).contains(&tick),
            "released on tick {tick}"
        );
        assert_eq!(action, JobAction::Wait);
        assert!(scene.marks.contains([8, 1, 1]));
        assert_eq!(scene.claimed_by([8, 1, 1]), None);
    }

    #[test]
    fn a_walk_that_keeps_closing_in_is_not_stalled() {
        let mut scene = Scene::new(&[[12, 1, 1]]);

        let mut ticks = 0;
        let dig = loop {
            ticks += 1;
            assert!(ticks < 600, "the pawn never arrived: {:?}", scene.job);
            match scene.tick() {
                JobAction::Walk { to } => {
                    let waypoint = scene.waypoint();
                    let flat_to =
                        Vec3::new(to.x - scene.pose.feet.x, 0.0, to.z - scene.pose.feet.z);
                    let moved = flat_to.clamp_length_max(0.05);
                    scene.pose.feet += moved;
                    scene.pose.feet.y = waypoint.y as f32;
                }
                JobAction::Wait => panic!("the walk was given up after {ticks} ticks"),
                dig @ JobAction::Dig { .. } => break dig,
            }
        };

        assert!(
            ticks > STALL_TICKS,
            "the walk outlasts the stall limit: {ticks}"
        );
        assert!(matches!(dig, JobAction::Dig { block, .. } if block == cell(12, 1, 1)));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut scene = Scene::new(&[[8, 1, 1]]);
        scene.tick();
        let waypoint = scene.waypoint();

        scene
            .grid
            .mutator()
            .place(waypoint + BlockPos::new(0, 2, 0), GROUND, None);
        let action = scene.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(scene.job, Job::Idle);
        assert!(scene.marks.contains([8, 1, 1]), "the mark stays");
        assert_eq!(scene.claimed_by([8, 1, 1]), None);
    }

    fn per_tick() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    fn facing(yaw: f32) -> PawnPose {
        PawnPose {
            feet: Vec3::new(0.5, 1.0, 0.5),
            yaw,
        }
    }

    /// A target `distance` metres from the pose along heading `yaw`.
    fn along(pose: PawnPose, yaw: f32) -> Vec3 {
        pose.feet + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * 5.0
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let pose = facing(0.0);

        let right = steer(pose, along(pose, PI / 2.0));
        let left = steer(pose, along(pose, -PI / 2.0));
        let nearly_there = steer(pose, along(pose, 0.01));
        let around_the_seam = steer(facing(3.0), along(facing(3.0), -3.0));

        assert_near(TURN_RATE, TAU, "one turn per second");
        assert_near(right.yaw_delta, per_tick(), "right turn");
        assert_near(left.yaw_delta, -per_tick(), "left turn");
        assert_near(nearly_there.yaw_delta, 0.01, "a small turn is not overshot");
        assert_near(
            around_the_seam.yaw_delta,
            per_tick(),
            "the short way round from 3.0 to -3.0 is up through pi",
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let pose = facing(0.0);

        let ahead = steer(pose, along(pose, 0.0));
        let near_edge = steer(pose, along(pose, 1.0));
        let past_edge = steer(pose, along(pose, 1.1));
        let behind = steer(pose, along(pose, PI));
        let above = steer(pose, pose.feet + Vec3::new(0.0, 5.0, 3.0));

        assert_near(ahead.forward, 1.0, "straight ahead");
        assert_near(ahead.yaw_delta, 0.0, "no turn needed");
        assert_near(near_edge.forward, 1.0, "57 degrees off");
        assert_near(past_edge.forward, 0.0, "63 degrees off turns in place");
        assert_near(behind.forward, 0.0, "behind");
        assert_near(above.forward, 1.0, "height is ignored");
    }

    proptest! {
        #[test]
        fn steer_turns_the_short_way_by_at_most_the_turn_per_tick(
            yaw in -7.0f32..7.0,
            heading in -7.0f32..7.0,
        ) {
            let pose = facing(yaw);
            let gap = (heading - yaw + PI).rem_euclid(TAU) - PI;
            prop_assume!(gap.abs() < PI - 0.01);

            let walk = steer(pose, along(pose, heading));

            let expected = gap.clamp(-per_tick(), per_tick());
            prop_assert!((walk.yaw_delta - expected).abs() < 1e-3, "{} vs {expected}", walk.yaw_delta);
        }
    }
}
