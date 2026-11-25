//! What a pawn does when nobody possesses it: take a marked block, walk to it, dig it.
//!
//! The loop reads the marks, the grid and the pawn's pose and answers with a plain action;
//! the caller applies it through the pawn's own movement and mining.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{FRAC_PI_3, PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::crew::Crew;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{
    ROUTE_NODE_BUDGET, feet_cell, is_step, route, standable, standing_cell, walkable,
};

/// How close to a waypoint's centre, horizontally in metres, the feet count as arrived.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Turning speed, radians per second: one turn per second.
pub const TURN_RATE: f32 = TAU;

/// How far off its heading, in radians, a pawn still walks instead of turning in place.
const WALK_CONE: f32 = FRAC_PI_3;

/// Metres closer to a waypoint that count as progress.
const PROGRESS_EPSILON: f32 = 1e-3;

/// The eye's height above the feet, metres.
pub const EYE_HEIGHT: f32 = 1.15;

pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn stands and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    /// Radians; 0 faces +Z and positive turns toward +X.
    pub yaw: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    pub ticks_without_progress: u32,
    /// The nearest the feet have been to the current waypoint, horizontally.
    pub closest: Option<f32>,
}

/// The marks that failed to route from `from` at grid `revision`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unreachable {
    pub revision: u64,
    pub from: BlockPos,
    pub marks: BTreeSet<[i32; 3]>,
}

impl Unreachable {
    fn new(revision: u64, from: BlockPos) -> Self {
        Self {
            revision,
            from,
            marks: BTreeSet::new(),
        }
    }

    /// Empties the memo unless it was recorded at this `revision` from this cell.
    fn refresh(&mut self, revision: u64, from: BlockPos) {
        if self.revision != revision || self.from != from {
            *self = Self::new(revision, from);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Idle {
        /// The marks this idle pawn has found no route to, so it does not search for them
        /// again until the terrain or its cell changes.
        unreachable: Unreachable,
    },
    Mine {
        mark: [i32; 3],
        /// Cells to walk, from the pawn's cell to the stand cell.
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
        /// The grid revision the route was last checked at.
        revision: u64,
    },
}

impl Default for Job {
    fn default() -> Self {
        Self::Idle {
            unreachable: Unreachable::default(),
        }
    }
}

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
/// and z, between one below and two above it, and not standing on it, so the dig never takes
/// the pawn's own ground.
pub fn is_stand_cell(block: BlockPos, cell: BlockPos) -> bool {
    let offset = cell - block;
    offset.x.abs() <= 1
        && offset.z.abs() <= 1
        && (-1..=2).contains(&offset.y)
        && offset != BlockPos::Y
}

const NEIGHBOURS: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

fn has_exposed_face(grid: &VoxelGrid, block: BlockPos) -> bool {
    NEIGHBOURS.iter().any(|&d| !grid.is_solid_at(block + d))
}

fn eye(feet: Vec3) -> Vec3 {
    feet + Vec3::Y * EYE_HEIGHT
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a.x - b.x).hypot(a.z - b.z)
}

fn release(job: &mut Job, marks: &mut MineMarks, mark: [i32; 3]) {
    marks.release(mark);
    *job = Job::default();
}

/// The centre of the exposed face of `block` nearest `eye`.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Vec3 {
    let centre = block.as_vec3() + Vec3::splat(0.5);
    NEIGHBOURS
        .iter()
        .filter(|&&d| !grid.is_solid_at(block + d))
        .map(|&d| centre + d.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
        .unwrap_or(centre)
}

/// Takes the nearest routable unclaimed mark the memo does not hold, trying at most
/// [`ROUTE_ATTEMPTS_PER_TICK`] routes and memoising each failure. The memo is emptied first
/// when the terrain or the pawn's cell is not what it recorded.
fn claim_nearest(
    unreachable: &mut Unreachable,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let start = start_cell(grid, pose);
    unreachable.refresh(grid.revision(), start);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .filter(|mark| !unreachable.marks.contains(mark))
        .map(BlockPos::from_array)
        .filter(|&block| grid.is_solid_at(block) && has_exposed_face(grid, block))
        .collect();
    // Stable, so equal distances keep key order.
    candidates.sort_by_key(|&block| (block - start).length_squared());

    for block in candidates.into_iter().take(ROUTE_ATTEMPTS_PER_TICK) {
        let mark = block.to_array();
        let Some(route) = route_to_stand_cell(grid, start, block) else {
            unreachable.marks.insert(mark);
            continue;
        };
        if marks.claim(mark, id) {
            pawn.select_slot(TOOL_SLOT);
            return Some(Job::Mine {
                mark,
                route,
                next: 0,
                stall: StallGuard::default(),
                revision: grid.revision(),
            });
        }
    }
    None
}

/// The cell routes from the pawn start in: the one it stands in (see [`standing_cell`]), or
/// its raw feet cell when it stands nowhere, which no route starts from either.
fn start_cell(grid: &VoxelGrid, pose: Pose) -> BlockPos {
    let feet = feet_cell(pose.feet);
    standing_cell(grid, feet).unwrap_or(feet)
}

/// Moves `next` past the waypoints the feet have reached.
fn advance(job: &mut Job, pose: Pose) {
    let Job::Mine {
        route, next, stall, ..
    } = job
    else {
        return;
    };
    let reached = route[*next..]
        .iter()
        .take_while(|&&cell| horizontal_distance(pose.feet, waypoint_centre(cell)) <= ARRIVE_RADIUS)
        .count();
    if reached > 0 {
        *next += reached;
        *stall = StallGuard::default();
    }
}

/// Whether the grid changed since the job's route was last checked. Records the grid's
/// revision as the one checked, so each change reports once.
fn terrain_changed_since_check(job: &mut Job, grid: &VoxelGrid) -> bool {
    let Job::Mine { revision, .. } = job else {
        return false;
    };
    let changed = *revision != grid.revision();
    *revision = grid.revision();
    changed
}

/// Whether the pawn can no longer walk from where it stands along the rest of its route.
fn route_broken(job: &Job, pose: Pose, grid: &VoxelGrid) -> bool {
    let Job::Mine { route, next, .. } = job else {
        return false;
    };
    let start = start_cell(grid, pose);
    let rest = &route[*next..];
    let rest = rest.strip_prefix(&[start]).unwrap_or(rest);
    !(standable(grid, start)
        && rest
            .first()
            .is_none_or(|&first| is_step(grid, start, first))
        && walkable(grid, rest))
}

/// Routes the pawn from its feet cell to a stand cell of the mark it holds, keeping the
/// claim. With no route the claim is released and the pawn goes idle with the mark
/// remembered as unreachable from here. Returns whether the job goes on.
fn replan(job: &mut Job, marks: &mut MineMarks, pose: Pose, grid: &VoxelGrid) -> bool {
    let Job::Mine {
        mark,
        route: planned,
        next,
        stall,
        ..
    } = job
    else {
        return false;
    };
    let held = *mark;
    let block = BlockPos::from_array(held);
    let feet = start_cell(grid, pose);
    if let Some(found) = route_to_stand_cell(grid, feet, block) {
        *planned = found;
        *next = 0;
        *stall = StallGuard::default();
        return true;
    }
    release(job, marks, held);
    let mut unreachable = Unreachable::new(grid.revision(), feet);
    unreachable.marks.insert(held);
    *job = Job::Idle { unreachable };
    false
}

/// The route from `start` to a cell from which `block` is within mining reach.
fn route_to_stand_cell(
    grid: &VoxelGrid,
    start: BlockPos,
    block: BlockPos,
) -> Option<Vec<BlockPos>> {
    route(
        grid,
        start,
        |cell| is_stand_cell(block, cell),
        block,
        ROUTE_NODE_BUDGET,
    )
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
    if let Job::Idle { unreachable } = job {
        match claim_nearest(unreachable, pawn, id, pose, marks, grid) {
            Some(claimed) => *job = claimed,
            None => return JobAction::Wait,
        }
    }
    let Job::Mine { mark, .. } = job else {
        return JobAction::Wait;
    };
    let held = *mark;
    let block = BlockPos::from_array(held);

    if !marks.contains(held) || !grid.is_solid_at(block) || !has_exposed_face(grid, block) {
        release(job, marks, held);
        return JobAction::Wait;
    }
    // A mark cancelled and ordered again between two ticks comes back unclaimed: take it
    // back, or let go of it when another pawn has meanwhile.
    if !marks.claim(held, id) {
        *job = Job::default();
        return JobAction::Wait;
    }

    advance(job, pose);
    if terrain_changed_since_check(job, grid) && route_broken(job, pose, grid) {
        if !replan(job, marks, pose, grid) {
            return JobAction::Wait;
        }
        advance(job, pose);
    }

    let Job::Mine {
        route, next, stall, ..
    } = job
    else {
        return JobAction::Wait;
    };
    let Some(&waypoint) = route.get(*next) else {
        let aim = nearest_exposed_face(grid, block, eye(pose.feet));
        release(job, marks, held);
        return JobAction::Dig { block, aim };
    };
    let to = waypoint_centre(waypoint);

    let distance = horizontal_distance(pose.feet, to);
    if stall
        .closest
        .is_none_or(|closest| distance < closest - PROGRESS_EPSILON)
    {
        stall.closest = Some(distance);
        stall.ticks_without_progress = 0;
    } else {
        stall.ticks_without_progress += 1;
    }
    if stall.ticks_without_progress >= STALL_TICKS {
        release(job, marks, held);
        return JobAction::Wait;
    }
    JobAction::Walk { to }
}

/// Advances every pawn of `crew` one tick, in id order, each at its pose in `poses`, and
/// answers with what each does. The pawns take their marks one after another, so a mark one
/// of them has claimed is already taken when the next looks. A pawn with no entry in `poses`
/// is skipped this tick: it gets no answer and keeps whatever mark it holds, so no other pawn
/// takes it meanwhile.
pub fn step_crew(
    crew: &mut Crew,
    poses: &BTreeMap<PawnId, Pose>,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Vec<(PawnId, JobAction)> {
    let mut actions = Vec::new();
    let mut being_dug = Vec::new();
    for (id, pawn) in crew.iter_mut() {
        let Some(&pose) = poses.get(&id) else {
            continue;
        };
        let mut job = std::mem::take(&mut pawn.job);
        let action = step(&mut job, pawn, id, pose, marks, grid);
        pawn.job = job;
        if let JobAction::Dig { block, .. } = action {
            // A dig lets go of its mark, but the block stands until the caller applies the
            // dig: keep the mark taken so no later pawn claims it this tick.
            let mark = block.to_array();
            let kept = marks.claim(mark, id);
            debug_assert!(kept, "the digger's mark was just released");
            being_dug.push(mark);
        }
        actions.push((id, action));
    }
    for mark in being_dug {
        marks.release(mark);
    }
    actions
}

fn waypoint_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

/// Turn and speed that bring a pawn at `pose` toward `to`.
///
/// Height is ignored: the pawn walks the ground, so only the bearing matters.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let bearing = (to.x - pose.feet.x).atan2(to.z - pose.feet.z);
    let off = (bearing - pose.yaw + PI).rem_euclid(TAU) - PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: off.clamp(-per_tick, per_tick),
        forward: if off.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawn::MINE_REACH;
    use crate::routing::{feet_cell, is_step, standable};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const STONE: u32 = 1;
    const ME: PawnId = PawnId(0);
    const OTHER: PawnId = PawnId(1);
    const EPS: f32 = 1e-4;

    fn cell(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

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

        /// Removes the block at `c` the way a dig does: the grid change, then the marker.
        fn dig(&mut self, c: BlockPos) {
            let before = self.grid.revision();
            let removed = self.grid.mutator().remove(c);
            self.marks.block_removed(c.to_array());
            assert!(removed, "{c} holds no block to dig");
            assert_ne!(self.grid.revision(), before, "a dig moves the revision");
        }

        fn memo(&self) -> &Unreachable {
            match &self.job {
                Job::Idle { unreachable } => unreachable,
                other @ Job::Mine { .. } => panic!("the pawn is not idle: {other:?}"),
            }
        }

        /// Cuts the whole ground column at (x, z) so no pawn can stand there or cross it.
        fn cut_ground(&mut self, x: i32, z: i32) {
            self.dig(cell(x, 0, z));
        }

        /// Moves the pawn onto each waypoint it is sent to until its feet are in `at`;
        /// at most `limit` ticks.
        fn walk_until_at(&mut self, at: BlockPos, limit: usize) {
            for _ in 0..limit {
                if feet_cell(self.pose.feet) == at {
                    return;
                }
                if let JobAction::Walk { to } = self.tick() {
                    self.pose.feet = to;
                }
            }
            panic!("never reached {at}: {:?}", self.job);
        }

        /// The flat field with a trench across it at x 3 to 6, crossed only by one-cell-wide
        /// causeways along x at each of `bridge_zs`.
        fn trench_with_causeways(bridge_zs: &[i32]) -> Self {
            let mut site = Self::flat();
            for x in 3..=6 {
                for z in -12..=12 {
                    site.grid.mutator().remove(cell(x, 0, z));
                }
            }
            for &z in bridge_zs {
                for x in 3..=6 {
                    place(&mut site.grid, cell(x, 0, z));
                }
            }
            site
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
            assert!(matches!(site.job, Job::Idle { .. }));
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
            assert!(matches!(site.job, Job::Idle { .. }));
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
            // Only the other pawn's mark: wait. Beside a free mark: take that one.
            let mut only_claimed = Site::flat();
            only_claimed.mark_block(cell(4, 1, 0));
            assert!(only_claimed.marks.claim([4, 1, 0], OTHER));
            let mut with_a_free_one = Site::flat();
            with_a_free_one.mark_block(cell(4, 1, 0));
            with_a_free_one.mark_block(cell(-8, 1, 0));
            assert!(with_a_free_one.marks.claim([4, 1, 0], OTHER));

            let waiting = only_claimed.tick();
            with_a_free_one.tick();

            assert_eq!(waiting, JobAction::Wait);
            assert!(matches!(only_claimed.job, Job::Idle { .. }));
            assert_eq!(only_claimed.claimed_by([4, 1, 0]), Some(OTHER));
            assert_eq!(only_claimed.marks.unclaimed().count(), 0);
            assert_eq!(with_a_free_one.claimed_by([-8, 1, 0]), Some(ME));
            assert_eq!(with_a_free_one.claimed_by([4, 1, 0]), Some(OTHER));
        }
    }

    mod claiming_order {
        use super::*;

        #[test]
        fn the_nearest_mark_is_measured_from_where_the_pawn_stands() {
            let mut site = Site::flat();
            site.pose.feet = centre(cell(10, 1, 0));
            site.mark_block(cell(7, 1, 0));
            site.mark_block(cell(-3, 1, 0));

            site.tick();

            assert_eq!(site.claimed_by([7, 1, 0]), Some(ME));
            assert_eq!(site.claimed_by([-3, 1, 0]), None);
        }

        #[test]
        fn a_new_mark_is_routed_while_failed_ones_stay_memoised() {
            let mut site = Site::flat();
            // Two floating blocks no ground lets the pawn stand beside.
            site.mark_block(cell(1, 5, 0));
            site.mark_block(cell(2, 5, 0));

            let first = site.tick();
            let revision = site.grid.revision();
            // A new mark on ground that is already there: the terrain, and so the memo,
            // stays as it was.
            site.marks.toggle([4, 0, 0]);
            let second = site.tick();

            assert_eq!(first, JobAction::Wait);
            assert_eq!(site.grid.revision(), revision);
            assert_ne!(second, JobAction::Wait);
            assert_eq!(site.claimed_by([4, 0, 0]), Some(ME));
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
            assert!(matches!(site.job, Job::Idle { .. }));
            assert!(
                site.is_marked([4, 1, 0]),
                "removal, not the dig, drops marks"
            );
            assert_eq!(site.claimed_by([4, 1, 0]), None);
        }

        #[test]
        fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
            // Beside the block: its west face is nearest.
            let mut beside = Site::flat();
            beside.mark_block(cell(4, 1, 0));
            beside.pose.feet = centre(cell(3, 1, 0));
            // Beside it on a step a block higher: the top is nearest.
            let mut on_top = Site::flat();
            place(&mut on_top.grid, cell(3, 1, 0));
            on_top.mark_block(cell(4, 1, 0));
            on_top.pose.feet = centre(cell(3, 2, 0));
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
                let sent = site.tick();
                assert!(
                    matches!(sent, JobAction::Walk { to } if feet_cell(to) == cell(1, 1, 0)),
                    "the claim tick skips the start cell and walks to the first one not reached: {sent:?}"
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

    mod digging_position {
        use super::*;

        #[test]
        fn the_dig_aims_from_the_eye_above_the_feet() {
            // Feet half a block up: the eye is closer to the top face than the west face only
            // at the full eye height.
            let mut site = Site::flat();
            site.mark_block(cell(4, 1, 0));
            site.pose.feet = centre(cell(3, 1, 0)) + Vec3::Y * 0.5;

            let run = site.run_to_the_dig(10);

            let (_, aim, _) = run.dig.expect("the pawn digs from where it stands");
            assert_near(aim, Vec3::new(4.5, 2.0, 0.5));
        }

        #[test]
        fn a_pawn_standing_on_its_marked_block_steps_aside_before_digging() {
            let mut site = Site::flat();
            site.mark_block(cell(0, 0, 0));
            let start = feet_cell(site.pose.feet);

            let run = site.run_to_the_dig(30);

            let (block, _, feet) = run.dig.expect("the pawn digs once it has stepped off");
            assert_eq!(start, cell(0, 1, 0), "the pawn begins on the marked block");
            assert_eq!(block, cell(0, 0, 0));
            assert!(!run.targets.is_empty(), "it walked first");
            assert_ne!(
                feet_cell(feet) - BlockPos::Y,
                block,
                "dug the block under its own feet"
            );
            assert!(is_stand_cell(block, feet_cell(feet)));
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
            assert!(matches!(site.job, Job::Idle { .. }));
            assert!(!site.is_marked([9, 1, 0]));
        }

        #[test]
        fn a_mark_buried_after_the_claim_releases_the_pawn() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();
            assert!(matches!(site.job, Job::Mine { .. }), "the walk began");

            for d in NEIGHBOURS {
                place(&mut site.grid, cell(9, 1, 0) + d);
            }
            let action = site.tick();

            assert_eq!(action, JobAction::Wait);
            assert!(matches!(site.job, Job::Idle { .. }));
            assert!(site.is_marked([9, 1, 0]), "the mark stays");
            assert_eq!(site.claimed_by([9, 1, 0]), None);
            assert!(site.grid.is_solid_at(cell(9, 1, 0)), "nothing was dug");
        }

        #[test]
        fn moving_closer_counts_as_progress_and_resets_the_stall_count() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));
            site.tick();

            site.pose.feet.x = 1.0;
            site.tick();
            site.pose.feet.x = 1.1;
            site.tick();

            let Job::Mine { stall, .. } = &site.job else {
                panic!("still walking: {:?}", site.job);
            };
            assert!(
                stall
                    .closest
                    .is_some_and(|closest| (closest - 0.4).abs() < EPS),
                "{stall:?}"
            );
            assert_eq!(stall.ticks_without_progress, 0);
        }

        #[test]
        fn a_step_closer_by_exactly_the_progress_epsilon_is_not_progress() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));
            site.tick();
            // The first waypoint is the centre of cell 1, one metre from the feet.
            let closer = 1.0 - PROGRESS_EPSILON;
            site.pose.feet.x = 1.5 - closer;

            site.tick();

            let Job::Mine { stall, .. } = &site.job else {
                panic!("still walking: {:?}", site.job);
            };
            assert_eq!(stall.ticks_without_progress, 1);
            assert_eq!(stall.closest, Some(1.0));
        }

        #[test]
        fn a_mark_dug_by_another_pawn_releases_the_claim() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();

            site.grid.mutator().remove(cell(9, 1, 0));
            site.marks.block_removed([9, 1, 0]);
            let action = site.tick();
            site.marks.toggle([9, 1, 0]);

            assert_eq!(action, JobAction::Wait);
            assert!(matches!(site.job, Job::Idle { .. }));
            assert_eq!(
                site.claimed_by([9, 1, 0]),
                None,
                "no claim outlives the block"
            );
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
                if matches!(site.job, Job::Idle { .. }) {
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
            // A corridor one cell wide: with another way round the job re-plans instead.
            for x in -12..=12 {
                for z in (-12..=12).filter(|&z| z != 0) {
                    site.grid.mutator().remove(cell(x, 0, z));
                }
            }
            site.mark_block(cell(8, 1, 0));
            site.tick();
            assert!(matches!(site.job, Job::Mine { .. }), "the walk began");

            place(&mut site.grid, cell(1, 2, 0));
            let mut released = false;
            for _ in 0..3 {
                site.tick();
                if matches!(site.job, Job::Idle { .. }) {
                    released = true;
                    break;
                }
            }

            assert!(released, "a blocked waypoint ends the job");
            assert!(site.is_marked([8, 1, 0]), "the mark stays");
            assert_eq!(site.claimed_by([8, 1, 0]), None);
        }
    }

    mod waiting {
        use super::*;

        /// A marked block floating where no ground lets the pawn stand beside it.
        fn float(site: &mut Site, x: i32, z: i32) {
            site.mark_block(cell(x, 5, z));
        }

        /// Eight floating marks; the four nearest the pawn at (0, 1, 0) are (1, 0), (2, 0),
        /// (3, 0) and (1, 3) in x and z.
        fn site_with_eight_unreachable_marks() -> Site {
            let mut site = Site::flat();
            for x in 1..=5 {
                float(&mut site, x, 0);
            }
            for x in 1..=3 {
                float(&mut site, x, 3);
            }
            site
        }

        #[test]
        fn a_mark_in_a_sealed_pocket_is_never_claimed() {
            let mut site = Site::flat();
            site.grid
                .mutator()
                .fill_region(cell(2, 1, -3), cell(8, 5, 3), STONE);
            for x in 4..=6 {
                for y in 2..=4 {
                    for z in -1..=1 {
                        site.grid.mutator().remove(cell(x, y, z));
                    }
                }
            }
            site.marks.toggle([5, 1, 0]);
            assert!(
                standable(&site.grid, cell(4, 2, 0)),
                "the pocket has a stand cell, only no way in"
            );

            for tick in 0..300 {
                let action = site.tick();

                assert_eq!(action, JobAction::Wait, "tick {tick}");
            }

            assert!(matches!(site.job, Job::Idle { .. }));
            assert_eq!(site.claimed_by([5, 1, 0]), None);
            assert!(site.is_marked([5, 1, 0]), "the mark stays");
            assert!(site.memo().marks.contains(&[5, 1, 0]), "remembered");

            // A way in opens: a tunnel with headroom through the wall, into the pocket.
            for x in 2..=3 {
                for y in 1..=4 {
                    site.dig(cell(x, y, 0));
                }
            }
            let after_the_way_opens = site.tick();

            assert_ne!(after_the_way_opens, JobAction::Wait);
            assert_eq!(site.claimed_by([5, 1, 0]), Some(ME), "workable at once");
        }

        #[test]
        fn digging_the_upper_mark_opens_the_lower_one() {
            let mut site = Site::flat();
            site.grid
                .mutator()
                .fill_region(cell(-12, -2, -12), cell(12, 0, 12), STONE);
            site.marks.toggle([4, 0, 0]);
            site.marks.toggle([4, -1, 0]);

            let first = site.run_to_the_dig(30);
            let (upper, _, _) = first.dig.expect("the exposed upper block is dug first");
            let lower_claimed_early = site.claimed_by([4, -1, 0]);
            site.dig(upper);
            let second = site.run_to_the_dig(30);
            let (lower, _, feet) = second.dig.expect("the lower block is dug once exposed");

            assert_eq!(upper, cell(4, 0, 0));
            assert_eq!(lower_claimed_early, None, "buried until the upper is gone");
            assert_eq!(lower, cell(4, -1, 0));
            assert!(is_stand_cell(lower, feet_cell(feet)));
            assert_ne!(
                feet_cell(feet),
                cell(4, 0, 0),
                "not from on top of the lower block"
            );
            assert!(
                site.is_marked([4, -1, 0]),
                "the dig, not the job, drops the mark"
            );
        }

        #[test]
        fn unreachable_marks_are_not_searched_again_until_the_terrain_changes() {
            let mut site = site_with_eight_unreachable_marks();
            let start = feet_cell(site.pose.feet);

            site.tick();
            let after_one = site.memo().clone();
            site.tick();
            let after_two = site.memo().clone();
            let third = site.tick();
            let after_three = site.memo().clone();
            place(&mut site.grid, cell(-11, 1, -11));
            site.tick();
            let after_the_edit = site.memo().clone();

            let nearest_four: BTreeSet<[i32; 3]> =
                [[1, 5, 0], [2, 5, 0], [3, 5, 0], [1, 5, 3]].into();
            assert_eq!(after_one.marks, nearest_four, "four searches in a tick");
            assert_eq!(after_one.from, start);
            assert_eq!(after_two.marks.len(), 8, "the next four, not the same four");
            assert!(after_two.marks.is_superset(&after_one.marks));
            assert_eq!(third, JobAction::Wait);
            assert_eq!(
                after_three, after_two,
                "nothing left to search, nothing new"
            );
            assert_eq!(
                after_the_edit.marks, nearest_four,
                "an edit anywhere empties the memo and the search starts over"
            );
            assert_eq!(after_the_edit.revision, site.grid.revision());
        }

        #[test]
        fn a_pawn_behind_five_unreachable_marks_reaches_the_sixth() {
            let mut site = Site::flat();
            for x in 1..=5 {
                float(&mut site, x, 0);
            }
            site.mark_block(cell(11, 1, 0));

            let first = site.tick();
            let claimed_after_first = site.claimed_by([11, 1, 0]);
            let memo_after_first = site.memo().marks.clone();
            let second = site.tick();

            assert_eq!(first, JobAction::Wait, "four failed routes use the tick up");
            assert_eq!(claimed_after_first, None);
            assert_eq!(
                memo_after_first,
                BTreeSet::from([[1, 5, 0], [2, 5, 0], [3, 5, 0], [4, 5, 0]])
            );
            assert!(
                matches!(
                    site.job,
                    Job::Mine {
                        mark: [11, 1, 0],
                        ..
                    }
                ),
                "{second:?}"
            );
            assert_eq!(site.claimed_by([11, 1, 0]), Some(ME));
            for x in 1..=5 {
                assert_eq!(site.claimed_by([x, 5, 0]), None);
                assert!(site.is_marked([x, 5, 0]));
            }
        }

        #[test]
        fn the_memo_empties_when_the_pawn_moves() {
            let mut site = site_with_eight_unreachable_marks();
            site.tick();
            site.tick();
            assert_eq!(site.memo().marks.len(), 8, "all remembered where it stood");

            site.pose.feet += Vec3::new(0.2, 0.0, -0.2);
            site.tick();
            assert_eq!(site.memo().marks.len(), 8, "the same cell keeps the memo");
            assert_eq!(site.memo().from, cell(0, 1, 0));

            site.pose.feet = centre(cell(5, 1, 5));
            site.tick();

            assert_eq!(site.memo().from, cell(5, 1, 5));
            assert_eq!(
                site.memo().marks.len(),
                4,
                "searched afresh from the new cell"
            );
        }
    }

    mod replanning {
        use super::*;

        const MARK: [i32; 3] = [9, 1, 0];

        #[test]
        fn a_hole_cut_in_the_only_route_stops_the_pawn_before_it() {
            let mut site = Site::trench_with_causeways(&[0]);
            site.mark_block(cell(9, 1, 0));
            site.walk_until_at(cell(2, 1, 0), 10);
            assert!(matches!(site.job, Job::Mine { .. }), "the walk is on");

            site.cut_ground(4, 0);
            let first = site.tick();

            assert_eq!(first, JobAction::Wait, "stops at once");
            assert!(matches!(site.job, Job::Idle { .. }), "{:?}", site.job);
            assert_eq!(site.claimed_by(MARK), None, "released at once");
            assert_eq!(site.memo().marks, BTreeSet::from([MARK]));
            assert_eq!(site.memo().from, cell(2, 1, 0));
            assert_eq!(site.memo().revision, site.grid.revision());

            let mut actions = vec![first];
            actions.extend((0..19).map(|_| site.tick()));
            assert!(
                actions.iter().all(|action| *action == JobAction::Wait),
                "{actions:?}"
            );
            assert!(matches!(site.job, Job::Idle { .. }), "{:?}", site.job);
            assert_eq!(site.claimed_by(MARK), None, "the claim is released");
            assert!(site.is_marked(MARK), "the mark stays");
            assert_eq!(feet_cell(site.pose.feet), cell(2, 1, 0));
            assert!(
                site.memo().marks.contains(&MARK),
                "and is not searched again"
            );
            assert_eq!(site.memo().from, cell(2, 1, 0));
        }

        #[test]
        fn a_cut_route_is_replaced_by_the_other_way_to_the_same_mark() {
            let mut site = Site::trench_with_causeways(&[0, 6]);
            site.mark_block(cell(9, 1, 2));
            site.walk_until_at(cell(2, 1, 0), 10);
            let Job::Mine { route, .. } = &site.job else {
                panic!("the walk is on: {:?}", site.job);
            };
            assert!(
                route.contains(&cell(4, 1, 0)),
                "the near causeway is chosen"
            );

            // A nearer mark on ground that stays put: only a release and re-claim takes it.
            site.marks.toggle([2, 0, -2]);
            site.cut_ground(4, 0);
            let mut targets = Vec::new();
            let mut dug = None;
            for tick in 0..120 {
                assert_eq!(site.claimed_by([2, 0, -2]), None, "tick {tick}");
                match site.tick() {
                    JobAction::Walk { to } => {
                        assert_eq!(site.claimed_by([9, 1, 2]), Some(ME), "tick {tick}");
                        targets.push(to);
                        site.pose.feet = to;
                    }
                    JobAction::Dig { block, .. } => {
                        dug = Some(block);
                        break;
                    }
                    JobAction::Wait => panic!("waited at tick {tick}: {:?}", site.job),
                }
            }

            assert_eq!(dug, Some(cell(9, 1, 2)), "the same mark is dug");
            assert_eq!(site.claimed_by([2, 0, -2]), None, "the nearer mark waits");
            assert!(site.is_marked([2, 0, -2]));
            let on_the_trench: Vec<BlockPos> = targets
                .iter()
                .map(|&to| feet_cell(to))
                .filter(|c| (3..=6).contains(&c.x))
                .collect();
            assert!(!on_the_trench.is_empty(), "it crossed");
            assert!(
                on_the_trench.iter().all(|c| c.z == 6),
                "crossed by the far causeway: {on_the_trench:?}"
            );
        }

        #[test]
        fn an_unchanged_grid_never_rechecks_the_route() {
            let mut site = Site::flat();
            site.mark_block(cell(9, 1, 0));
            site.tick();
            let Job::Mine {
                revision: claimed_at,
                route: planned,
                ..
            } = site.job.clone()
            else {
                panic!("the walk began: {:?}", site.job);
            };

            for _ in 0..3 {
                if let JobAction::Walk { to } = site.tick() {
                    site.pose.feet = to;
                }
            }
            let before_the_edit = site.grid.revision();
            let Job::Mine {
                next: next_before, ..
            } = &site.job
            else {
                panic!("still walking: {:?}", site.job);
            };
            let next_before = *next_before;
            site.dig(cell(-11, 0, 11));
            site.tick();

            let Job::Mine {
                revision,
                route,
                next,
                ..
            } = &site.job
            else {
                panic!("an edit far away does not end the walk: {:?}", site.job);
            };
            assert_eq!(claimed_at, before_the_edit, "the claim checked the grid");
            assert_ne!(*revision, claimed_at);
            assert_eq!(*revision, site.grid.revision(), "checked after the edit");
            assert_eq!(*route, planned, "a route still walkable is kept");
            assert!(next_before >= 3, "the pawn had walked on: {next_before}");
            assert_eq!(*next, next_before + 1, "the walk went on, not back to 0");
        }

        /// A site with a one-block rise across x 3, the pawn walking to a mark beyond it and
        /// standing with its feet inside the rise column.
        fn mid_climb() -> Site {
            let mut site = Site::flat();
            for z in -12..=12 {
                place(&mut site.grid, cell(3, 1, z));
            }
            site.mark_block(cell(9, 1, 0));
            site.walk_until_at(cell(2, 1, 0), 10);
            site.pose.feet = Vec3::new(3.5, 1.4, 0.5);
            site
        }

        /// Places or clears a block far from every route, one revision per call.
        fn churn(site: &mut Site, tick: usize) {
            let far = cell(-11, 1, 11);
            if tick.is_multiple_of(2) {
                site.dig(far);
            } else {
                place(&mut site.grid, far);
            }
        }

        #[test]
        fn feet_inside_a_rise_keep_the_route_when_the_terrain_far_away_changes() {
            let mut site = mid_climb();
            assert!(
                site.grid.is_solid_at(feet_cell(site.pose.feet)),
                "the feet are inside the rise"
            );
            let before = site.tick();
            let Job::Mine {
                route: planned,
                next: planned_next,
                ..
            } = site.job.clone()
            else {
                panic!("walking: {:?}", site.job);
            };

            site.dig(cell(-11, 0, 11));
            let after = site.tick();

            let Job::Mine { route, next, .. } = &site.job else {
                panic!("an unrelated edit ends the job: {:?}", site.job);
            };
            assert!(matches!(before, JobAction::Walk { .. }), "{before:?}");
            assert_eq!(after, before, "the same waypoint");
            assert_eq!(*route, planned, "no re-plan");
            assert_eq!(*next, planned_next);
            assert_eq!(site.claimed_by([9, 1, 0]), Some(ME));
        }

        #[test]
        fn a_stalled_pawn_still_gives_up_while_the_terrain_keeps_changing() {
            let stand_still = |mut site: Site| {
                site.tick();
                for ticks in 1..=STALL_TICKS + 2 {
                    churn(&mut site, ticks as usize);
                    site.tick();
                    if matches!(site.job, Job::Idle { .. }) {
                        return (Some(ticks), site);
                    }
                }
                (None, site)
            };
            let mut level = Site::flat();
            level.mark_block(cell(9, 1, 0));

            let (on_the_level, level) = stand_still(level);
            let (mid_climb, climbing) = stand_still(mid_climb());

            for (name, ticks, site) in [
                ("level", on_the_level, level),
                ("mid-climb", mid_climb, climbing),
            ] {
                let ticks = ticks.unwrap_or_else(|| panic!("{name}: never gave up"));
                assert!(
                    (STALL_TICKS - 1..=STALL_TICKS + 2).contains(&ticks),
                    "{name}: gave up after {ticks} ticks"
                );
                assert!(site.is_marked([9, 1, 0]), "{name}: the mark stays");
                assert_eq!(site.claimed_by([9, 1, 0]), None, "{name}");
            }
        }

        #[test]
        fn a_block_placed_over_a_waypoint_sends_the_pawn_round_it_to_the_same_mark() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));
            site.walk_until_at(cell(1, 1, 0), 10);

            place(&mut site.grid, cell(3, 2, 0));
            let mut dug = None;
            for tick in 0..100 {
                match site.tick() {
                    JobAction::Walk { to } => {
                        assert_eq!(site.claimed_by([8, 1, 0]), Some(ME), "tick {tick}");
                        assert_ne!(feet_cell(to), cell(3, 1, 0), "walked into the block");
                        site.pose.feet = to;
                    }
                    JobAction::Dig { block, .. } => {
                        dug = Some(block);
                        break;
                    }
                    JobAction::Wait => panic!("waited at tick {tick}: {:?}", site.job),
                }
            }

            assert_eq!(dug, Some(cell(8, 1, 0)));
        }

        #[test]
        fn a_walk_over_a_raised_stretch_survives_terrain_changing_every_tick() {
            let mut site = Site::flat();
            for x in 3..=5 {
                for z in -12..=12 {
                    place(&mut site.grid, cell(x, 1, z));
                }
            }
            site.mark_block(cell(9, 1, 0));
            let pace = 4.0 / crate::TICK_HZ as f32;
            let mut planned = None;
            let mut dug = None;

            for tick in 0..STALL_TICKS * 4 {
                if planned.is_some() {
                    churn(&mut site, tick as usize);
                }
                let action = site.tick();
                if planned.is_none() {
                    let Job::Mine { route, .. } = &site.job else {
                        panic!("the claim tick: {:?}", site.job);
                    };
                    planned = Some(route.clone());
                }
                match action {
                    JobAction::Walk { to } => {
                        assert_eq!(site.claimed_by([9, 1, 0]), Some(ME), "tick {tick}");
                        let Job::Mine { route, .. } = &site.job else {
                            panic!("tick {tick}: {:?}", site.job);
                        };
                        assert_eq!(Some(route), planned.as_ref(), "re-planned on tick {tick}");
                        let mut toward = to - site.pose.feet;
                        toward.y = 0.0;
                        site.pose.feet += toward.clamp_length_max(pace);
                        let climb = (to.y - site.pose.feet.y).clamp(-0.1, 0.1);
                        site.pose.feet.y += climb;
                    }
                    JobAction::Dig { block, .. } => {
                        dug = Some(block);
                        break;
                    }
                    JobAction::Wait => panic!("lapsed on tick {tick}: {:?}", site.job),
                }
            }

            assert_eq!(dug, Some(cell(9, 1, 0)));
        }

        #[test]
        fn the_idle_memo_holds_while_the_feet_stay_over_the_same_standing_cell() {
            let mut site = Site::flat();
            for z in -12..=12 {
                place(&mut site.grid, cell(3, 1, z));
            }
            for x in 1..=5 {
                site.mark_block(cell(x, 5, 6));
            }
            for x in 1..=3 {
                site.mark_block(cell(x, 5, 8));
            }
            site.pose.feet = Vec3::new(3.5, 1.4, 0.5);
            assert!(site.grid.is_solid_at(feet_cell(site.pose.feet)));

            site.tick();
            let first = site.memo().clone();
            site.pose.feet.x += 0.1;
            site.tick();
            let same_raw_cell = site.memo().clone();
            site.pose.feet.y = 2.2;
            site.tick();
            let other_raw_cell = site.memo().clone();

            assert_eq!(first.marks.len(), 4);
            assert_eq!(same_raw_cell.marks.len(), 8, "no search again");
            assert_eq!(same_raw_cell.from, first.from);
            assert_eq!(other_raw_cell.marks.len(), 8, "the same standing cell");
            assert_eq!(other_raw_cell.from, first.from);
        }

        #[test]
        fn a_failed_replan_mid_climb_remembers_the_standing_cell() {
            let mut site = Site::flat();
            for x in -12..=12 {
                for z in (-12..=12).filter(|&z| z != 0) {
                    site.grid.mutator().remove(cell(x, 0, z));
                }
            }
            place(&mut site.grid, cell(3, 1, 0));
            site.mark_block(cell(9, 1, 0));
            site.walk_until_at(cell(2, 1, 0), 10);
            site.pose.feet = Vec3::new(3.5, 1.4, 0.5);
            assert!(site.grid.is_solid_at(feet_cell(site.pose.feet)));
            site.tick();

            site.dig(cell(6, 0, 0));
            let cut = site.tick();
            let after_the_cut = site.memo().clone();
            let again = site.tick();
            let next_tick = site.memo().clone();

            assert_eq!(cut, JobAction::Wait);
            assert_eq!(again, JobAction::Wait);
            assert_eq!(after_the_cut.marks, BTreeSet::from([[9, 1, 0]]));
            assert_eq!(after_the_cut.from, cell(3, 2, 0), "where the pawn stands");
            assert_eq!(next_tick, after_the_cut, "no search again");
        }

        #[test]
        fn a_far_edit_short_of_the_stand_cell_does_not_end_the_walk() {
            let mut site = Site::flat();
            site.mark_block(cell(8, 1, 0));
            site.walk_until_at(cell(6, 1, 0), 20);
            site.tick();
            // In the stand cell but not yet within the arrival radius of its centre.
            site.pose.feet = Vec3::new(7.0, 1.0, 0.5);
            let before = site.tick();
            let planned = site.job.clone();

            site.dig(cell(-11, 0, 11));
            let after = site.tick();
            let Job::Mine { mark, route, .. } = &site.job else {
                panic!("the walk ended: {:?}", site.job);
            };
            let (mark, route) = (*mark, route.clone());
            site.pose.feet = centre(cell(7, 1, 0));
            let last = site.tick();

            assert!(
                matches!(before, JobAction::Walk { to } if feet_cell(to) == cell(7, 1, 0)),
                "{before:?}"
            );
            assert_eq!(after, before, "still on its way to the stand cell");
            assert_eq!(mark, [8, 1, 0]);
            assert!(matches!(&planned, Job::Mine { route: r, .. } if *r == route));
            assert!(
                matches!(last, JobAction::Dig { block, .. } if block == cell(8, 1, 0)),
                "{last:?}"
            );
        }

        proptest! {
            #[test]
            fn every_walk_target_is_a_step_from_the_feet_cell(
                cuts in proptest::collection::vec((1..=9i32, -3..=3i32), 1..8),
            ) {
                let mut site = Site::flat();
                site.mark_block(cell(11, 1, 0));

                for (x, z) in cuts {
                    let under_feet = feet_cell(site.pose.feet) - BlockPos::Y;
                    let ground = cell(x, 0, z);
                    if ground != under_feet && site.grid.is_solid_at(ground) {
                        site.dig(ground);
                    }
                    for _ in 0..3 {
                        let from = feet_cell(site.pose.feet);
                        let JobAction::Walk { to } = site.tick() else {
                            break;
                        };
                        prop_assert!(
                            is_step(&site.grid, from, feet_cell(to)),
                            "{from} to {to}"
                        );
                        site.pose.feet = to;
                    }
                }
            }
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

        #[test]
        fn the_cell_on_top_of_the_block_is_not_a_stand_cell() {
            let block = cell(10, 20, -5);

            let on_top = is_stand_cell(block, block + BlockPos::Y);
            let beside_on_top = is_stand_cell(block, block + cell(1, 1, 0));

            assert!(!on_top, "the ground there is the block itself");
            assert!(beside_on_top, "the block is not the ground there");
        }

        proptest! {
            #[test]
            fn every_stand_cell_is_within_the_mining_reach(
                block in (-1000..1000i32, -64..64i32, -1000..1000i32),
            ) {
                let block = cell(block.0, block.1, block.2);
                let lo = block.as_vec3();
                let hi = lo + Vec3::ONE;
                let mut stand_cells = 0;

                for dx in -3..=3 {
                    for dy in -4..=5 {
                        for dz in -3..=3 {
                            let feet = block + cell(dx, dy, dz);
                            if !is_stand_cell(block, feet) {
                                continue;
                            }
                            stand_cells += 1;
                            let eye = centre(feet) + Vec3::Y * EYE_HEIGHT;
                            let nearest = eye.clamp(lo, hi).distance(eye);
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
                                nearest <= MINE_REACH && farthest <= MINE_REACH,
                                "from {feet} {block} is {nearest} to {farthest} m away"
                            );
                        }
                    }
                }

                prop_assert!(stand_cells > 0, "no stand cell around {block}");
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
            let lift = Vec3::Y * 4.0;
            // 0.9 rad is inside the walking cone on the ground, but about 61 degrees
            // from the pawn's heading once the 4 m of height is counted.
            let level = toward_bearing(pose, 0.9);
            let small = toward_bearing(pose, 0.05);

            let high = steer(pose, level + lift);
            let high_small = steer(pose, small + lift);

            assert_eq!(high, steer(pose, level));
            assert_eq!(high.forward, 1.0);
            assert!((high_small.yaw_delta - 0.05).abs() < EPS, "{high_small:?}");
        }

        #[test]
        fn steer_walks_only_when_facing_the_target() {
            let pose = facing(0.0);
            let bearing = |b: f32| steer(pose, toward_bearing(pose, b));

            let dead_ahead = bearing(0.0);
            let slightly_off = bearing(0.4);
            let off_the_other_way = bearing(-0.4);
            let inside_the_cone = bearing(0.9);
            let outside_the_cone = bearing(1.2);
            let side_on = bearing(1.6);
            let behind = bearing(std::f32::consts::PI);

            assert_eq!(dead_ahead.forward, 1.0, "full speed");
            assert_eq!(dead_ahead.yaw_delta, 0.0);
            assert_eq!(slightly_off.forward, 1.0);
            assert_eq!(off_the_other_way.forward, 1.0);
            assert_eq!(inside_the_cone.forward, 1.0, "52 degrees still walks");
            assert_eq!(outside_the_cone.forward, 0.0, "69 degrees turns in place");
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
                use std::f32::consts::{FRAC_PI_3, PI, TAU};
                let pose = facing(yaw);
                let off = (bearing - yaw + PI).rem_euclid(TAU) - PI;

                let step = steer(pose, toward_bearing(pose, bearing));

                prop_assert!(step.yaw_delta.abs() <= PER_TICK + EPS);
                if off.abs() > PER_TICK + EPS && off.abs() < PI - 0.01 {
                    prop_assert!(
                        (step.yaw_delta - off.signum() * PER_TICK).abs() < EPS,
                        "off {off}, turned {}", step.yaw_delta
                    );
                }
                if off.abs() <= PER_TICK {
                    prop_assert!((step.yaw_delta - off).abs() < EPS, "off {off}, turned {}", step.yaw_delta);
                }
                if off.abs() < FRAC_PI_3 - 0.02 {
                    prop_assert_eq!(step.forward, 1.0);
                }
                if off.abs() > FRAC_PI_3 + 0.02 {
                    prop_assert_eq!(step.forward, 0.0);
                }
            }
        }
    }

    mod crew {
        use super::*;

        /// Several pawns sharing one flat field and one set of marks, driven a tick at a time.
        struct CrewSite {
            grid: VoxelGrid,
            marks: MineMarks,
            crew: Crew,
            poses: BTreeMap<PawnId, Pose>,
            ids: Vec<PawnId>,
        }

        impl CrewSite {
            /// The flat field with one idle pawn standing in each of `starts`.
            fn flat(starts: &[BlockPos]) -> Self {
                let Site { grid, marks, .. } = Site::flat();
                let mut crew = Crew::default();
                let mut poses = BTreeMap::new();
                let mut ids = Vec::new();
                for &start in starts {
                    let id = crew.spawn(Pawn::default());
                    poses.insert(
                        id,
                        Pose {
                            feet: centre(start),
                            yaw: 0.0,
                        },
                    );
                    ids.push(id);
                }
                Self {
                    grid,
                    marks,
                    crew,
                    poses,
                    ids,
                }
            }

            fn mark_block(&mut self, c: BlockPos) {
                place(&mut self.grid, c);
                self.marks.toggle(c.to_array());
            }

            /// One crew tick. Each pawn that is sent walking stands on its target next, and
            /// each block dug is removed the way the colony does it.
            fn tick(&mut self) -> Vec<(PawnId, JobAction)> {
                let actions = step_crew(&mut self.crew, &self.poses, &mut self.marks, &self.grid);
                for &(id, action) in &actions {
                    match action {
                        JobAction::Wait => {}
                        JobAction::Walk { to } => {
                            self.poses
                                .get_mut(&id)
                                .expect("only pawns with a pose are stepped")
                                .feet = to;
                        }
                        JobAction::Dig { block, .. } => {
                            assert!(self.grid.mutator().remove(block), "{block} holds no block");
                            self.marks.block_removed(block.to_array());
                        }
                    }
                }
                actions
            }

            fn claimed_by(&self, mark: [i32; 3]) -> Option<PawnId> {
                self.marks
                    .iter()
                    .find(|(pos, _)| *pos == mark)
                    .and_then(|(_, m)| m.claimed_by)
            }

            /// The mark each pawn holds, by pawn.
            fn held(&self) -> Vec<(PawnId, [i32; 3])> {
                self.crew
                    .iter()
                    .filter_map(|(id, pawn)| match &pawn.job {
                        Job::Mine { mark, .. } => Some((id, *mark)),
                        Job::Idle { .. } => None,
                    })
                    .collect()
            }
        }

        fn walk_target(actions: &[(PawnId, JobAction)], id: PawnId) -> Option<Vec3> {
            actions.iter().find_map(|&(who, action)| match action {
                JobAction::Walk { to } if who == id => Some(to),
                _ => None,
            })
        }

        #[test]
        fn two_idle_pawns_and_one_mark_one_claims_and_the_other_waits() {
            // The second pawn is nearer the block, but the first takes its turn first.
            let mut site = CrewSite::flat(&[cell(0, 1, 0), cell(3, 1, 0)]);
            site.mark_block(cell(6, 1, 0));
            let [first, second] = site.ids[..] else {
                panic!("two pawns were spawned: {:?}", site.ids);
            };

            let actions = site.tick();

            assert_eq!(actions.len(), 2, "{actions:?}");
            assert!(
                matches!(actions[0], (id, JobAction::Walk { .. }) if id == first),
                "{actions:?}"
            );
            assert_eq!(actions[1], (second, JobAction::Wait));
            assert_eq!(site.claimed_by([6, 1, 0]), Some(first));
            assert_eq!(site.held(), [(first, [6, 1, 0])]);
            assert!(matches!(
                site.crew.get(second).map(|pawn| &pawn.job),
                Some(Job::Idle { .. })
            ));
        }

        #[test]
        fn three_pawns_and_three_marks_each_pawn_digs_one() {
            let mut site = CrewSite::flat(&[cell(0, 1, -8), cell(0, 1, 0), cell(0, 1, 8)]);
            for z in [-8, 0, 8] {
                site.mark_block(cell(4, 1, z));
            }
            let mut digs: Vec<(PawnId, BlockPos)> = Vec::new();

            for _ in 0..60 {
                for (id, action) in site.tick() {
                    if let JobAction::Dig { block, .. } = action {
                        digs.push((id, block));
                    }
                }
            }

            let expected: Vec<(PawnId, BlockPos)> = site
                .ids
                .iter()
                .copied()
                .zip([cell(4, 1, -8), cell(4, 1, 0), cell(4, 1, 8)])
                .collect();
            let mut dug = digs.clone();
            dug.sort_by_key(|&(id, _)| id);
            assert_eq!(site.ids.len(), 3);
            assert_eq!(
                dug, expected,
                "each pawn digs the block of its own row, once"
            );
            assert_eq!(digs.len(), 3, "no block is dug twice: {digs:?}");
            assert_eq!(site.marks.iter().count(), 0, "every mark is worked off");
        }

        #[test]
        fn each_pawn_takes_the_mark_nearest_itself() {
            for (west_pawn, east_pawn) in [(0, 1), (1, 0)] {
                let mut starts = [cell(-10, 1, 0), cell(10, 1, 0)];
                if west_pawn == 1 {
                    starts.reverse();
                }
                let mut site = CrewSite::flat(&starts);
                site.mark_block(cell(-6, 1, 0));
                site.mark_block(cell(6, 1, 0));

                let actions = site.tick();

                let (west, east) = (site.ids[west_pawn], site.ids[east_pawn]);
                assert_eq!(actions.len(), 2, "{actions:?}");
                assert_eq!(site.claimed_by([-6, 1, 0]), Some(west), "{starts:?}");
                assert_eq!(site.claimed_by([6, 1, 0]), Some(east), "{starts:?}");
                assert!(
                    walk_target(&actions, west).is_some_and(|to| to.x > starts[west_pawn].x as f32),
                    "the western pawn walks east, toward its mark: {actions:?}"
                );
                assert!(
                    walk_target(&actions, east).is_some_and(|to| to.x < starts[east_pawn].x as f32),
                    "the eastern pawn walks west, toward its mark: {actions:?}"
                );
            }
        }

        #[test]
        fn two_pawns_beside_one_mark_dig_it_once() {
            // Both stand where the block is within reach; the first digs it and lets go of
            // the claim, which must not hand it to the second in the same tick.
            let mut site = CrewSite::flat(&[cell(3, 1, 0), cell(3, 1, 1)]);
            site.mark_block(cell(4, 1, 0));
            let [first, second] = site.ids[..] else {
                panic!("two pawns were spawned: {:?}", site.ids);
            };

            let actions = site.tick();

            let digs: Vec<PawnId> = actions
                .iter()
                .filter(|(_, action)| matches!(action, JobAction::Dig { .. }))
                .map(|&(id, _)| id)
                .collect();
            assert_eq!(digs, [first], "{actions:?}");
            assert_eq!(actions.len(), 2);
            assert_eq!(actions[1], (second, JobAction::Wait));
        }

        #[test]
        fn a_pawn_freed_by_a_cancel_walks_to_the_pending_mark() {
            let mut site = CrewSite::flat(&[cell(0, 1, 0)]);
            site.mark_block(cell(5, 1, 0));
            site.mark_block(cell(0, 1, 9));
            let pawn = site.ids[0];
            site.tick();
            assert_eq!(
                site.claimed_by([5, 1, 0]),
                Some(pawn),
                "walking to the nearer"
            );
            assert_eq!(site.claimed_by([0, 1, 9]), None, "the other is pending");
            let before = site.poses[&pawn].feet;

            site.marks.toggle([5, 1, 0]);
            let mut walked_to = None;
            for _ in 0..3 {
                let actions = site.tick();
                if let Some(to) = walk_target(&actions, pawn) {
                    walked_to = Some(to);
                    break;
                }
            }

            assert_eq!(site.claimed_by([5, 1, 0]), None);
            assert_eq!(site.claimed_by([0, 1, 9]), Some(pawn));
            let to = walked_to.expect("the freed pawn walks again within three ticks");
            assert!(
                to.x <= before.x + EPS,
                "toward the pending mark, not on toward the cancelled one: {to} from {before}"
            );
        }

        #[test]
        fn step_crew_steps_pawns_in_id_order() {
            let mut site = CrewSite::flat(&[cell(0, 1, 0), cell(2, 1, 0), cell(-2, 1, 0)]);

            let actions = site.tick();

            let waiting: Vec<(PawnId, JobAction)> =
                site.ids.iter().map(|&id| (id, JobAction::Wait)).collect();
            assert_eq!(site.ids, [PawnId(0), PawnId(1), PawnId(2)]);
            assert_eq!(actions, waiting, "one answer per pawn, in id order");
        }

        /// A crew of two, the second holding the mark at (8, 1, 0) while the first has no pose
        /// (and so takes no turn).
        fn second_pawn_holding_a_mark() -> CrewSite {
            let mut site = CrewSite::flat(&[cell(-6, 1, 0), cell(6, 1, 0)]);
            site.mark_block(cell(8, 1, 0));
            let first_pose = site
                .poses
                .remove(&site.ids[0])
                .expect("a pose to set aside");
            site.tick();
            assert_eq!(site.held(), [(site.ids[1], [8, 1, 0])], "setup");
            site.poses.insert(site.ids[0], first_pose);
            site
        }

        #[test]
        fn a_mark_ordered_again_is_held_by_exactly_one_pawn() {
            let mut site = second_pawn_holding_a_mark();
            site.marks.toggle([8, 1, 0]);
            site.marks.toggle([8, 1, 0]);

            site.tick();

            let held = site.held();
            let holders: Vec<PawnId> = held
                .iter()
                .filter(|&&(_, mark)| mark == [8, 1, 0])
                .map(|&(id, _)| id)
                .collect();
            assert_eq!(holders.len(), 1, "{held:?}");
            for (id, mark) in held {
                assert_eq!(site.claimed_by(mark), Some(id), "pawn {id:?}");
            }
        }

        #[test]
        fn a_pawn_without_a_pose_keeps_its_claim_and_is_skipped() {
            let mut site = second_pawn_holding_a_mark();
            let (first, second) = (site.ids[0], site.ids[1]);
            site.poses.remove(&second);

            let actions = site.tick();

            assert_eq!(actions.len(), 1, "{actions:?}");
            assert_eq!(actions[0], (first, JobAction::Wait));
            assert_eq!(site.claimed_by([8, 1, 0]), Some(second));
            assert_eq!(site.held(), [(second, [8, 1, 0])]);
        }

        proptest! {
            #[test]
            fn no_mark_is_ever_claimed_by_two_pawns(
                // Starts reach into the marks' rows, so some pawns begin beside a mark.
                starts in proptest::collection::btree_set((-8..=8i32, 0..=9i32), 1..=4),
                marked in proptest::collection::btree_set((-8..=8i32, 3..=8i32), 1..=6),
                cancels in proptest::collection::vec(
                    proptest::collection::vec(0..6usize, 0..3),
                    20,
                ),
            ) {
                let marked: Vec<[i32; 3]> = marked
                    .into_iter()
                    .map(|(x, z)| cell(x, 1, z).to_array())
                    .collect();
                let starts: Vec<BlockPos> = starts
                    .into_iter()
                    .map(|(x, z)| cell(x, 1, z))
                    .filter(|start| !marked.contains(&start.to_array()))
                    .collect();
                prop_assume!(!starts.is_empty());
                let mut site = CrewSite::flat(&starts);
                for mark in &marked {
                    site.mark_block(BlockPos::from_array(*mark));
                }
                let mut dug_per_block: BTreeMap<[i32; 3], usize> = BTreeMap::new();

                let first = site.tick();

                let first_digs = first
                    .iter()
                    .filter(|(_, action)| matches!(action, JobAction::Dig { .. }))
                    .count();
                prop_assert!(
                    site.held().len() + first_digs >= starts.len().min(marked.len()),
                    "every pawn takes or digs a mark while there are marks to take: {:?}",
                    first
                );
                for (_, action) in &first {
                    if let JobAction::Dig { block, .. } = action {
                        *dug_per_block.entry(block.to_array()).or_default() += 1;
                    }
                }
                for cancel in &cancels {
                    for &index in cancel {
                        site.marks.toggle(marked[index % marked.len()]);
                    }
                    let actions = site.tick();
                    for (_, action) in &actions {
                        if let JobAction::Dig { block, .. } = action {
                            *dug_per_block.entry(block.to_array()).or_default() += 1;
                        }
                    }

                    let held = site.held();
                    let distinct: BTreeSet<[i32; 3]> = held.iter().map(|&(_, mark)| mark).collect();
                    prop_assert_eq!(distinct.len(), held.len(), "two pawns hold one mark: {:?}", held);
                    for (id, mark) in held {
                        prop_assert_eq!(site.claimed_by(mark), Some(id));
                    }
                    let digs: Vec<BlockPos> = actions
                        .iter()
                        .filter_map(|&(_, action)| match action {
                            JobAction::Dig { block, .. } => Some(block),
                            _ => None,
                        })
                        .collect();
                    let dug: BTreeSet<[i32; 3]> = digs.iter().map(BlockPos::to_array).collect();
                    prop_assert_eq!(dug.len(), digs.len(), "one block dug twice: {:?}", digs);
                }
                prop_assert!(
                    dug_per_block.values().all(|&times| times == 1),
                    "a block was dug more than once: {:?}",
                    dug_per_block
                );
            }
        }
    }
}
