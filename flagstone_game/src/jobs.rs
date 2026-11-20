//! The pawn job loop: claim a marked block, walk the route to a stand cell, dig.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;
use std::f32::consts::{PI, TAU};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Half-angle within which a pawn walks rather than turns in place.
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

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
///
/// An idle pawn claims the nearest routable mark and starts walking to it; a pawn with a job
/// walks its route and, past the last waypoint, returns [`JobAction::Dig`], which ends the job
/// and releases the claim. A mark that vanished, a waypoint that is no longer standable or a
/// walk that stalls for [`STALL_TICKS`] drops the job and leaves the mark.
pub fn step(
    id: PawnId,
    job: &mut Job,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(job, Job::Idle) {
        let Some(claimed) = claim_nearest(id, pawn, pose, marks, grid) else {
            return JobAction::Wait;
        };
        *job = claimed;
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
    let key = mark.to_array();

    if !marks.contains(key) {
        *job = Job::Idle;
        return JobAction::Wait;
    }
    let Some(&waypoint) = route.get(*next) else {
        return finish_with_dig(job, mark, pose, marks, grid);
    };
    if !standable(grid, waypoint) {
        marks.release(key);
        *job = Job::Idle;
        return JobAction::Wait;
    }

    let to = cell_centre(waypoint);
    let distance = horizontal_distance(pose.feet, to);
    if distance <= ARRIVE_RADIUS {
        *next += 1;
        *stall = StallGuard::default();
        return match route.get(*next) {
            Some(&following) => JobAction::Walk {
                to: cell_centre(following),
            },
            None => finish_with_dig(job, mark, pose, marks, grid),
        };
    }
    if stall.best_distance.is_none_or(|best| distance < best) {
        stall.best_distance = Some(distance);
        stall.ticks_without_progress = 0;
    } else {
        stall.ticks_without_progress += 1;
    }
    if stall.ticks_without_progress >= STALL_TICKS {
        marks.release(key);
        *job = Job::Idle;
        return JobAction::Wait;
    }
    JobAction::Walk { to }
}

/// Claims the nearest unclaimed, exposed mark that routes to a stand cell, trying at most
/// [`MAX_ROUTE_ATTEMPTS`] marks, and equips the tool for it.
fn claim_nearest(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let feet = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| exposed_faces(grid, block).next().is_some())
        .collect();
    candidates.sort_by_key(|&block| (block - feet).length_squared());

    for block in candidates.into_iter().take(MAX_ROUTE_ATTEMPTS) {
        let is_stand_cell = |cell: BlockPos| {
            (cell.x - block.x).abs() <= 1
                && (cell.z - block.z).abs() <= 1
                && (block.y - 1..=block.y + 2).contains(&cell.y)
                && standable(grid, cell)
        };
        let Some(route) = route(grid, feet, is_stand_cell, block, ROUTE_NODE_BUDGET) else {
            continue;
        };
        if marks.claim(block.to_array(), id) {
            pawn.select_slot(TOOL_SLOT);
            return Some(Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            });
        }
    }
    None
}

/// Ends the job and releases the claim, returning the dig of `mark` from the nearest exposed face.
fn finish_with_dig(
    job: &mut Job,
    mark: BlockPos,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    marks.release(mark.to_array());
    *job = Job::Idle;
    let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
    let nearest = exposed_faces(grid, mark).min_by(|a, b| {
        eye.distance_squared(*a)
            .total_cmp(&eye.distance_squared(*b))
    });
    match nearest {
        Some(aim) => JobAction::Dig { block: mark, aim },
        None => JobAction::Wait,
    }
}

/// Centres of the faces of `block` that border air, in a fixed order.
fn exposed_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = Vec3> + '_ {
    [
        BlockPos::X,
        BlockPos::NEG_X,
        BlockPos::Y,
        BlockPos::NEG_Y,
        BlockPos::Z,
        BlockPos::NEG_Z,
    ]
    .into_iter()
    .filter(move |&side| !grid.is_solid_at(block + side))
    .map(move |side| block.as_vec3() + Vec3::splat(0.5) + side.as_vec3() * 0.5)
}

/// The centre of `cell` at feet height.
fn cell_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a - b).with_y(0.0).length()
}

/// Turn toward `to` at [`TURN_RATE`] and walk once within 60 degrees of facing it.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let toward = (to - pose.feet).with_y(0.0);
    if toward.length_squared() < f32::EPSILON {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let wanted = toward.x.atan2(toward.z);
    let error = (wanted - pose.yaw + PI).rem_euclid(TAU) - PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-per_tick, per_tick),
        forward: if error.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
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

    /// Ground two blocks thick (`y` in `0..=1`) over `x` and `z` in `0..=15`.
    fn thick_ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        grid.mutator()
            .fill_region(cell(0, 0, 0), cell(15, 1, 15), GROUND);
        grid
    }

    /// A marked block at `(x, 1, z)` that stands on a ground block (so it is exposed on top)
    /// inside a 3x3 pit of two-high walls the pawn cannot climb: no stand cell can be routed to.
    fn unroutable_mark(scene: &mut Scene, x: i32, z: i32) {
        scene
            .grid
            .mutator()
            .fill_region(cell(x - 1, 1, z - 1), cell(x + 1, 2, z + 1), GROUND);
        scene.grid.mutator().remove(cell(x, 2, z));
        scene.marks.toggle(key(cell(x, 1, z)));
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut scene = Scene::new(thick_ground());
        scene.grid.mutator().place(cell(6, 2, 2), GROUND, None);
        scene.marks.toggle(key(cell(6, 1, 2)));

        let action = scene.step(at(0.5, 2.0, 2.5));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_skipped_for_a_farther_exposed_one() {
        let mut scene = Scene::new(thick_ground());
        scene.grid.mutator().place(cell(6, 2, 2), GROUND, None);
        scene.marks.toggle(key(cell(6, 1, 2)));
        scene.grid.mutator().place(cell(11, 2, 2), GROUND, None);
        scene.marks.toggle(key(cell(11, 2, 2)));

        scene.step(at(0.5, 2.0, 2.5));

        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert_eq!(scene.claimed_by(cell(11, 2, 2)), Some(ME));
    }

    #[test]
    fn an_unroutable_nearer_mark_is_skipped_for_a_farther_reachable_one() {
        let mut scene = Scene::new(flat_ground());
        unroutable_mark(&mut scene, 6, 2);
        scene.mark(cell(13, 1, 12));

        scene.step(at(0.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert_eq!(scene.claimed_by(cell(13, 1, 12)), Some(ME));
    }

    /// `pits` unroutable marks, all nearer the pawn at `(0.5, 1, 0.5)` than the one reachable
    /// mark at `(14, 1, 14)`, so that mark is candidate number `pits + 1`.
    fn scene_with_pits_before_a_reachable_mark(pits: usize) -> Scene {
        let nearest_first = [(3, 3), (3, 7), (7, 7), (11, 3), (7, 11)];
        let mut scene = Scene::new(flat_ground());
        for &(x, z) in &nearest_first[..pits] {
            unroutable_mark(&mut scene, x, z);
        }
        scene.mark(cell(14, 1, 14));
        scene
    }

    #[test]
    fn the_fourth_candidate_is_claimed_within_the_same_tick_after_three_failed_routes() {
        let mut scene = scene_with_pits_before_a_reachable_mark(MAX_ROUTE_ATTEMPTS - 1);

        scene.step(at(0.5, 1.0, 0.5));

        assert_eq!(scene.claimed_by(cell(14, 1, 14)), Some(ME));
    }

    #[test]
    fn at_most_four_routes_are_tried_per_tick_and_the_fifth_candidate_waits() {
        let mut scene = scene_with_pits_before_a_reachable_mark(MAX_ROUTE_ATTEMPTS);

        let first = scene.step(at(0.5, 1.0, 0.5));
        let second = scene.step(at(0.5, 1.0, 0.5));

        assert_eq!(MAX_ROUTE_ATTEMPTS, 4);
        assert_eq!(first, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by(cell(14, 1, 14)), None);
        assert_eq!(
            second,
            JobAction::Wait,
            "no cursor across ticks: the same four nearer candidates are tried again"
        );
        assert!(scene.is_idle());
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.mark(cell(12, 1, 2));
        scene.pawn.select_slot(1);

        scene.step(at(15.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(12, 1, 2)), Some(ME));
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
        assert_eq!(scene.pawn.selected_slot, TOOL_SLOT);
        assert_eq!(scene.pawn.equipped_tool, Some(STARTING_TOOL));
        match &scene.job {
            Job::Mine { mark, next, .. } => {
                assert_eq!(*mark, cell(12, 1, 2));
                assert_eq!(*next, 0);
            }
            Job::Idle => panic!("the pawn should have taken the mark"),
        }
    }

    #[test]
    fn equally_near_marks_go_to_the_first_in_key_order() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(11, 1, 2));
        scene.mark(cell(7, 1, 2));

        scene.step(at(9.5, 1.0, 2.5));

        assert_eq!(scene.claimed_by(cell(7, 1, 2)), Some(ME));
        assert_eq!(scene.claimed_by(cell(11, 1, 2)), None);
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

    /// A pawn standing at its stand cell with the mark at `(6, 1, 2)` claimed; the west side
    /// of the mark is covered by a block.
    fn digging_scene() -> Scene {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.grid.mutator().place(cell(5, 1, 2), GROUND, None);
        scene.marks.claim(key(cell(6, 1, 2)), ME);
        scene.job = Job::Mine {
            mark: cell(6, 1, 2),
            route: vec![],
            next: 0,
            stall: StallGuard::default(),
        };
        scene
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
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
            let mut scene = digging_scene();

            let action = scene.step(pose);

            match action {
                JobAction::Dig { block, aim } => {
                    assert_eq!(block, cell(6, 1, 2), "{case}");
                    assert_near(aim, face);
                }
                other => panic!("{case}: expected a dig, got {other:?}"),
            }
            assert!(scene.is_idle(), "{case}: the dig ends the job");
            assert_eq!(
                scene.claimed_by(cell(6, 1, 2)),
                None,
                "{case}: the claim is released"
            );
        }
    }

    #[test]
    fn a_pawn_already_on_a_stand_cell_digs_at_once_without_walking() {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));

        let action = scene.step(at(5.5, 1.0, 2.5));

        match action {
            JobAction::Dig { block, aim } => {
                assert_eq!(block, cell(6, 1, 2));
                assert_near(aim, Vec3::new(6.0, 1.5, 2.5));
            }
            other => panic!("expected an immediate dig, got {other:?}"),
        }
        assert!(scene.is_idle(), "the dig ends the job");
        assert_eq!(scene.claimed_by(cell(6, 1, 2)), None);
    }

    /// Walks a pawn toward the mark at `(6, 1, 2)`, then applies `withdraw`: the pawn must
    /// drop the job and wait.
    fn assert_withdrawn_mark_releases_the_pawn(withdraw: impl Fn(&mut Scene), case: &str) {
        let mut scene = Scene::new(flat_ground());
        scene.mark(cell(6, 1, 2));
        scene.step(at(0.5, 1.0, 2.5));
        scene.step(at(2.5, 1.0, 2.5));

        withdraw(&mut scene);
        let action = scene.step(at(2.5, 1.0, 2.5));

        assert_eq!(action, JobAction::Wait, "{case}");
        assert!(scene.is_idle(), "{case}");
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        assert_withdrawn_mark_releases_the_pawn(
            |scene| {
                scene.marks.toggle(key(cell(6, 1, 2)));
            },
            "cancelled by the player",
        );
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        assert_withdrawn_mark_releases_the_pawn(
            |scene| {
                scene.grid.mutator().remove(cell(6, 1, 2));
                scene.marks.block_removed(key(cell(6, 1, 2)));
            },
            "dug by another pawn",
        );
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

            let first = scene.step(at(0.5, start.y as f32, 0.5));
            let stand = if let JobAction::Dig { block: dug, .. } = first {
                prop_assert_eq!(dug, block);
                start
            } else {
                scene.step(at(0.5, start.y as f32, 0.5));
                prop_assert!(!scene.is_idle(), "a reachable mark was not claimed");
                scene.route().last().copied().unwrap_or(start)
            };
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
