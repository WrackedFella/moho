//! What an idle pawn does on its own: take a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it, in metres.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Route searches an idle pawn makes in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn is: the feet, the eye (the capsule centre) and the facing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub eye: Vec3,
    /// Radians; 0 faces +Z and a positive turn goes toward +X.
    pub yaw: f32,
}

/// Counts the ticks a walk has gone without getting closer to its waypoint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard;

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walking `route` to dig the marked block `mark`; `next` indexes the waypoint being walked to.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    Walk { to: Vec3 },
    Dig { block: BlockPos, aim: Vec3 },
}

/// How to move this tick to face and approach a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to turn this tick, positive toward +X.
    pub yaw_delta: f32,
    /// Walk axis, 0 to stand still and 1 to walk at full speed.
    pub forward: f32,
}

/// Advances the job of `pawn` (`id`) one tick from its `pose`, claiming and releasing marks in `marks`.
pub fn step(
    pawn: &mut Pawn,
    id: PawnId,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    let _ = (pawn, id, pose, marks, grid);
    todo!("step")
}

/// The turn and walk that take a pawn at `pose` toward `to`.
pub fn steer(pose: &Pose, to: Vec3) -> WalkStep {
    let _ = (pose, to);
    todo!("steer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::routing::standable;
    use proptest::prelude::*;
    use std::f32::consts::{PI, TAU};

    const EYE_HEIGHT: f32 = 1.15;
    const ID: PawnId = PawnId(0);
    const TURN_PER_TICK: f32 = TURN_RATE / TICK_HZ as f32;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn pose_at(c: BlockPos) -> Pose {
        let feet = Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5);
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    /// A pawn at the origin facing `yaw`.
    fn pose_facing(yaw: f32) -> Pose {
        Pose {
            feet: Vec3::ZERO,
            eye: Vec3::Y * EYE_HEIGHT,
            yaw,
        }
    }

    /// Open ground, `size` blocks a side, its surface blocks at y = 0.
    fn field(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..size {
            for z in 0..size {
                grid.mutator().place(cell(x, 0, z), 1, None);
            }
        }
        grid
    }

    /// Marks an ore block standing on the ground at `at`.
    fn mark_block(grid: &mut VoxelGrid, marks: &mut MineMarks, at: [i32; 3]) {
        grid.mutator().place(cell(at[0], at[1], at[2]), 1, Some(1));
        marks.toggle(at);
    }

    fn claimed_by(marks: &MineMarks, at: [i32; 3]) -> Option<Option<PawnId>> {
        marks
            .iter()
            .find(|(pos, _)| *pos == at)
            .map(|(_, mark)| mark.claimed_by)
    }

    fn horizontal(v: Vec3) -> (f32, f32) {
        (v.x, v.z)
    }

    fn centre_of(c: BlockPos) -> (f32, f32) {
        (c.x as f32 + 0.5, c.z as f32 + 0.5)
    }

    /// A pawn already walking `route` to `mark`, which it has claimed.
    fn walking(marks: &mut MineMarks, mark: [i32; 3], route: Vec<BlockPos>) -> Pawn {
        assert!(marks.claim(mark, ID), "the mark can be claimed");
        Pawn {
            job: Job::Mine {
                mark,
                route,
                next: 0,
                stall: StallGuard::default(),
            },
            ..Pawn::default()
        }
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = field(8);
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();

        let action = step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = field(12);
        for x in 4..=6 {
            for y in 1..=3 {
                for z in 4..=6 {
                    grid.mutator().place(cell(x, y, z), 1, None);
                }
            }
        }
        let mut marks = MineMarks::default();
        marks.toggle([5, 2, 5]);
        let mut pawn = Pawn::default();

        let action = step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(claimed_by(&marks, [5, 2, 5]), Some(None), "the mark stays");
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [9, 1, 9]);
        mark_block(&mut grid, &mut marks, [4, 1, 1]);
        mark_block(&mut grid, &mut marks, [1, 1, 7]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        assert_eq!(pawn.equipped_tool, None);

        step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert!(
            matches!(&pawn.job, Job::Mine { mark, .. } if *mark == [4, 1, 1]),
            "{:?}",
            pawn.job
        );
        assert_eq!(claimed_by(&marks, [4, 1, 1]), Some(Some(ID)));
        assert_eq!(claimed_by(&marks, [9, 1, 9]), Some(None));
        assert_eq!(claimed_by(&marks, [1, 1, 7]), Some(None));
        assert!(pawn.equipped_tool.is_some(), "the pickaxe is in hand");
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn an_idle_pawn_skips_a_mark_it_cannot_reach() {
        let mut grid = field(14);
        for x in 4..=6 {
            for z in 4..=6 {
                if (x, z) != (5, 5) {
                    grid.mutator().place(cell(x, 1, z), 1, None);
                    grid.mutator().place(cell(x, 2, z), 1, None);
                }
            }
        }
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [5, 1, 5]);
        mark_block(&mut grid, &mut marks, [12, 1, 12]);
        let mut pawn = Pawn::default();

        step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert!(
            matches!(&pawn.job, Job::Mine { mark, .. } if *mark == [12, 1, 12]),
            "{:?}",
            pawn.job
        );
        assert_eq!(claimed_by(&marks, [5, 1, 5]), Some(None));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let mut pawn = Pawn::default();
        let mut pose = pose_at(cell(1, 1, 1));
        let mut walked = Vec::new();
        let mut dug = None;

        for _ in 0..60 {
            match step(&mut pawn, ID, &pose, &mut marks, &grid) {
                JobAction::Walk { to } => {
                    walked.push(to);
                    pose = Pose {
                        feet: to,
                        eye: to + Vec3::Y * EYE_HEIGHT,
                        yaw: 0.0,
                    };
                }
                JobAction::Dig { block, .. } => {
                    dug = Some((block, pose));
                    break;
                }
                JobAction::Wait => panic!("a reachable mark leaves no reason to wait"),
            }
        }

        let (block, at) = dug.expect("the pawn digs once it has walked there");
        assert_eq!(block, cell(8, 1, 1));
        assert!(walked.len() >= 3, "the block is 7 m away: {walked:?}");
        let stand = crate::routing::feet_cell(at.feet);
        assert!(
            (stand.x - 8).abs() <= 1 && (stand.z - 1).abs() <= 1,
            "dug from {stand:?}, too far from the block"
        );
        assert!(
            (0..=3).contains(&stand.y),
            "dug from {stand:?}, off the block's height"
        );
        for to in walked {
            let waypoint = crate::routing::feet_cell(to);
            assert!(standable(&grid, waypoint), "walked through {waypoint:?}");
        }
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let cases = [
            // From the west the block's near side is closest.
            (cell(5, 1, 1), Vec3::new(6.0, 1.5, 1.5)),
            // From on top of it, the top face.
            (cell(6, 2, 1), Vec3::new(6.5, 2.0, 1.5)),
            // From the south side.
            (cell(6, 1, 0), Vec3::new(6.5, 1.5, 1.0)),
        ];

        for (stand, expected) in cases {
            let mut grid = field(12);
            let mut marks = MineMarks::default();
            mark_block(&mut grid, &mut marks, [6, 1, 1]);
            let mut pawn = walking(&mut marks, [6, 1, 1], vec![stand]);
            if let Job::Mine { next, .. } = &mut pawn.job {
                *next = 1;
            }

            let action = step(&mut pawn, ID, &pose_at(stand), &mut marks, &grid);

            let JobAction::Dig { block, aim } = action else {
                panic!("{stand:?}: past the last waypoint the pawn digs, got {action:?}");
            };
            assert_eq!(block, cell(6, 1, 1), "{stand:?}");
            assert!(aim.distance(expected) < 1e-3, "{stand:?}: aimed at {aim}");
        }
    }

    #[test]
    fn the_dig_does_not_aim_at_a_face_that_is_covered() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [6, 1, 1]);
        grid.mutator().place(cell(5, 1, 1), 1, None);
        let stand = cell(5, 2, 1);
        let mut pawn = walking(&mut marks, [6, 1, 1], vec![stand]);
        if let Job::Mine { next, .. } = &mut pawn.job {
            *next = 1;
        }

        let action = step(&mut pawn, ID, &pose_at(stand), &mut marks, &grid);

        let JobAction::Dig { aim, .. } = action else {
            panic!("expected a dig, got {action:?}");
        };
        assert!(
            aim.distance(Vec3::new(6.0, 1.5, 1.5)) > 0.1,
            "aimed through the neighbour at {aim}"
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            heights in proptest::collection::vec(1..=3i32, 64),
            mx in 0..8i32, mz in 0..8i32, sx in 0..8i32, sz in 0..8i32,
        ) {
            let mut grid = VoxelGrid::new(16);
            for (i, &height) in heights.iter().enumerate() {
                for y in 0..height {
                    grid.mutator().place(cell((i % 8) as i32, y, (i / 8) as i32), 1, None);
                }
            }
            let top = |x: i32, z: i32| heights[(z * 8 + x) as usize];
            let block = [mx, top(mx, mz) - 1, mz];
            let mut marks = MineMarks::default();
            marks.toggle(block);
            let mut pawn = Pawn::default();
            let start = pose_at(cell(sx, top(sx, sz), sz));

            step(&mut pawn, ID, &start, &mut marks, &grid);

            if let Job::Mine { route, .. } = &pawn.job {
                let stand = route.last().copied().unwrap_or(cell(sx, top(sx, sz), sz));
                prop_assert!((stand.x - block[0]).abs() <= 1, "{stand:?} for {block:?}");
                prop_assert!((stand.z - block[2]).abs() <= 1, "{stand:?} for {block:?}");
                prop_assert!(
                    (block[1] - 1..=block[1] + 2).contains(&stand.y),
                    "{stand:?} for {block:?}"
                );
                let eye = pose_at(stand).eye;
                let centre = Vec3::new(
                    block[0] as f32 + 0.5,
                    block[1] as f32 + 0.5,
                    block[2] as f32 + 0.5,
                );
                prop_assert!(
                    eye.distance(centre) <= MINE_REACH,
                    "eye {eye} is {} from the block at {block:?}",
                    eye.distance(centre)
                );
            }
        }
    }

    #[test]
    fn a_claim_comes_with_a_route() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 8]);
        let mut pawn = Pawn::default();

        step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert!(
            matches!(&pawn.job, Job::Mine { route, .. } if !route.is_empty()),
            "{:?}",
            pawn.job
        );
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);
        let mid_walk = pose_at(cell(3, 1, 1));
        marks.toggle([8, 1, 1]);

        let action = step(&mut pawn, ID, &mid_walk, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(grid.is_solid_at(cell(8, 1, 1)), "the block is untouched");
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);
        grid.mutator().remove(cell(8, 1, 1));
        marks.block_removed([8, 1, 1]);

        let action = step(&mut pawn, ID, &pose_at(cell(3, 1, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [4, 1, 1]);
        assert!(marks.claim([4, 1, 1], PawnId(1)));
        let mut pawn = Pawn::default();

        let alone = step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);
        mark_block(&mut grid, &mut marks, [9, 1, 9]);
        step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert_eq!(alone, JobAction::Wait, "the only mark is taken");
        assert_eq!(claimed_by(&marks, [4, 1, 1]), Some(Some(PawnId(1))));
        assert!(
            matches!(&pawn.job, Job::Mine { mark, .. } if *mark == [9, 1, 9]),
            "the free mark goes to the free pawn: {:?}",
            pawn.job
        );
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);
        let stuck = pose_at(cell(1, 1, 1));
        let mut released_on = None;

        for tick in 1..=STALL_TICKS + 5 {
            step(&mut pawn, ID, &stuck, &mut marks, &grid);
            if pawn.job == Job::Idle {
                released_on = Some(tick);
                break;
            }
        }

        let tick = released_on.expect("a pawn that cannot get anywhere gives up");
        assert!(
            tick + 2 >= STALL_TICKS,
            "gave up after {tick} ticks, before the stall limit"
        );
        assert_eq!(
            claimed_by(&marks, [8, 1, 1]),
            Some(None),
            "the mark stays, unclaimed"
        );
    }

    #[test]
    fn a_walk_that_makes_progress_is_not_stalled() {
        let mut grid = field(40);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [36, 1, 1]);
        let mut pawn = Pawn::default();
        let mut pose = pose_at(cell(1, 1, 1));

        for _ in 0..4 * STALL_TICKS {
            if let JobAction::Walk { to } = step(&mut pawn, ID, &pose, &mut marks, &grid) {
                let toward = Vec3::new(to.x - pose.feet.x, 0.0, to.z - pose.feet.z)
                    .normalize_or_zero()
                    * 0.05;
                pose.feet += toward;
                pose.eye += toward;
            }
        }

        assert!(
            matches!(pawn.job, Job::Mine { .. }),
            "a pawn creeping toward its goal still has its job"
        );
        assert_eq!(claimed_by(&marks, [36, 1, 1]), Some(Some(ID)));
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);
        grid.mutator().remove(cell(2, 0, 1));

        let action = step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(
            claimed_by(&marks, [8, 1, 1]),
            Some(None),
            "the mark stays, unclaimed"
        );
    }

    #[test]
    fn walking_to_the_current_waypoint_names_its_centre() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);

        let action = step(&mut pawn, ID, &pose_at(cell(1, 1, 1)), &mut marks, &grid);

        let JobAction::Walk { to } = action else {
            panic!("expected a walk, got {action:?}");
        };
        assert_eq!(horizontal(to), centre_of(cell(2, 1, 1)));
    }

    #[test]
    fn reaching_a_waypoint_moves_on_to_the_next() {
        let mut grid = field(12);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [8, 1, 1]);
        let route: Vec<BlockPos> = (2..=7).map(|x| cell(x, 1, 1)).collect();
        let mut pawn = walking(&mut marks, [8, 1, 1], route);
        let almost = Pose {
            feet: Vec3::new(2.5 + ARRIVE_RADIUS - 0.05, 1.0, 1.5),
            ..pose_at(cell(2, 1, 1))
        };

        let action = step(&mut pawn, ID, &almost, &mut marks, &grid);

        let JobAction::Walk { to } = action else {
            panic!("expected a walk, got {action:?}");
        };
        assert_eq!(horizontal(to), centre_of(cell(3, 1, 1)));
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let east = Vec3::new(5.0, 0.0, 0.0);
        let west = Vec3::new(-5.0, 0.0, 0.0);

        let to_east = steer(&pose_facing(0.0), east);
        let to_west = steer(&pose_facing(0.0), west);
        let nearly_there = steer(&pose_facing(0.0), Vec3::new(0.05, 0.0, 5.0));
        let across_the_seam = steer(
            &pose_facing(3.0),
            Vec3::new(5.0 * (-3.0f32).sin(), 0.0, 5.0 * (-3.0f32).cos()),
        );

        assert!(
            (to_east.yaw_delta - TURN_PER_TICK).abs() < 1e-5,
            "east of north is a clockwise turn, at the turn rate: {to_east:?}"
        );
        assert!(
            (to_west.yaw_delta + TURN_PER_TICK).abs() < 1e-5,
            "{to_west:?}"
        );
        assert!(
            nearly_there.yaw_delta > 0.0 && nearly_there.yaw_delta < TURN_PER_TICK,
            "a small error is closed, not overshot: {nearly_there:?}"
        );
        assert!(
            (nearly_there.yaw_delta - 0.05f32.atan2(5.0)).abs() < 1e-4,
            "{nearly_there:?}"
        );
        assert!(
            (across_the_seam.yaw_delta - TURN_PER_TICK).abs() < 1e-5,
            "turns the short way through -pi/pi: {across_the_seam:?}"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let ahead = Vec3::new(0.0, 0.0, 5.0);
        let at_fifty_degrees = Vec3::new(50f32.to_radians().sin(), 0.0, 50f32.to_radians().cos());
        let at_seventy_degrees = Vec3::new(70f32.to_radians().sin(), 0.0, 70f32.to_radians().cos());
        let behind = Vec3::new(0.0, 0.0, -5.0);

        let straight = steer(&pose_facing(0.0), ahead);
        let near = steer(&pose_facing(0.0), at_fifty_degrees * 5.0);
        let far = steer(&pose_facing(0.0), at_seventy_degrees * 5.0);
        let back = steer(&pose_facing(0.0), behind);

        assert_eq!(straight.forward, 1.0);
        assert_eq!(straight.yaw_delta, 0.0);
        assert_eq!(near.forward, 1.0, "turning and walking together");
        assert_eq!(far.forward, 0.0, "turns in place");
        assert_eq!(back.forward, 0.0);
    }

    proptest! {
        #[test]
        fn steer_closes_the_shortest_arc_at_no_more_than_the_turn_rate(
            yaw in -PI..PI,
            bearing in -PI..PI,
        ) {
            let to = Vec3::new(5.0 * bearing.sin(), 0.0, 5.0 * bearing.cos());
            let mut error = (bearing - yaw).rem_euclid(TAU);
            if error > PI {
                error -= TAU;
            }
            prop_assume!((error.abs() - 60f32.to_radians()).abs() > 1e-2);
            prop_assume!((error.abs() - PI).abs() > 1e-2);

            let walk = steer(&pose_facing(yaw), to);

            prop_assert!(
                (walk.yaw_delta - error.clamp(-TURN_PER_TICK, TURN_PER_TICK)).abs() < 1e-4,
                "yaw {yaw}, bearing {bearing}: turned {}", walk.yaw_delta
            );
            let facing = error.abs() <= 60f32.to_radians();
            prop_assert_eq!(walk.forward, if facing { 1.0 } else { 0.0 });
        }
    }
}
