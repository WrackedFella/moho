//! A pawn's own work: claiming a marked block, walking to it and digging it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

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
    /// The nearest the feet have been to the waypoint, if measured yet.
    pub closest: Option<f32>,
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
///
/// A `Dig` ends the job: the pawn goes idle and the claim is released, so a mark that
/// survives the dig is offered again.
pub fn step(
    id: PawnId,
    job: &mut Job,
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if *job == Job::Idle {
        *job = claim_nearest(id, pawn, pose, marks, grid);
    }
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        return JobAction::Wait;
    };
    let mark = *mark;

    if !marks.contains(mark) {
        *job = Job::Idle;
        return JobAction::Wait;
    }

    while route
        .get(*next)
        .is_some_and(|&cell| horizontal_distance(pose.feet, cell_centre(cell)) <= ARRIVE_RADIUS)
    {
        *next += 1;
        *stall = StallGuard::default();
    }

    let Some(&waypoint) = route.get(*next) else {
        let block = BlockPos::from(mark);
        let aim = nearest_exposed_face(grid, block, pose.eye);
        marks.release(mark);
        *job = Job::Idle;
        return aim.map_or(JobAction::Wait, |aim| JobAction::Dig { block, aim });
    };

    let distance = horizontal_distance(pose.feet, cell_centre(waypoint));
    let progressed = stall
        .closest
        .is_none_or(|closest| distance < closest - PROGRESS_EPSILON);
    if progressed {
        *stall = StallGuard {
            ticks: 0,
            closest: Some(distance),
        };
    } else {
        stall.ticks += 1;
    }

    if stall.ticks >= STALL_TICKS || !standable(grid, waypoint) {
        marks.release(mark);
        *job = Job::Idle;
        return JobAction::Wait;
    }
    JobAction::Walk {
        to: cell_centre(waypoint),
    }
}

/// How to move from `pose` toward `to` in one tick.
pub fn steer(pose: &Pose, to: Vec3) -> WalkStep {
    use std::f32::consts::{PI, TAU};

    let bearing = (to.x - pose.feet.x).atan2(to.z - pose.feet.z);
    let error = (bearing - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-max_turn, max_turn),
        forward: if error.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

/// Smallest closing of the distance to a waypoint that counts as progress, in metres.
const PROGRESS_EPSILON: f32 = 1e-3;

/// How far off the target's bearing a pawn may be and still walk, rather than turn in place.
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// Most route searches an idle pawn starts in one tick.
const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

const FACE_DIRECTIONS: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

fn cell_centre(cell: BlockPos) -> Vec3 {
    cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// The centre of the face of `block` nearest `eye` among those with air beyond them.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Option<Vec3> {
    let centre = block.as_vec3() + Vec3::splat(0.5);
    FACE_DIRECTIONS
        .into_iter()
        .filter(|&dir| !grid.is_solid_at(block + dir))
        .map(|dir| centre + dir.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

/// Claims the nearest marked block that has an exposed face and a route to a stand cell,
/// equips the tool and returns the job; `Idle` when there is none. Tries at most
/// `ROUTE_ATTEMPTS_PER_TICK` routes.
fn claim_nearest(
    id: PawnId,
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Job {
    let start = feet_cell(pose.feet);
    let mut candidates = marks.unclaimed();
    candidates.sort_by_key(|&pos| (BlockPos::from(pos) - start).length_squared());

    let reachable = candidates
        .into_iter()
        .filter(|&pos| {
            let block = BlockPos::from(pos);
            FACE_DIRECTIONS
                .iter()
                .any(|&dir| !grid.is_solid_at(block + dir))
        })
        .take(ROUTE_ATTEMPTS_PER_TICK)
        .find_map(|pos| {
            let block = BlockPos::from(pos);
            let is_stand_cell = |cell: BlockPos| {
                (cell.x - block.x).abs() <= 1
                    && (cell.z - block.z).abs() <= 1
                    && (block.y - 1..=block.y + 2).contains(&cell.y)
            };
            route(grid, start, is_stand_cell, block, ROUTE_NODE_BUDGET).map(|route| (pos, route))
        });
    let Some((mark, route)) = reachable else {
        return Job::Idle;
    };

    assert!(marks.claim(mark, id), "an unclaimed mark can be claimed");
    pawn.select_slot(TOOL_SLOT);
    Job::Mine {
        mark,
        route,
        next: 0,
        stall: StallGuard::default(),
    }
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
        let mut rig = Rig::new(floor(0..=16, 0..=8), cell(14, 1, 2));
        rig.mark(5, 0, 2);
        rig.mark(12, 0, 2);
        rig.pawn.select_slot(3);

        rig.step();

        let Job::Mine { mark, route, .. } = &rig.job else {
            panic!("expected a mine job, got {:?}", rig.job);
        };
        assert_eq!(
            *mark,
            [12, 0, 2],
            "the nearer mark, though not first in key order"
        );
        assert_eq!(
            route.last(),
            Some(&cell(13, 1, 2)),
            "the nearest stand cell"
        );
        assert_eq!(rig.claim_of([12, 0, 2]), held_by(ID));
        assert_eq!(rig.claim_of([5, 0, 2]), FREE);
        assert_eq!(rig.pawn.selected_slot, crate::pawn::TOOL_SLOT);
        assert_eq!(rig.pawn.equipped_tool, Some(crate::tools::STARTING_TOOL));
    }

    /// A floor split by a two-block wall across `x = 8`, the pawn on the west side at `(6, 1, 4)`.
    fn walled_rig() -> Rig {
        let mut grid = floor(0..=16, 0..=8);
        for z in 0..=8 {
            place(&mut grid, 8, 1, z);
            place(&mut grid, 8, 2, z);
        }
        Rig::new(grid, cell(6, 1, 4))
    }

    #[test]
    fn a_nearer_mark_behind_a_wall_is_skipped_for_a_routable_one() {
        let mut rig = walled_rig();
        rig.mark(9, 0, 4);
        rig.mark(0, 0, 4);

        rig.step();

        assert_eq!(rig.mined_mark(), Some([0, 0, 4]));
        assert_eq!(
            rig.claim_of([9, 0, 4]),
            FREE,
            "the walled-off mark stays free"
        );
    }

    #[test]
    fn at_most_four_unroutable_marks_are_tried_in_a_tick() {
        let mut rig = walled_rig();
        for z in 2..=6 {
            rig.mark(9, 0, z);
        }
        rig.mark(0, 0, 4);

        let first = rig.step();

        assert_eq!(first, JobAction::Wait);
        assert_eq!(
            rig.job,
            Job::Idle,
            "five failed searches do not fit in a tick"
        );
        assert_eq!(rig.claim_of([0, 0, 4]), FREE);
        for z in 2..=6 {
            assert_eq!(rig.claim_of([9, 0, z]), FREE, "walled mark at z = {z}");
        }

        let mut within_cap = walled_rig();
        for z in 3..=5 {
            within_cap.mark(9, 0, z);
        }
        within_cap.mark(0, 0, 4);
        within_cap.step();

        assert_eq!(
            within_cap.mined_mark(),
            Some([0, 0, 4]),
            "the fourth search is still allowed"
        );
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
        let standing_on = horizontal(standing_in(cell(2, 1, 2)).feet);
        let mut waypoints: Vec<(f32, f32)> = walks
            .into_iter()
            .filter(|&(x, z)| (x - standing_on.0).hypot(z - standing_on.1) > ARRIVE_RADIUS)
            .collect();
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
            (STALL_TICKS..=STALL_TICKS + 2).contains(&tick),
            "gave up at tick {tick}"
        );
        assert!(rig.marks.contains([5, 0, 2]), "the mark stays");
        assert_eq!(rig.claim_of([5, 0, 2]), FREE, "the claim does not");
    }

    #[test]
    fn a_pawn_already_in_a_stand_cell_digs_on_the_first_step_without_walking() {
        let mut rig = Rig::new(floor(0..=10, 0..=8), cell(4, 1, 2));
        rig.mark(5, 0, 2);

        let first = rig.step();

        assert!(
            matches!(first, JobAction::Dig { block, .. } if block == cell(5, 0, 2)),
            "expected an immediate dig, got {first:?}"
        );
        assert_eq!(rig.job, Job::Idle);
        assert_eq!(rig.claim_of([5, 0, 2]), FREE, "the claim is released");
    }

    #[test]
    fn a_surviving_mark_is_unclaimed_after_a_dig_and_claimed_again_by_the_next_step() {
        let mut rig = Rig::new(floor(0..=10, 0..=8), cell(4, 1, 2));
        rig.mark(5, 0, 2);
        let first = rig.step();
        assert!(matches!(first, JobAction::Dig { .. }), "setup: {first:?}");

        assert!(rig.marks.contains([5, 0, 2]), "the mark stays after a dig");
        assert_eq!(rig.claim_of([5, 0, 2]), FREE);

        rig.pawn.select_slot(3);
        let second = rig.step();

        assert!(
            matches!(second, JobAction::Dig { block, .. } if block == cell(5, 0, 2)),
            "the surviving mark is dug again, got {second:?}"
        );
        assert_eq!(
            rig.pawn.selected_slot,
            crate::pawn::TOOL_SLOT,
            "re-claiming draws the tool"
        );
        assert_eq!(rig.job, Job::Idle);
        assert_eq!(rig.claim_of([5, 0, 2]), FREE);
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
