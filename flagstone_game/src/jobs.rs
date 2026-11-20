//! What an idle pawn does with the marks: claim one, walk to it, dig it.

use std::f32::consts::{PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Route searches an idle pawn may spend per tick.
const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Half-angle, in radians, within which a pawn walks while it turns: 60 degrees.
const WALK_CONE: f32 = PI / 3.0;

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward the current waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// Turning speed in radians per second: one full turn per second.
pub const TURN_RATE: f32 = TAU;

/// Plain-data progress tracker for a walk; counts ticks, holds no clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StallGuard {
    /// Ticks since the feet last came closer to the current waypoint.
    pub ticks: u32,
    /// Closest horizontal approach to the current waypoint so far, in millimetres.
    pub closest_mm: u32,
}

impl Default for StallGuard {
    /// A guard that has seen no approach yet: the first reading counts as progress.
    fn default() -> Self {
        Self {
            ticks: 0,
            closest_mm: u32::MAX,
        }
    }
}

impl StallGuard {
    /// Records the current distance to the waypoint; true once progress has stopped for
    /// [`STALL_TICKS`].
    fn stalled(&mut self, distance_mm: u32) -> bool {
        if distance_mm < self.closest_mm {
            self.closest_mm = distance_mm;
            self.ticks = 0;
        } else {
            self.ticks += 1;
        }
        self.ticks >= STALL_TICKS
    }
}

/// What a pawn is doing when nobody is at its controls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Job {
    #[default]
    Idle,
    Mine {
        /// The claimed block.
        mark: BlockPos,
        /// Standable cells from where the pawn was to the stand cell, both included.
        route: Vec<BlockPos>,
        /// The waypoint being walked to.
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn is: the feet, the eye (the capsule centre) and the facing (yaw 0 faces +Z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub eye: Vec3,
    pub yaw: f32,
}

/// What the glue should do for the pawn this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    Walk { to: Vec3 },
    Dig { block: BlockPos, aim: Vec3 },
}

/// A turn and a walk for one tick: plain numbers the glue maps onto a character command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to add to the yaw this tick, positive turning from +Z toward +X.
    pub yaw_delta: f32,
    /// Forward axis, 0 to 1.
    pub forward: f32,
}

/// Advances `job` one tick for `pawn` and says what to do.
///
/// Idle: claims the nearest unclaimed, exposed, routable mark in `marks`, equips the tool
/// and starts a job. Mine: walks the route, then digs; releases the claim and goes idle
/// when the mark is gone, a waypoint stops being standable or the walk stalls.
pub fn step(
    job: &mut Job,
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(job, Job::Idle) {
        let Some(started) = claim_nearest(pawn, pose, marks, grid) else {
            return JobAction::Wait;
        };
        *job = started;
    }
    follow(job, pose, marks, grid)
}

/// Claims the nearest mark that has a route to a stand cell, trying at most
/// [`ROUTE_ATTEMPTS_PER_TICK`] routes and resuming after them on the next tick.
fn claim_nearest(
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let feet = feet_cell(pose.feet);
    let mut candidates: Vec<[i32; 3]> = marks
        .unclaimed()
        .filter(|pos| {
            exposed_faces(grid, BlockPos::from_array(*pos))
                .next()
                .is_some()
        })
        .collect();
    candidates.sort_by_key(|pos| (feet.distance_squared(BlockPos::from_array(*pos)), *pos));
    let first = if pawn.search_cursor < candidates.len() {
        pawn.search_cursor
    } else {
        0
    };

    let mut tried = 0;
    for pos in candidates.iter().skip(first).take(ROUTE_ATTEMPTS_PER_TICK) {
        tried += 1;
        let block = BlockPos::from_array(*pos);
        let found = route(
            grid,
            feet,
            |cell| is_stand_cell(grid, block, cell),
            block,
            ROUTE_NODE_BUDGET,
        );
        if let Some(route) = found {
            pawn.search_cursor = 0;
            marks.claim(*pos, pawn.id);
            pawn.select_slot(TOOL_SLOT);
            return Some(Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            });
        }
    }
    pawn.search_cursor = first + tried;
    None
}

/// A standable cell from which `block` is inside the mining reach: within one column of it
/// and with the feet from one below to two above it, never on the block itself.
fn is_stand_cell(grid: &VoxelGrid, block: BlockPos, cell: BlockPos) -> bool {
    let off = cell - block;
    off.x.abs() <= 1
        && off.z.abs() <= 1
        && (-1..=2).contains(&off.y)
        && off != BlockPos::Y
        && standable(grid, cell)
}

/// The directions in which the solid `block` has an air neighbour.
fn exposed_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    grid.is_solid_at(block)
        .then_some(())
        .into_iter()
        .flat_map(|()| {
            [
                BlockPos::X,
                BlockPos::NEG_X,
                BlockPos::Y,
                BlockPos::NEG_Y,
                BlockPos::Z,
                BlockPos::NEG_Z,
            ]
        })
        .filter(move |dir| !grid.is_solid_at(block + *dir))
}

fn waypoint_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

/// Walks a claimed route, then digs; gives the claim up when it cannot continue.
fn follow(job: &mut Job, pose: &Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        return JobAction::Wait;
    };
    let block = *mark;
    let route_is_open = route[*next..].iter().all(|cell| standable(grid, *cell));
    if !marks.contains(block.to_array()) || !route_is_open {
        marks.release(block.to_array());
        *job = Job::Idle;
        return JobAction::Wait;
    }

    while *next < route.len()
        && horizontal_distance(pose.feet, waypoint_centre(route[*next])) <= ARRIVE_RADIUS
    {
        *next += 1;
        *stall = StallGuard::default();
    }
    let Some(&waypoint) = route.get(*next) else {
        let aim = exposed_faces(grid, block)
            .map(|dir| block.as_vec3() + Vec3::splat(0.5) + dir.as_vec3() * 0.5)
            .min_by(|a, b| {
                a.distance_squared(pose.eye)
                    .total_cmp(&b.distance_squared(pose.eye))
            });
        if let Some(aim) = aim {
            return JobAction::Dig { block, aim };
        }
        marks.release(block.to_array());
        *job = Job::Idle;
        return JobAction::Wait;
    };

    let to = waypoint_centre(waypoint);
    if stall.stalled((horizontal_distance(pose.feet, to) * 1000.0) as u32) {
        marks.release(block.to_array());
        *job = Job::Idle;
        return JobAction::Wait;
    }
    JobAction::Walk { to }
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

/// The turn and walk that take `pose` toward `to`: turns at [`TURN_RATE`], clamped per tick,
/// and walks at full speed only once within 60 degrees of facing it.
pub fn steer(pose: &Pose, to: Vec3) -> WalkStep {
    let offset = to - pose.feet;
    let heading = offset.x.atan2(offset.z);
    let turn = (heading - pose.yaw + PI).rem_euclid(TAU) - PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: turn.clamp(-per_tick, per_tick),
        forward: if turn.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use proptest::prelude::*;

    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, PawnId, TOOL_SLOT};
    use crate::tools::STARTING_TOOL;

    const EYE_HEIGHT: f32 = 1.15;
    const EPS: f32 = 1e-4;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn centre(c: BlockPos) -> Vec3 {
        Vec3::new(c.x as f32 + 0.5, c.y as f32, c.z as f32 + 0.5)
    }

    fn pose_at(feet: Vec3) -> Pose {
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    fn strip(len: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..len {
            grid.mutator().place(cell(x, 0, 0), 1, None);
        }
        grid
    }

    fn plane(size: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..size {
            for z in 0..size {
                grid.mutator().place(cell(x, 0, z), 1, None);
            }
        }
        grid
    }

    #[derive(Debug, PartialEq)]
    enum Claim {
        Unmarked,
        Free,
        By(PawnId),
    }

    struct World {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
        pose: Pose,
    }

    impl World {
        fn new(grid: VoxelGrid, pawn_at: BlockPos) -> Self {
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::Idle,
                pose: pose_at(centre(pawn_at)),
            }
        }

        fn mark(&mut self, block: BlockPos) {
            self.marks.toggle(block.to_array());
        }

        /// One tick; a walk lands the feet on the waypoint, as a perfect body would.
        fn tick(&mut self) -> JobAction {
            let action = step(
                &mut self.job,
                &mut self.pawn,
                &self.pose,
                &mut self.marks,
                &self.grid,
            );
            if let JobAction::Walk { to } = action {
                self.pose = pose_at(to);
            }
            action
        }

        /// Ticks until the pawn digs; the waypoints walked to and the dig.
        fn until_dig(&mut self) -> (Vec<Vec3>, BlockPos, Vec3) {
            let mut walked = Vec::new();
            for _ in 0..300 {
                match self.tick() {
                    JobAction::Walk { to } => walked.push(to),
                    JobAction::Dig { block, aim } => return (walked, block, aim),
                    JobAction::Wait => {}
                }
            }
            panic!("the pawn never dug; walked {walked:?}, job {:?}", self.job);
        }

        fn claimant(&self, block: BlockPos) -> Claim {
            self.marks
                .iter()
                .find(|(pos, _)| *pos == block.to_array())
                .map_or(Claim::Unmarked, |(_, mark)| {
                    mark.claimed_by.map_or(Claim::Free, Claim::By)
                })
        }
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut world = World::new(strip(6), cell(0, 1, 0));
        world.pawn.select_slot(3);

        for _ in 0..5 {
            let action = world.tick();

            assert_eq!(action, JobAction::Wait);
        }

        assert_eq!(world.job, Job::Idle);
        assert_eq!(world.pawn.equipped_tool, None, "nothing to equip for");
        assert_eq!(world.pose, pose_at(centre(cell(0, 1, 0))));
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = VoxelGrid::new(16);
        for x in 0..9 {
            for z in 0..9 {
                for y in 0..3 {
                    grid.mutator().place(cell(x, y, z), 1, None);
                }
            }
        }
        let mut world = World::new(grid, cell(1, 3, 1));
        world.mark(cell(5, 1, 5));

        let action = world.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(world.job, Job::Idle);
        assert_eq!(world.claimant(cell(5, 1, 5)), Claim::Free);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut world = World::new(plane(14), cell(12, 1, 12));
        world.mark(cell(2, 1, 2));
        world.mark(cell(10, 1, 10));
        for block in [cell(2, 1, 2), cell(10, 1, 10)] {
            world.grid.mutator().place(block, 1, None);
        }
        for y in 1..=3 {
            world.grid.mutator().place(cell(12, y, 11), 1, None);
        }
        world.mark(cell(12, 3, 11));
        world.pawn.select_slot(3);

        world.tick();

        assert_eq!(
            world.claimant(cell(12, 3, 11)),
            Claim::Free,
            "the nearer mark atop the pillar has no stand cell"
        );
        assert_eq!(world.claimant(cell(10, 1, 10)), Claim::By(PawnId(0)));
        assert_eq!(
            world.claimant(cell(2, 1, 2)),
            Claim::Free,
            "the far mark stays free"
        );
        assert!(matches!(world.job, Job::Mine { mark, .. } if mark == cell(10, 1, 10)));
        assert_eq!(world.pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(world.pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(4, 1, 0), 1, None);
        world.mark(cell(4, 1, 0));

        let (walked, block, _) = world.until_dig();

        assert_eq!(block, cell(4, 1, 0));
        let stand = world.pose.feet;
        assert_eq!(
            (stand.x, stand.z),
            (3.5, 0.5),
            "the nearest stand cell by route length is the one before the block"
        );
        assert_eq!(walked.last().map(|w| (w.x, w.z)), Some((3.5, 0.5)));
        for pair in walked.windows(2) {
            assert!(
                (pair[1].x - pair[0].x - 1.0).abs() < EPS && pair[1].z == pair[0].z,
                "each waypoint is the next cell along: {walked:?}"
            );
        }
        assert!(walked.len() >= 3, "it walked, not teleported: {walked:?}");
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(4, 1, 0), 1, None);
        world.mark(cell(4, 1, 0));

        let (_, block, aim) = world.until_dig();

        assert_eq!(block, cell(4, 1, 0));
        assert!(
            (aim - Vec3::new(4.0, 1.5, 0.5)).length() < EPS,
            "the west face centre, not the top: {aim}"
        );
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 2i32..10,
            bz in 2i32..10,
            height in 1i32..=2,
            sx in 0i32..12,
            sz in 0i32..12,
        ) {
            prop_assume!((sx, sz) != (bx, bz));
            let mut grid = plane(12);
            for y in 1..=height {
                grid.mutator().place(cell(bx, y, bz), 1, None);
            }
            let block = cell(bx, height, bz);
            let mut world = World::new(grid, cell(sx, 1, sz));
            world.mark(block);

            let (_, dug, aim) = world.until_dig();

            prop_assert_eq!(dug, block);
            let eye = world.pose.eye;
            prop_assert!((aim - eye).length() <= MINE_REACH, "aim {aim} from eye {eye}");
            prop_assert!(
                (centre(block) + Vec3::new(0.0, 0.5, 0.0) - eye).length() <= MINE_REACH,
                "block centre from eye {eye}"
            );
            let stand = world.pose.feet;
            prop_assert!((stand.x - (bx as f32 + 0.5)).abs() <= 1.5);
            prop_assert!((stand.z - (bz as f32 + 0.5)).abs() <= 1.5);
            prop_assert!(stand.y >= (height - 1) as f32 && stand.y <= (height + 2) as f32);
        }
    }

    fn mid_walk() -> World {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(6, 1, 0), 1, None);
        world.mark(cell(6, 1, 0));
        world.tick();
        world.tick();
        assert!(
            matches!(world.job, Job::Mine { .. }),
            "walking: {:?}",
            world.job
        );
        world
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut world = mid_walk();
        let before = world.pose;
        world.marks.toggle([6, 1, 0]);

        let action = world.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(world.job, Job::Idle);
        assert_eq!(world.pose, before, "the pawn stopped");
        assert!(
            world.grid.is_solid_at(cell(6, 1, 0)),
            "the block is not removed"
        );
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut world = mid_walk();
        world.grid.mutator().remove(cell(6, 1, 0));
        world.marks.block_removed([6, 1, 0]);

        let action = world.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(world.job, Job::Idle);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(6, 1, 0), 1, None);
        world.mark(cell(6, 1, 0));
        world.tick();
        let mut second = Pawn {
            id: PawnId(1),
            ..Pawn::default()
        };
        let mut second_job = Job::Idle;

        let action = step(
            &mut second_job,
            &mut second,
            &pose_at(centre(cell(1, 1, 0))),
            &mut world.marks,
            &world.grid,
        );

        assert_eq!(action, JobAction::Wait);
        assert_eq!(second_job, Job::Idle);
        assert_eq!(world.claimant(cell(6, 1, 0)), Claim::By(PawnId(0)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(6, 1, 0), 1, None);
        world.mark(cell(6, 1, 0));
        let mut released_at = None;

        for tick in 0..STALL_TICKS + 20 {
            step(
                &mut world.job,
                &mut world.pawn,
                &world.pose,
                &mut world.marks,
                &world.grid,
            );
            if world.job == Job::Idle {
                released_at = Some(tick);
                break;
            }
        }

        let released_at = released_at.expect("the stalled pawn lets go");
        assert!(
            (STALL_TICKS - 2..=STALL_TICKS + 2).contains(&released_at),
            "released on tick {released_at}"
        );
        assert!(world.marks.contains([6, 1, 0]), "the mark stays");
        assert_eq!(world.claimant(cell(6, 1, 0)), Claim::Free);
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut world = mid_walk();
        let Job::Mine { route, next, .. } = world.job.clone() else {
            panic!("walking");
        };
        let upcoming = route[next + 1];
        world.grid.mutator().remove(upcoming - cell(0, 1, 0));

        let action = world.tick();

        assert_eq!(action, JobAction::Wait);
        assert_eq!(world.job, Job::Idle);
        assert!(world.marks.contains([6, 1, 0]), "the mark stays");
        assert_eq!(world.claimant(cell(6, 1, 0)), Claim::Free);
    }

    #[test]
    fn a_stall_counter_resets_while_the_feet_make_progress() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(6, 1, 0), 1, None);
        world.mark(cell(6, 1, 0));
        world.tick();

        for _ in 0..STALL_TICKS + 30 {
            step(
                &mut world.job,
                &mut world.pawn,
                &world.pose,
                &mut world.marks,
                &world.grid,
            );
            world.pose = pose_at(world.pose.feet + Vec3::X * 0.003);

            assert!(matches!(world.job, Job::Mine { .. }), "{:?}", world.job);
            assert_eq!(world.claimant(cell(6, 1, 0)), Claim::By(PawnId(0)));
        }
    }

    /// A claimed walk on a strip, with the route and the waypoint being walked to.
    fn claimed_walk() -> (World, Vec<BlockPos>, usize) {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(6, 1, 0), 1, None);
        world.mark(cell(6, 1, 0));
        world.tick();
        let Job::Mine { route, next, .. } = world.job.clone() else {
            panic!("claimed: {:?}", world.job);
        };
        (world, route, next)
    }

    fn next_waypoint(world: &World) -> usize {
        let Job::Mine { next, .. } = world.job else {
            panic!("still walking: {:?}", world.job);
        };
        next
    }

    #[test]
    fn a_waypoint_counts_as_reached_within_the_horizontal_arrive_radius_whatever_the_height() {
        let (mut world, route, next) = claimed_walk();
        let off = Vec3::new(ARRIVE_RADIUS - 0.05, 1.0, 0.0);
        world.pose = pose_at(centre(route[next]) + off);

        world.tick();

        assert_eq!(next_waypoint(&world), next + 1);
    }

    #[test]
    fn a_waypoint_is_not_reached_outside_the_horizontal_arrive_radius() {
        let (mut world, route, next) = claimed_walk();
        let off = Vec3::new(ARRIVE_RADIUS + 0.15, 0.0, 0.0);
        world.pose = pose_at(centre(route[next]) + off);

        let action = world.tick();

        assert_eq!(next_waypoint(&world), next);
        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
    }

    #[test]
    fn at_most_four_unreachable_marks_are_tried_per_tick() {
        let mut grid = plane(14);
        let pillars = [2, 4, 6, 8, 10];
        for x in pillars {
            for y in 1..=3 {
                grid.mutator().place(cell(x, y, 0), 1, None);
            }
        }
        grid.mutator().place(cell(12, 1, 12), 1, None);
        let mut world = World::new(grid, cell(0, 1, 0));
        for x in pillars {
            world.mark(cell(x, 3, 0));
        }
        world.mark(cell(12, 1, 12));

        let first = world.tick();

        assert_eq!(first, JobAction::Wait);
        assert_eq!(world.job, Job::Idle);
        assert_eq!(world.claimant(cell(12, 1, 12)), Claim::Free);

        for _ in 0..3 {
            world.tick();
        }

        assert_eq!(
            world.claimant(cell(12, 1, 12)),
            Claim::By(PawnId(0)),
            "a later tick reaches the reachable mark"
        );
        for x in pillars {
            assert_eq!(world.claimant(cell(x, 3, 0)), Claim::Free);
        }
    }

    #[test]
    fn equally_near_reachable_marks_go_to_the_lower_key() {
        let mut world = World::new(plane(14), cell(5, 1, 5));
        for block in [cell(7, 1, 5), cell(3, 1, 5)] {
            world.grid.mutator().place(block, 1, None);
            world.mark(block);
        }

        world.tick();

        assert_eq!(world.claimant(cell(3, 1, 5)), Claim::By(PawnId(0)));
        assert_eq!(world.claimant(cell(7, 1, 5)), Claim::Free);
    }

    fn facing(yaw: f32, angle_off: f32) -> (Pose, Vec3) {
        let pose = Pose {
            yaw,
            ..pose_at(Vec3::ZERO)
        };
        let heading = yaw + angle_off;
        (pose, Vec3::new(heading.sin(), 0.0, heading.cos()) * 5.0)
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let per_tick = TURN_RATE / TICK_HZ as f32;

        let (pose, left) = facing(0.0, 1.5);
        let (_, right) = facing(0.0, -1.5);
        let (past_pi_pose, past_pi) = facing(3.0, 0.4);
        let (before_pi_pose, before_pi) = facing(-3.0, -0.4);

        let to_plus_x = steer(&pose, left).yaw_delta;
        let to_minus_x = steer(&pose, right).yaw_delta;
        let across_the_seam = steer(&past_pi_pose, past_pi).yaw_delta;
        let back_across = steer(&before_pi_pose, before_pi).yaw_delta;

        assert!((to_plus_x - per_tick).abs() < EPS, "{to_plus_x}");
        assert!((to_minus_x + per_tick).abs() < EPS, "{to_minus_x}");
        assert!(
            (across_the_seam - per_tick).abs() < EPS,
            "3.0 to 3.4 rad is a short turn the positive way: {across_the_seam}"
        );
        assert!(
            (back_across + per_tick).abs() < EPS,
            "-3.0 to -3.4 rad is a short turn the negative way: {back_across}"
        );
    }

    #[test]
    fn steer_aims_at_the_target_relative_to_the_feet_not_the_origin() {
        let pose = pose_at(Vec3::new(10.0, 0.0, 10.0));

        let step = steer(&pose, Vec3::new(10.0, 0.0, 15.0));

        assert!(step.yaw_delta.abs() < EPS, "{}", step.yaw_delta);
        assert_eq!(step.forward, 1.0);
    }

    #[test]
    fn exposed_faces_lists_only_the_directions_with_air() {
        let mut grid = strip(8);
        grid.mutator().place(cell(3, 1, 0), 1, None);
        grid.mutator().place(cell(4, 1, 0), 1, None);

        let faces: Vec<BlockPos> = exposed_faces(&grid, cell(4, 1, 0)).collect();

        assert_eq!(
            faces,
            vec![BlockPos::X, BlockPos::Y, BlockPos::Z, BlockPos::NEG_Z]
        );
        assert_eq!(
            exposed_faces(&grid, cell(4, 2, 0)).count(),
            0,
            "air has none"
        );
    }

    #[test]
    fn the_dig_aims_at_the_top_face_of_a_block_set_in_the_ground() {
        let mut grid = plane(9);
        for x in 0..9 {
            for z in 0..9 {
                grid.mutator().place(cell(x, 1, z), 1, None);
            }
        }
        let mut world = World::new(grid, cell(0, 2, 0));
        world.mark(cell(4, 1, 4));

        let (_, block, aim) = world.until_dig();

        assert_eq!(block, cell(4, 1, 4));
        assert!(
            (aim - Vec3::new(4.5, 2.0, 4.5)).length() < EPS,
            "only the top is exposed: {aim}"
        );
    }

    #[test]
    fn a_search_cursor_at_the_end_of_the_candidates_wraps_to_the_start() {
        let mut world = World::new(strip(8), cell(0, 1, 0));
        world.grid.mutator().place(cell(4, 1, 0), 1, None);
        world.mark(cell(4, 1, 0));
        world.pawn.search_cursor = 1;

        world.tick();

        assert_eq!(world.claimant(cell(4, 1, 0)), Claim::By(PawnId(0)));
        assert_eq!(world.pawn.search_cursor, 0);
    }

    #[test]
    fn steer_does_not_overshoot_a_small_remaining_angle() {
        let (pose, to) = facing(0.7, 0.01);

        let step = steer(&pose, to);

        assert!((step.yaw_delta - 0.01).abs() < EPS, "{}", step.yaw_delta);
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let (pose, ahead) = facing(0.4, 0.0);
        let (_, near) = facing(0.4, 50f32.to_radians());
        let (_, wide) = facing(0.4, -70f32.to_radians());
        let (_, side) = facing(0.4, FRAC_PI_2);
        let (_, behind) = facing(0.4, PI);

        let on_target = steer(&pose, ahead);
        let nearly = steer(&pose, near);
        let off = steer(&pose, wide);
        let beside = steer(&pose, side);
        let away = steer(&pose, behind);

        assert_eq!(
            (on_target.forward, on_target.yaw_delta.abs() < EPS),
            (1.0, true)
        );
        assert_eq!(nearly.forward, 1.0, "within 60 degrees: walk while turning");
        assert!(nearly.yaw_delta > 0.0);
        for turning in [off, beside, away] {
            assert_eq!(turning.forward, 0.0, "turn in place");
            assert!(turning.yaw_delta.abs() > 0.0);
        }
    }
}
