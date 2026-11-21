//! What an idle pawn does with the colony's mining marks: claim one, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId};
use crate::selection::PAWN_HALF_EXTENTS;

/// How close the feet must be to a waypoint's centre, horizontally, to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward the current waypoint before a pawn gives its claim up.
pub const STALL_TICKS: u32 = 120;

/// Fastest turn, in radians per second: one full turn a second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Where a pawn stands and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The point under the capsule's centre where the feet touch the ground.
    pub feet: Vec3,
    /// The eye, where a dig starts: the capsule's centre.
    pub eye: Vec3,
    /// Facing in radians; 0 faces +Z and the angle grows toward +X.
    pub yaw: f32,
}

impl Pose {
    /// The pose of a capsule whose feet are at `feet`.
    pub fn standing(feet: Vec3, yaw: f32) -> Self {
        Self {
            feet,
            eye: feet + Vec3::Y * PAWN_HALF_EXTENTS.y,
            yaw,
        }
    }
}

/// Counts the ticks a pawn has made no progress toward its waypoint.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    still_ticks: u32,
    nearest: Option<f32>,
}

/// What a pawn is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walking `route` to dig the block marked at `mark`, `next` being the waypoint in front.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// What a pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward the point `to`.
    Walk {
        to: Vec3,
    },
    /// Dig `block`, aiming at the point `aim` on its surface.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// How a pawn moves for one tick, in the units a character command takes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WalkStep {
    /// Radians to turn, positive toward +X from facing +Z.
    pub yaw_delta: f32,
    /// `1.0` walks at full speed, `0.0` turns in place.
    pub forward: f32,
}

/// Advances the job of `pawn`, which is `id`, for one tick: claims a mark when idle, follows
/// its route, and says what the pawn does now.
pub fn step(
    _pawn: &mut Pawn,
    _id: PawnId,
    _pose: &Pose,
    _marks: &mut MineMarks,
    _grid: &VoxelGrid,
) -> JobAction {
    JobAction::Wait
}

/// The turn and walk that take a pawn at `pose` toward `to` this tick.
pub fn steer(_pose: &Pose, _to: Vec3) -> WalkStep {
    WalkStep::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use proptest::prelude::*;

    const ID: PawnId = PawnId(0);

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    /// Solid ground at `y = 0` over `0..size` in x and z.
    fn flat_world(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(size - 1, 0, size - 1), 1);
        grid
    }

    fn solid(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(cell(x, y, z), 1, None);
    }

    /// A solid block at `pos`, ore of resource 1 or plain, and a mark on it.
    fn mark_block(grid: &mut VoxelGrid, marks: &mut MineMarks, pos: [i32; 3], ore: bool) {
        grid.mutator()
            .place(cell(pos[0], pos[1], pos[2]), 1, ore.then_some(1));
        marks.toggle(pos);
    }

    /// The feet at the middle of the floor of the cell `(x, y, z)`.
    fn feet_in(x: i32, y: i32, z: i32) -> Vec3 {
        Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5)
    }

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn column_of(to: Vec3) -> (i32, i32) {
        (to.x.floor() as i32, to.z.floor() as i32)
    }

    struct Run {
        walks: Vec<Vec3>,
        dig: Option<(BlockPos, Vec3, Pose)>,
    }

    /// Steps `pawn` from `start` until it digs, putting the feet on every walk target at once,
    /// as a pawn walking perfectly would.
    fn walk_until_dig(
        pawn: &mut Pawn,
        start: Vec3,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> Run {
        let mut feet = start;
        let mut walks = Vec::new();
        for _ in 0..400 {
            let pose = Pose::standing(feet, 0.0);
            match step(pawn, ID, &pose, marks, grid) {
                JobAction::Wait => {}
                JobAction::Walk { to } => {
                    walks.push(to);
                    feet = to;
                }
                JobAction::Dig { block, aim } => {
                    return Run {
                        walks,
                        dig: Some((block, aim, pose)),
                    };
                }
            }
        }
        Run { walks, dig: None }
    }

    /// Steps once at a fixed `pose` until the pawn digs, ignoring any walk it asks for.
    fn dig_from(
        pawn: &mut Pawn,
        pose: &Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
    ) -> Option<(BlockPos, Vec3)> {
        (0..5).find_map(|_| match step(pawn, ID, pose, marks, grid) {
            JobAction::Dig { block, aim } => Some((block, aim)),
            JobAction::Wait | JobAction::Walk { .. } => None,
        })
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = flat_world(20);
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);

        let action = step(&mut pawn, ID, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(pawn.selected_slot, 3, "nothing to dig, nothing equipped");
        assert_eq!(pawn.equipped_tool, None);
        let mut grid = grid;
        mark_block(&mut grid, &mut marks, [6, 1, 5], true);
        step(&mut pawn, ID, &pose, &mut marks, &grid);
        assert!(
            matches!(pawn.job, Job::Mine { .. }),
            "control: a mark gives the idle pawn work"
        );
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [14, 1, 12], true);
        mark_block(&mut grid, &mut marks, [6, 1, 5], true);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);

        let action = step(&mut pawn, ID, &pose, &mut marks, &grid);

        let Job::Mine { mark, .. } = &pawn.job else {
            panic!("the pawn did not start a job: {:?}", pawn.job);
        };
        assert_eq!(*mark, [6, 1, 5], "the nearer mark");
        assert_eq!(claimed_by(&marks, [6, 1, 5]), Some(ID));
        assert_eq!(claimed_by(&marks, [14, 1, 12]), None);
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
        assert!(pawn.equipped_tool.is_some(), "the pickaxe is in hand");
        assert!(!matches!(action, JobAction::Dig { .. }), "{action:?}");
    }

    #[test]
    fn an_unreachable_nearer_mark_is_skipped() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        for z in 0..20 {
            solid(&mut grid, 4, 1, z);
            solid(&mut grid, 4, 2, z);
        }
        mark_block(&mut grid, &mut marks, [6, 1, 5], true);
        mark_block(&mut grid, &mut marks, [2, 1, 15], true);
        let mut pawn = Pawn::default();
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);

        step(&mut pawn, ID, &pose, &mut marks, &grid);

        let Job::Mine { mark, .. } = &pawn.job else {
            panic!("the pawn did not start a job: {:?}", pawn.job);
        };
        assert_eq!(*mark, [2, 1, 15], "the one on this side of the wall");
        assert_eq!(claimed_by(&marks, [6, 1, 5]), None);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = flat_world(24);
        let mut marks = MineMarks::default();
        for x in 3..=5 {
            for y in 0..=2 {
                for z in 4..=6 {
                    solid(&mut grid, x, y, z);
                }
            }
        }
        mark_block(&mut grid, &mut marks, [4, 1, 5], true);
        let mut pawn = Pawn::default();
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);

        let buried = step(&mut pawn, ID, &pose, &mut marks, &grid);

        assert_eq!(buried, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(claimed_by(&marks, [4, 1, 5]), None);
        mark_block(&mut grid, &mut marks, [14, 1, 12], true);
        step(&mut pawn, ID, &pose, &mut marks, &grid);
        let Job::Mine { mark, .. } = &pawn.job else {
            panic!("the exposed mark was not taken: {:?}", pawn.job);
        };
        assert_eq!(
            *mark,
            [14, 1, 12],
            "the buried one is skipped, not blocking"
        );
        assert_eq!(claimed_by(&marks, [4, 1, 5]), None);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [6, 1, 5], true);
        let mut pawn = Pawn::default();

        let run = walk_until_dig(&mut pawn, feet_in(1, 1, 5), &mut marks, &grid);

        let (block, _, pose) = run.dig.expect("the pawn digs");
        assert_eq!(block, cell(6, 1, 5));
        assert_eq!(
            crate::routing::feet_cell(pose.feet),
            cell(5, 1, 5),
            "the nearest cell beside the block"
        );
        let mut columns: Vec<(i32, i32)> = run.walks.iter().map(|&to| column_of(to)).collect();
        columns.dedup();
        if columns.first() == Some(&(1, 5)) {
            columns.remove(0);
        }
        assert_eq!(columns, [(2, 5), (3, 5), (4, 5), (5, 5)]);
        for to in &run.walks {
            assert!(
                (to.x.fract() - 0.5).abs() < 1e-3,
                "{to:?} is not a cell centre"
            );
            assert!(
                (to.z.fract() - 0.5).abs() < 1e-3,
                "{to:?} is not a cell centre"
            );
        }
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let pose = Pose::standing(Vec3::new(5.5, 1.0, 4.3), 0.0);
        let aim_with = |hidden_neighbour: Option<(i32, i32, i32)>| {
            let mut grid = flat_world(20);
            let mut marks = MineMarks::default();
            mark_block(&mut grid, &mut marks, [6, 1, 5], true);
            if let Some((x, y, z)) = hidden_neighbour {
                solid(&mut grid, x, y, z);
            }
            let mut pawn = Pawn::default();
            let (block, aim) =
                dig_from(&mut pawn, &pose, &mut marks, &grid).expect("the pawn digs");
            assert_eq!(block, cell(6, 1, 5));
            aim
        };

        let nearest = aim_with(None);
        let nearest_exposed = aim_with(Some((6, 1, 4)));

        assert!(
            nearest.distance(Vec3::new(6.5, 1.5, 5.0)) < 1e-3,
            "the face toward -z: {nearest:?}"
        );
        assert!(
            nearest_exposed.distance(Vec3::new(6.0, 1.5, 5.5)) < 1e-3,
            "with -z covered, the face toward -x: {nearest_exposed:?}"
        );
    }

    /// Ground one or two blocks high over a 12 x 12 field: `heights[x * 12 + z]` is 0 or 1.
    fn rolling(heights: &[i32]) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..12 {
            for z in 0..12 {
                grid.mutator().fill_region(
                    cell(x, 0, z),
                    cell(x, heights[(x * 12 + z) as usize], z),
                    1,
                );
            }
        }
        grid
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            heights in proptest::collection::vec(0..=1i32, 144),
            block_column in (0..12i32, 0..12i32),
            start_column in (0..12i32, 0..12i32),
        ) {
            prop_assume!(block_column != start_column);
            let mut grid = rolling(&heights);
            let height_at = |(x, z): (i32, i32)| heights[(x * 12 + z) as usize];
            let block = [block_column.0, height_at(block_column) + 1, block_column.1];
            let mut marks = MineMarks::default();
            mark_block(&mut grid, &mut marks, block, true);
            let start = feet_in(start_column.0, height_at(start_column) + 1, start_column.1);
            let mut pawn = Pawn::default();

            let run = walk_until_dig(&mut pawn, start, &mut marks, &grid);

            let (dug, aim, pose) = run.dig.expect("every block on open ground can be reached");
            let centre = Vec3::new(block[0] as f32 + 0.5, block[1] as f32 + 0.5, block[2] as f32 + 0.5);
            prop_assert_eq!(dug, cell(block[0], block[1], block[2]));
            prop_assert!(pose.eye.distance(centre) <= MINE_REACH, "eye {:?} to {:?}", pose.eye, centre);
            prop_assert!(pose.eye.distance(aim) <= MINE_REACH, "eye {:?} to aim {:?}", pose.eye, aim);
            let on_block = (0..3).all(|axis| {
                aim[axis] >= block[axis] as f32 - 1e-3 && aim[axis] <= block[axis] as f32 + 1.0 + 1e-3
            });
            prop_assert!(on_block, "aim {:?} is off the block {:?}", aim, block);
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [14, 1, 5], true);
        let mut pawn = Pawn::default();
        let mut feet = feet_in(1, 1, 5);
        for _ in 0..4 {
            if let JobAction::Walk { to } =
                step(&mut pawn, ID, &Pose::standing(feet, 0.0), &mut marks, &grid)
            {
                feet = to;
            }
        }
        assert!(matches!(pawn.job, Job::Mine { .. }), "walking to the mark");

        marks.toggle([14, 1, 5]);
        let action = step(&mut pawn, ID, &Pose::standing(feet, 0.0), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(grid.is_solid_at(cell(14, 1, 5)), "the block is not removed");
        assert_eq!(marks.iter().count(), 0);
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [14, 1, 5], true);
        let mut pawn = Pawn::default();
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);
        step(&mut pawn, ID, &pose, &mut marks, &grid);
        assert!(matches!(pawn.job, Job::Mine { .. }));

        grid.mutator().remove(cell(14, 1, 5));
        marks.block_removed([14, 1, 5]);
        let action = step(&mut pawn, ID, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [6, 1, 5], true);
        let mut first = Pawn::default();
        let mut second = Pawn::default();
        let other = PawnId(1);
        step(
            &mut first,
            ID,
            &Pose::standing(feet_in(1, 1, 5), 0.0),
            &mut marks,
            &grid,
        );
        let pose = Pose::standing(feet_in(2, 1, 6), 0.0);

        let action = step(&mut second, other, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(second.job, Job::Idle, "the only mark is taken");
        assert_eq!(claimed_by(&marks, [6, 1, 5]), Some(ID));
        mark_block(&mut grid, &mut marks, [14, 1, 12], true);
        step(&mut second, other, &pose, &mut marks, &grid);
        let Job::Mine { mark, .. } = &second.job else {
            panic!("the free mark was not taken: {:?}", second.job);
        };
        assert_eq!(*mark, [14, 1, 12]);
        assert_eq!(claimed_by(&marks, [14, 1, 12]), Some(other));
        assert_eq!(
            claimed_by(&marks, [6, 1, 5]),
            Some(ID),
            "the first claim stands"
        );
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [14, 1, 5], true);
        let mut pawn = Pawn::default();
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);
        let mut released_at = None;

        for call in 1..=STALL_TICKS + 10 {
            let action = step(&mut pawn, ID, &pose, &mut marks, &grid);

            assert!(!matches!(action, JobAction::Dig { .. }), "{action:?}");
            if call > 1 && pawn.job == Job::Idle {
                released_at = Some(call);
                break;
            }
        }

        let released_at = released_at.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS..=STALL_TICKS + 3).contains(&released_at),
            "released after {released_at} ticks"
        );
        assert!(marks.contains([14, 1, 5]), "the mark stays");
        assert_eq!(claimed_by(&marks, [14, 1, 5]), None);
        assert_eq!(marks.unclaimed().collect::<Vec<_>>(), [[14, 1, 5]]);
    }

    #[test]
    fn steady_progress_is_not_a_stall() {
        let mut grid = flat_world(24);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [22, 1, 5], true);
        let mut pawn = Pawn::default();
        let mut feet = feet_in(1, 1, 5);

        for tick in 0..300 {
            let action = step(&mut pawn, ID, &Pose::standing(feet, 0.0), &mut marks, &grid);

            assert!(
                matches!(pawn.job, Job::Mine { .. }),
                "gave up at tick {tick} while still walking"
            );
            if let JobAction::Walk { to } = action {
                let toward = Vec3::new(to.x - feet.x, 0.0, to.z - feet.z);
                feet += toward.normalize_or_zero() * toward.length().min(0.03);
                feet.y = to.y;
            }
        }
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = flat_world(20);
        let mut marks = MineMarks::default();
        mark_block(&mut grid, &mut marks, [14, 1, 5], true);
        let mut pawn = Pawn::default();
        let pose = Pose::standing(feet_in(1, 1, 5), 0.0);
        step(&mut pawn, ID, &pose, &mut marks, &grid);
        assert!(matches!(pawn.job, Job::Mine { .. }));
        for x in 2..=4 {
            solid(&mut grid, x, 1, 5);
        }
        let blocked: Vec<(i32, i32)> = (2..=4).map(|x| (x, 5)).collect();

        let mut released = false;
        for _ in 0..3 {
            let action = step(&mut pawn, ID, &pose, &mut marks, &grid);

            if let JobAction::Walk { to } = action {
                assert!(
                    !blocked.contains(&column_of(to)),
                    "walks into the block at {to:?}"
                );
            }
            released |= pawn.job == Job::Idle;
            if released {
                break;
            }
        }

        assert!(released, "the route through the new blocks was kept");
        assert!(marks.contains([14, 1, 5]), "the mark stays");
        assert_eq!(claimed_by(&marks, [14, 1, 5]), None);
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let max = TURN_RATE / TICK_HZ as f32;
        let cases: [(f32, f32, f32); 9] = [
            (0.0, 1.5, max),
            (0.0, -1.5, -max),
            (0.0, 0.05, 0.05),
            (0.0, -0.05, -0.05),
            (3.0, -3.0, max),
            (-3.0, 3.0, -max),
            (1.0, 1.0, 0.0),
            (2.0, 2.07, 0.07),
            (7.0, 7.5, max),
        ];

        for (yaw, bearing, expected) in cases {
            let pose = Pose::standing(Vec3::new(2.0, 1.0, 2.0), yaw);
            let to = pose.feet + Vec3::new(bearing.sin(), 0.0, bearing.cos()) * 5.0;

            let walk = steer(&pose, to);

            assert!(
                (walk.yaw_delta - expected).abs() < 1e-4,
                "facing {yaw}, target at {bearing}: turned {} not {expected}",
                walk.yaw_delta
            );
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let yaw = 0.3_f32;
        let cases = [
            (0.0, 1.0),
            (30.0, 1.0),
            (-30.0, 1.0),
            (59.0, 1.0),
            (-59.0, 1.0),
            (61.0, 0.0),
            (-61.0, 0.0),
            (90.0, 0.0),
            (120.0, 0.0),
            (179.0, 0.0),
        ];

        for (off_degrees, expected) in cases {
            let pose = Pose::standing(Vec3::new(2.0, 1.0, 2.0), yaw);
            let bearing = yaw + f32::to_radians(off_degrees);
            let to = pose.feet + Vec3::new(bearing.sin() * 5.0, 3.0, bearing.cos() * 5.0);

            let walk = steer(&pose, to);

            assert_eq!(
                walk.forward, expected,
                "target {off_degrees} degrees off the facing"
            );
        }
    }
}
