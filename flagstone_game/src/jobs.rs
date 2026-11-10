//! What a pawn does when nobody possesses it: take a marked block, walk to it, dig it.
//!
//! The loop reads the marks, the grid and the pawn's pose and answers with a plain action;
//! the caller applies it through the pawn's own movement and mining.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How close to a waypoint's centre, horizontally in metres, the feet count as arrived.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Turning speed, radians per second: one turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// The eye's height above the feet, metres.
pub const EYE_HEIGHT: f32 = 1.15;

/// Routes an idle pawn tries in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn stands and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    /// Radians; 0 faces +Z and positive turns toward +X.
    pub yaw: f32,
}

/// Watches a walk for lack of progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    pub ticks_without_progress: u32,
}

/// A pawn's current job.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The marked block this pawn holds.
        mark: [i32; 3],
        /// Cells to walk, from the pawn's cell to the stand cell.
        route: Vec<BlockPos>,
        /// Index into `route` of the waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of a cell at feet level.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming from the eye at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of movement: how far to turn and how much of full walking speed to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians this tick, positive turning toward +X.
    pub yaw_delta: f32,
    /// 0 to 1 of walking speed.
    pub forward: f32,
}

/// Whether feet in `cell` put `block` inside the mining reach: within one column of it in x
/// and z, between one below and two above it.
pub fn is_stand_cell(block: BlockPos, cell: BlockPos) -> bool {
    let _ = (block, cell);
    todo!("jobs::is_stand_cell")
}

/// Advances `job` one tick for the pawn `id` at `pose`.
pub fn step(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    let _ = (job, pawn, id, pose, marks, grid);
    todo!("jobs::step")
}

/// Turn and speed that bring a pawn at `pose` toward `to`.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let _ = (pose, to);
    todo!("jobs::steer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::MINE_REACH;
    use crate::routing::{feet_cell, standable};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const STONE: u32 = 1;
    const ME: PawnId = PawnId(0);
    const OTHER: PawnId = PawnId(1);
    const EPS: f32 = 1e-4;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// The feet position that stands centred in `c`.
    fn centre(c: BlockPos) -> Vec3 {
        Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5)
    }

    fn place(grid: &mut VoxelGrid, c: BlockPos) {
        grid.mutator().place(c, STONE, None);
    }

    /// Everything one pawn's job needs, driven a tick at a time.
    struct Site {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
        pose: Pose,
    }

    impl Site {
        /// Ground one block thick (y 0) from -12 to 12 in x and z, the pawn centred at (0, 1, 0).
        fn flat() -> Self {
            let mut grid = VoxelGrid::new(16);
            grid.mutator()
                .fill_region(cell(-12, 0, -12), cell(12, 0, 12), STONE);
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::default(),
                pose: Pose {
                    feet: centre(cell(0, 1, 0)),
                    yaw: 0.0,
                },
            }
        }

        /// A solid block at `c`, marked.
        fn mark_block(&mut self, c: BlockPos) {
            place(&mut self.grid, c);
            self.marks.toggle(c.to_array());
        }

        fn tick(&mut self) -> JobAction {
            step(
                &mut self.job,
                &mut self.pawn,
                ME,
                self.pose,
                &mut self.marks,
                &self.grid,
            )
        }

        /// Ticks, moving the pawn onto each waypoint it is sent to, until it digs; at most
        /// `limit` ticks.
        fn run_to_the_dig(&mut self, limit: usize) -> Run {
            let mut run = Run::default();
            for _ in 0..limit {
                match self.tick() {
                    JobAction::Wait => {}
                    JobAction::Walk { to } => {
                        run.targets.push(to);
                        self.pose.feet = to;
                    }
                    JobAction::Dig { block, aim } => {
                        run.dig = Some((block, aim, self.pose.feet));
                        break;
                    }
                }
            }
            run
        }

        fn claimed_by(&self, mark: [i32; 3]) -> Option<PawnId> {
            self.marks
                .iter()
                .find(|(pos, _)| *pos == mark)
                .and_then(|(_, m)| m.claimed_by)
        }

        fn is_marked(&self, mark: [i32; 3]) -> bool {
            self.marks.contains(mark)
        }
    }

    #[derive(Default)]
    struct Run {
        targets: Vec<Vec3>,
        dig: Option<(BlockPos, Vec3, Vec3)>,
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < EPS,
            "expected {expected:?}, got {actual:?}"
        );
    }

    mod claiming {
        use super::*;

        #[test]
        fn no_marks_leaves_the_pawn_waiting() {
            let mut site = Site::flat();
            site.pawn.select_slot(3);

            let action = site.tick();

            assert_eq!(action, JobAction::Wait);
            assert_eq!(site.job, Job::Idle);
            assert_eq!(site.pawn.equipped_tool, None, "nothing to dig, no tool out");
        }

        #[test]
        fn a_mark_with_no_exposed_face_is_not_claimed() {
            let mut site = Site::flat();
            site.grid
                .mutator()
                .fill_region(cell(-12, -2, -12), cell(12, 0, 12), STONE);
            site.marks.toggle([4, -1, 0]);

            let buried = site.tick();

            assert_eq!(buried, JobAction::Wait);
            assert_eq!(site.job, Job::Idle);
            assert_eq!(site.claimed_by([4, -1, 0]), None);
            assert!(site.is_marked([4, -1, 0]), "the order stands");

            site.marks.toggle([6, 0, 0]);
            let with_an_exposed_one = site.tick();

            assert_ne!(with_an_exposed_one, JobAction::Wait);
            assert_eq!(site.claimed_by([6, 0, 0]), Some(ME));
            assert_eq!(site.claimed_by([4, -1, 0]), None);
        }

        #[test]
        fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
            let mut site = Site::flat();
            // An island nearer than any other mark, cut off by a moat.
            for x in -2..=2_i32 {
                for z in 1..=5_i32 {
                    if x.abs().max((z - 3).abs()) == 2 {
                        site.grid.mutator().remove(cell(x, 0, z));
                    }
                }
            }
            site.mark_block(cell(0, 1, 3));
            site.mark_block(cell(-9, 1, 0));
            site.mark_block(cell(4, 1, 0));
            site.pawn.select_slot(3);

            let action = site.tick();

            assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
            assert_eq!(site.claimed_by([4, 1, 0]), Some(ME), "nearest reachable");
            assert_eq!(site.claimed_by([0, 1, 3]), None, "nearer, but unreachable");
            assert_eq!(site.claimed_by([-9, 1, 0]), None, "reachable, but farther");
            assert!(matches!(
                site.job,
                Job::Mine {
                    mark: [4, 1, 0],
                    ..
                }
            ));
            assert_eq!(site.pawn.equipped_tool, Some(STARTING_TOOL));
            assert_eq!(site.pawn.selected_slot, crate::pawn::TOOL_SLOT);
        }

        #[test]
        fn ties_between_marks_go_to_the_lowest_key() {
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));
            site.mark_block(cell(-4, 1, 0));

            site.tick();

            assert_eq!(site.claimed_by([-4, 1, 0]), Some(ME));
            assert_eq!(site.claimed_by([4, 1, 0]), None);
        }

        #[test]
        fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));
            assert!(site.marks.claim([4, 1, 0], OTHER));

            let action = site.tick();

            assert_eq!(action, JobAction::Wait);
            assert_eq!(site.job, Job::Idle);
            assert_eq!(site.claimed_by([4, 1, 0]), Some(OTHER));
            assert_eq!(site.marks.unclaimed().count(), 0);
        }

        #[test]
        fn a_claimed_mark_leaves_the_pawn_the_next_free_one() {
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));
            site.mark_block(cell(-8, 1, 0));
            assert!(site.marks.claim([4, 1, 0], OTHER));

            site.tick();

            assert_eq!(site.claimed_by([-8, 1, 0]), Some(ME));
            assert_eq!(site.claimed_by([4, 1, 0]), Some(OTHER));
        }
    }

    mod working {
        use super::*;

        #[test]
        fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));
            let start = feet_cell(site.pose.feet);

            let run = site.run_to_the_dig(30);

            let (block, _, feet) = run.dig.expect("the pawn reaches the block and digs");
            assert_eq!(block, cell(4, 1, 0));
            assert_eq!(feet_cell(feet), cell(3, 1, 0), "the nearest stand cell");
            assert!(
                !run.targets.is_empty(),
                "the stand cell is three cells away"
            );
            let mut previous = start;
            for target in &run.targets {
                let at = feet_cell(*target);
                assert!(standable(&site.grid, at), "{at} is not ground to walk");
                let hop = at - previous;
                assert!(hop.x.abs() + hop.z.abs() <= 1 && hop.y.abs() <= 1, "{hop}");
                previous = at;
            }
            assert_eq!(
                previous,
                cell(3, 1, 0),
                "the last waypoint is the stand cell"
            );
            assert_near(*run.targets.last().unwrap(), centre(cell(3, 1, 0)));
        }

        #[test]
        fn the_dig_ends_the_job_and_frees_the_mark() {
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));

            let run = site.run_to_the_dig(30);

            assert!(run.dig.is_some());
            assert_eq!(site.job, Job::Idle);
            assert!(
                site.is_marked([4, 1, 0]),
                "removal, not the dig, drops marks"
            );
            assert_eq!(site.claimed_by([4, 1, 0]), None);
        }

        #[test]
        fn the_pawn_does_not_dig_before_it_arrives() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));

            let early: Vec<JobAction> = (0..5).map(|_| site.tick()).collect();

            assert!(
                early.iter().all(|a| !matches!(a, JobAction::Dig { .. })),
                "{early:?}"
            );
        }

        #[test]
        fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
            // Beside the block: its west face is nearest.
            let mut beside = Site::flat();
            beside.mark_block(cell(4, 1, 0));
            beside.pose.feet = centre(cell(3, 1, 0));
            // On top of it: the top is nearest.
            let mut on_top = Site::flat();
            on_top.mark_block(cell(4, 1, 0));
            on_top.pose.feet = centre(cell(4, 2, 0));
            // The nearest faces, west and top, are covered; the north face is the nearest bare one.
            let mut covered = Site::flat();
            place(&mut covered.grid, cell(3, 1, 0));
            place(&mut covered.grid, cell(4, 2, 0));
            place(&mut covered.grid, cell(4, 1, 1));
            covered.mark_block(cell(4, 1, 0));
            covered.pose.feet = centre(cell(3, 2, 0));

            let aims: Vec<Vec3> = [beside, on_top, covered]
                .into_iter()
                .map(|mut site| {
                    let run = site.run_to_the_dig(10);
                    run.dig.expect("the pawn digs from where it stands").1
                })
                .collect();

            assert_near(aims[0], Vec3::new(4.0, 1.5, 0.5));
            assert_near(aims[1], Vec3::new(4.5, 2.0, 0.5));
            assert_near(aims[2], Vec3::new(4.5, 1.5, 0.0));
        }

        #[test]
        fn arriving_within_the_radius_advances_to_the_next_waypoint() {
            let near = ARRIVE_RADIUS - 0.05;
            let far = ARRIVE_RADIUS + 0.05;
            let waypoint_after_arriving_off_centre = |offset: f32| {
                let mut site = Site::flat();
                site.mark_block(cell(8, 1, 0));
                let mut sent = site.tick();
                // Walk off the start cell, whether or not it is the first waypoint.
                for _ in 0..3 {
                    if matches!(sent, JobAction::Walk { to } if feet_cell(to) == cell(1, 1, 0)) {
                        break;
                    }
                    sent = site.tick();
                }
                assert!(
                    matches!(sent, JobAction::Walk { to } if feet_cell(to) == cell(1, 1, 0)),
                    "{sent:?}"
                );
                site.pose.feet = centre(cell(1, 1, 0)) + Vec3::new(0.0, 0.0, offset);
                site.tick()
            };

            let inside = waypoint_after_arriving_off_centre(near);
            let outside = waypoint_after_arriving_off_centre(far);

            assert!(
                matches!(inside, JobAction::Walk { to } if feet_cell(to) == cell(2, 1, 0)),
                "{inside:?}"
            );
            assert!(
                matches!(outside, JobAction::Walk { to } if feet_cell(to) == cell(1, 1, 0)),
                "{outside:?}"
            );
        }
    }

    mod letting_go {
        use super::*;

        #[test]
        fn a_cancelled_mark_releases_the_pawn_mid_walk() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();
            assert!(matches!(site.job, Job::Mine { .. }), "the walk began");

            site.marks.toggle([9, 1, 0]);
            let action = site.tick();

            assert_eq!(action, JobAction::Wait);
            assert_eq!(site.job, Job::Idle);
            assert!(!site.is_marked([9, 1, 0]));
        }

        #[test]
        fn a_mark_dug_by_another_pawn_releases_the_claim() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();
            assert_eq!(site.claimed_by([9, 1, 0]), Some(ME));

            site.grid.mutator().remove(cell(9, 1, 0));
            site.marks.block_removed([9, 1, 0]);
            let action = site.tick();

            assert_eq!(action, JobAction::Wait);
            assert_eq!(site.job, Job::Idle);
            assert_eq!(site.marks.iter().count(), 0);
        }

        #[test]
        fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();
            assert!(matches!(site.job, Job::Mine { .. }), "the walk began");

            let mut released_after = None;
            for ticks in 1..=STALL_TICKS + 2 {
                site.tick();
                if site.job == Job::Idle {
                    released_after = Some(ticks);
                    break;
                }
            }

            let ticks = released_after.expect("a pawn that never moves gives up");
            assert!(
                (STALL_TICKS - 1..=STALL_TICKS + 2).contains(&ticks),
                "gave up after {ticks} ticks"
            );
            assert!(site.is_marked([9, 1, 0]), "the mark stays");
            assert_eq!(site.claimed_by([9, 1, 0]), None, "and is free to take");
        }

        #[test]
        fn a_walk_that_makes_progress_never_stalls() {
            let mut site = Site::flat();
            site.mark_block(cell(12, 1, 0));
            let pace = 4.0 / crate::TICK_HZ as f32;
            let mut dug = false;

            for _ in 0..(STALL_TICKS * 4) {
                match site.tick() {
                    JobAction::Wait => {}
                    JobAction::Walk { to } => {
                        let mut toward = to - site.pose.feet;
                        toward.y = 0.0;
                        site.pose.feet += toward.clamp_length_max(pace);
                        site.pose.feet.y = to.y;
                    }
                    JobAction::Dig { block, .. } => {
                        assert_eq!(block, cell(12, 1, 0));
                        dug = true;
                        break;
                    }
                }
            }

            assert!(dug, "a long walk at walking pace reaches the block");
        }

        #[test]
        fn a_waypoint_that_stops_being_standable_releases_the_claim() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));
            site.tick();
            assert!(matches!(site.job, Job::Mine { .. }), "the walk began");

            place(&mut site.grid, cell(1, 2, 0));
            let mut released = false;
            for _ in 0..3 {
                site.tick();
                if site.job == Job::Idle {
                    released = true;
                    break;
                }
            }

            assert!(released, "a blocked waypoint ends the job");
            assert!(site.is_marked([8, 1, 0]), "the mark stays");
            assert_eq!(site.claimed_by([8, 1, 0]), None);
        }
    }

    mod stand_cells {
        use super::*;

        #[test]
        fn the_stand_window_is_one_column_wide_and_four_cells_tall() {
            let block = cell(10, 20, -5);
            let inside = [
                (0, 0, 0),
                (1, 0, 1),
                (-1, 0, -1),
                (0, -1, 0),
                (0, 2, 0),
                (1, 2, -1),
            ];
            let outside = [
                (2, 0, 0),
                (-2, 0, 0),
                (0, 0, 2),
                (0, 0, -2),
                (0, -2, 0),
                (0, 3, 0),
            ];

            for (dx, dy, dz) in inside {
                assert!(
                    is_stand_cell(block, block + cell(dx, dy, dz)),
                    "({dx}, {dy}, {dz}) is within reach"
                );
            }
            for (dx, dy, dz) in outside {
                assert!(
                    !is_stand_cell(block, block + cell(dx, dy, dz)),
                    "({dx}, {dy}, {dz}) is not"
                );
            }
        }

        proptest! {
            #[test]
            fn every_stand_cell_is_within_the_mining_reach(
                block in (-1000..1000i32, -64..64i32, -1000..1000i32),
                offset in (-4..=4i32, -4..=5i32, -4..=4i32),
            ) {
                let block = cell(block.0, block.1, block.2);
                let feet = block + cell(offset.0, offset.1, offset.2);

                if is_stand_cell(block, feet) {
                    let eye = centre(feet) + Vec3::Y * EYE_HEIGHT;
                    let lo = block.as_vec3();
                    let farthest = (0..8)
                        .map(|corner| {
                            lo + Vec3::new(
                                (corner & 1) as f32,
                                ((corner >> 1) & 1) as f32,
                                ((corner >> 2) & 1) as f32,
                            )
                        })
                        .map(|point| point.distance(eye))
                        .fold(0.0, f32::max);
                    prop_assert!(
                        farthest <= MINE_REACH,
                        "from {feet} the far side of {block} is {farthest} m away"
                    );
                }
            }
        }
    }

    mod steering {
        use super::*;

        const PER_TICK: f32 = std::f32::consts::TAU / crate::TICK_HZ as f32;

        fn facing(yaw: f32) -> Pose {
            Pose {
                feet: Vec3::new(0.5, 1.0, 0.5),
                yaw,
            }
        }

        /// A point five metres from the pawn on `bearing` (0 is +Z, positive toward +X).
        fn toward_bearing(pose: Pose, bearing: f32) -> Vec3 {
            pose.feet + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * 5.0
        }

        #[test]
        fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
            let pose = facing(0.0);

            let right = steer(pose, toward_bearing(pose, 1.5));
            let left = steer(pose, toward_bearing(pose, -1.5));
            let barely_off = steer(pose, toward_bearing(pose, 0.01));
            let over_the_seam = steer(facing(3.0), toward_bearing(facing(3.0), -3.0));

            assert!(
                (right.yaw_delta - PER_TICK).abs() < EPS,
                "a wide angle turns at the full rate: {}",
                right.yaw_delta
            );
            assert!(
                (left.yaw_delta + PER_TICK).abs() < EPS,
                "{}",
                left.yaw_delta
            );
            assert!(
                (barely_off.yaw_delta - 0.01).abs() < EPS,
                "a small angle is closed, not overshot: {}",
                barely_off.yaw_delta
            );
            assert!(
                (over_the_seam.yaw_delta - PER_TICK).abs() < EPS,
                "the short way round from 3.0 to -3.0 is up through pi: {}",
                over_the_seam.yaw_delta
            );
        }

        #[test]
        fn steer_ignores_height_when_picking_the_bearing() {
            let pose = facing(0.0);
            let level = toward_bearing(pose, 0.8);
            let high = level + Vec3::Y * 4.0;

            assert_eq!(steer(pose, level), steer(pose, high));
        }

        #[test]
        fn steer_walks_only_when_facing_the_target() {
            let pose = facing(0.0);
            let bearing = |b: f32| steer(pose, toward_bearing(pose, b));

            let dead_ahead = bearing(0.0);
            let slightly_off = bearing(0.4);
            let off_the_other_way = bearing(-0.4);
            let side_on = bearing(1.6);
            let behind = bearing(std::f32::consts::PI);

            assert_eq!(dead_ahead.forward, 1.0, "full speed");
            assert_eq!(dead_ahead.yaw_delta, 0.0);
            assert_eq!(slightly_off.forward, 1.0);
            assert_eq!(off_the_other_way.forward, 1.0);
            assert_eq!(side_on.forward, 0.0, "turns in place");
            assert!(side_on.yaw_delta > 0.0);
            assert_eq!(behind.forward, 0.0);
            assert!(behind.yaw_delta.abs() > 0.0, "and still turns");
        }

        proptest! {
            #[test]
            fn steer_never_turns_past_the_rate_nor_walks_unaimed(
                yaw in -6.0f32..6.0,
                bearing in -6.0f32..6.0,
            ) {
                let pose = facing(yaw);

                let step = steer(pose, toward_bearing(pose, bearing));

                prop_assert!(step.yaw_delta.abs() <= PER_TICK + EPS);
                prop_assert!((0.0..=1.0).contains(&step.forward));
            }
        }
    }
}
