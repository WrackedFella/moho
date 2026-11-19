//! The job loop: an idle pawn takes a marked block, walks to it and digs it.

use std::f32::consts::{FRAC_PI_3, PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// How close, horizontally, the feet must be to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Route searches an idle pawn may spend in one tick.
pub const MAX_ROUTE_ATTEMPTS: usize = 4;

/// Where a pawn is and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub eye: Vec3,
    /// Radians; facing is `(sin yaw, 0, cos yaw)`.
    pub yaw: f32,
}

/// Counts ticks without progress toward the current waypoint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    best: Option<f32>,
    ticks: u32,
}

impl StallGuard {
    /// Records this tick's horizontal distance to the waypoint; `true` once
    /// [`STALL_TICKS`] ticks in a row have not beaten the best distance so far.
    fn stalled(&mut self, distance: f32) -> bool {
        if self.best.is_none_or(|best| distance < best) {
            self.best = Some(distance);
            self.ticks = 0;
        } else {
            self.ticks += 1;
        }
        self.ticks >= STALL_TICKS
    }
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default, PartialEq)]
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

/// What the glue does for a pawn this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    Walk { to: Vec3 },
    Dig { block: BlockPos, aim: Vec3 },
}

/// How to move the body this tick toward a target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Radians to add to the yaw, no more than one tick's share of [`TURN_RATE`].
    pub yaw_delta: f32,
    /// `1.0` to walk at full speed, `0.0` to stand and turn.
    pub forward: f32,
}

/// Advances `job` one tick: claims and routes to a mark when idle, follows the route
/// while mining, and reports what the pawn does about it.
pub fn step(
    job: &mut Job,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(job, Job::Idle) {
        match claim_next(pawn, id, pose, marks, grid) {
            Some(claimed) => *job = claimed,
            None => return JobAction::Wait,
        }
    }
    let Job::Mine { mark, .. } = job else {
        return JobAction::Wait;
    };
    let mark = *mark;

    let action = follow(job, pose, marks, grid);

    match action {
        Some(walk @ JobAction::Walk { .. }) => walk,
        // A dig, or giving up, ends the job; the mark stays unless the dig removes its block.
        other => {
            marks.release(mark.to_array());
            *job = Job::Idle;
            other.unwrap_or(JobAction::Wait)
        }
    }
}

/// Offsets to the six neighbours of a block.
const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

fn exposed(grid: &VoxelGrid, block: BlockPos) -> bool {
    FACES.iter().any(|&face| !grid.is_solid_at(block + face))
}

/// Whether the pawn may dig `mark` standing with its feet in `cell`: close enough that
/// the dig is inside [`crate::pawn::MINE_REACH`], and not on top of the block.
fn is_stand_cell(grid: &VoxelGrid, mark: BlockPos, cell: BlockPos) -> bool {
    (cell.x - mark.x).abs() <= 1
        && (cell.z - mark.z).abs() <= 1
        && (mark.y - 1..=mark.y + 2).contains(&cell.y)
        && cell != mark + BlockPos::Y
        && standable(grid, cell)
}

/// Claims the nearest routable mark, resuming after where an earlier tick stopped, and
/// equips the tool. `None` when nothing was claimed this tick.
fn claim_next(
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let start = feet_cell(pose.feet);
    let cursor = pawn.search_cursor;
    let mut candidates: Vec<(i32, [i32; 3])> = marks
        .unclaimed()
        .map(|pos| ((start - BlockPos::from(pos)).length_squared(), pos))
        .filter(|candidate| cursor.is_none_or(|after| *candidate > after))
        .collect();
    candidates.sort_unstable();

    let mut attempts = 0;
    let mut last_tried = None;
    for candidate in candidates {
        let mark = BlockPos::from(candidate.1);
        if !exposed(grid, mark) {
            continue;
        }
        if attempts == MAX_ROUTE_ATTEMPTS {
            pawn.search_cursor = last_tried;
            return None;
        }
        attempts += 1;
        last_tried = Some(candidate);

        let Some(route) = route(
            grid,
            start,
            |cell| is_stand_cell(grid, mark, cell),
            mark,
            ROUTE_NODE_BUDGET,
        ) else {
            continue;
        };
        if !marks.claim(candidate.1, id) {
            continue;
        }
        pawn.search_cursor = None;
        pawn.select_slot(TOOL_SLOT);
        return Some(Job::Mine {
            mark,
            route,
            next: 0,
            stall: StallGuard::default(),
        });
    }
    pawn.search_cursor = None;
    None
}

fn horizontal_distance(feet: Vec3, cell: BlockPos) -> f32 {
    let centre = cell_centre(cell);
    (feet.x - centre.x).hypot(feet.z - centre.z)
}

fn cell_centre(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

/// One tick of a claimed job: `Walk` to keep going, `Dig` when at the stand cell, `None`
/// when the job cannot continue.
fn follow(job: &mut Job, pose: Pose, marks: &MineMarks, grid: &VoxelGrid) -> Option<JobAction> {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        return None;
    };
    if !marks.contains(mark.to_array()) || !exposed(grid, *mark) {
        return None;
    }

    if route
        .get(*next)
        .is_some_and(|&cell| horizontal_distance(pose.feet, cell) <= ARRIVE_RADIUS)
    {
        *next += 1;
        *stall = StallGuard::default();
    }

    let Some(&waypoint) = route.get(*next) else {
        return Some(JobAction::Dig {
            block: *mark,
            aim: nearest_exposed_face(grid, *mark, pose.eye),
        });
    };
    if !standable(grid, waypoint) || stall.stalled(horizontal_distance(pose.feet, waypoint)) {
        return None;
    }
    Some(JobAction::Walk {
        to: cell_centre(waypoint),
    })
}

/// The centre of the exposed face of `block` nearest `eye`; the block's own centre when
/// every face is covered.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Vec3 {
    let centre = block.as_vec3() + Vec3::splat(0.5);
    FACES
        .iter()
        .filter(|&&face| !grid.is_solid_at(block + face))
        .map(|&face| centre + face.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
        .unwrap_or(centre)
}

/// Turns toward `to` at [`TURN_RATE`] and walks once within 60 degrees of facing it.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let (dx, dz) = (to.x - pose.feet.x, to.z - pose.feet.z);
    if dx.hypot(dz) < f32::EPSILON {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let error = (dx.atan2(dz) - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-max_turn, max_turn),
        forward: if error.abs() <= FRAC_PI_3 { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::MINE_REACH;
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    const ID: PawnId = PawnId(0);
    const EYE_HEIGHT: f32 = 1.15;

    fn pose_at(feet: Vec3) -> Pose {
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    fn facing(yaw: f32) -> Pose {
        Pose {
            yaw,
            ..pose_at(Vec3::new(0.5, 1.0, 0.5))
        }
    }

    fn start() -> Vec3 {
        Vec3::new(2.5, 1.0, 2.5)
    }

    fn centre(x: i32, y: i32, z: i32) -> Vec3 {
        Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5)
    }

    fn block(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    struct Scene {
        grid: VoxelGrid,
        marks: MineMarks,
        pawn: Pawn,
        job: Job,
    }

    impl Scene {
        /// A 16 x 16 floor at y = 0.
        fn new() -> Self {
            let mut grid = VoxelGrid::new(16);
            for x in 0..16 {
                for z in 0..16 {
                    grid.mutator().place(block(x, 0, z), 1, None);
                }
            }
            Self {
                grid,
                marks: MineMarks::default(),
                pawn: Pawn::default(),
                job: Job::default(),
            }
        }

        fn solid(&mut self, pos: [i32; 3]) {
            self.grid.mutator().place(BlockPos::from(pos), 1, None);
        }

        fn marked_block(mut self, pos: [i32; 3]) -> Self {
            self.solid(pos);
            self.marks.toggle(pos);
            self
        }

        fn step(&mut self, pose: Pose) -> JobAction {
            step(
                &mut self.job,
                &mut self.pawn,
                ID,
                pose,
                &mut self.marks,
                &self.grid,
            )
        }

        fn claimed_by(&self, pos: [i32; 3]) -> Option<PawnId> {
            self.marks
                .iter()
                .find(|(p, _)| *p == pos)
                .expect("the block is marked")
                .1
                .claimed_by
        }

        fn is_idle(&self) -> bool {
            matches!(self.job, Job::Idle)
        }

        /// Walks the route by standing on each waypoint the pawn is sent to, until it digs.
        fn walk_until_dig(&mut self, from: Vec3) -> (Pose, JobAction) {
            let mut pose = pose_at(from);
            for _ in 0..200 {
                match self.step(pose) {
                    JobAction::Walk { to } => pose = pose_at(to),
                    dig @ JobAction::Dig { .. } => return (pose, dig),
                    JobAction::Wait => panic!("the pawn stopped before digging"),
                }
            }
            panic!("the pawn never dug");
        }
    }

    fn assert_horizontal(to: Vec3, x: f32, z: f32) {
        assert!(
            (to.x - x).abs() < 1e-4 && (to.z - z).abs() < 1e-4,
            "walks to ({x}, {z}), not ({}, {})",
            to.x,
            to.z
        );
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let mut scene = Scene::new();

        let action = scene.step(pose_at(start()));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        for pos in [[7, 1, 2], [9, 1, 2], [8, 1, 1], [8, 1, 3], [8, 2, 2]] {
            scene.solid(pos);
        }

        let action = scene.step(pose_at(start()));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by([8, 1, 2]), None);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut scene = Scene::new()
            .marked_block([1, 1, 12])
            .marked_block([8, 1, 2]);
        scene.pawn.select_slot(1);

        let action = scene.step(pose_at(start()));

        let JobAction::Walk { to } = action else {
            panic!("expected a walk, got {action:?}");
        };
        assert_horizontal(to, 3.5, 2.5);
        assert_eq!(scene.claimed_by([8, 1, 2]), Some(ID));
        assert_eq!(scene.claimed_by([1, 1, 12]), None);
        assert_eq!(scene.pawn.equipped_tool, Some(STARTING_TOOL));
        assert!(matches!(scene.job, Job::Mine { mark, .. } if mark == block(8, 1, 2)));
    }

    #[test]
    fn two_equidistant_marks_claim_the_lower_key() {
        let mut scene = Scene::new().marked_block([5, 1, 2]).marked_block([2, 1, 5]);

        let action = scene.step(pose_at(start()));

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(scene.claimed_by([2, 1, 5]), Some(ID));
        assert_eq!(scene.claimed_by([5, 1, 2]), None);
    }

    #[test]
    fn an_unreachable_nearest_mark_is_skipped_for_the_next() {
        let mut scene = Scene::new()
            .marked_block([3, 8, 2])
            .marked_block([12, 1, 2]);

        let action = scene.step(pose_at(start()));

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(scene.claimed_by([3, 8, 2]), None);
        assert_eq!(scene.claimed_by([12, 1, 2]), Some(ID));
    }

    fn behind_unreachable_marks(count: i32) -> Scene {
        let mut scene = Scene::new();
        for x in 0..count {
            scene = scene.marked_block([x, 8, 0]);
        }
        scene.marked_block([14, 1, 14])
    }

    #[test]
    fn a_pawn_behind_three_unreachable_marks_reaches_a_fourth_in_one_tick() {
        let mut scene = behind_unreachable_marks(3);

        let first = scene.step(pose_at(start()));

        assert!(matches!(first, JobAction::Walk { .. }), "{first:?}");
        assert_eq!(scene.claimed_by([14, 1, 14]), Some(ID));
    }

    #[test]
    fn a_pawn_behind_four_unreachable_marks_waits_a_tick_then_reaches_a_fifth() {
        let mut scene = behind_unreachable_marks(4);

        let first = scene.step(pose_at(start()));
        let claimed_after_first = scene.claimed_by([14, 1, 14]);
        let second = scene.step(pose_at(start()));

        assert_eq!(first, JobAction::Wait, "four attempts a tick, then wait");
        assert_eq!(claimed_after_first, None);
        assert!(matches!(second, JobAction::Walk { .. }), "{second:?}");
        assert_eq!(scene.claimed_by([14, 1, 14]), Some(ID));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        let first = scene.step(pose_at(start()));
        let JobAction::Walk { to } = first else {
            panic!("expected a walk, got {first:?}");
        };
        assert_horizontal(to, 3.5, 2.5);

        let halfway = scene.step(pose_at(Vec3::new(3.0, 1.0, 2.5)));
        let JobAction::Walk { to } = halfway else {
            panic!("expected a walk, got {halfway:?}");
        };
        assert_horizontal(to, 3.5, 2.5);

        for x in 3..7 {
            let action = scene.step(pose_at(centre(x, 1, 2)));
            let JobAction::Walk { to } = action else {
                panic!("expected a walk at x={x}, got {action:?}");
            };
            assert_horizontal(to, x as f32 + 1.5, 2.5);
        }
        let mut at_stand = pose_at(centre(7, 1, 2));
        at_stand.eye = Vec3::new(7.5, 1.5, 2.5);
        let action = scene.step(at_stand);

        assert_eq!(
            action,
            JobAction::Dig {
                block: block(8, 1, 2),
                aim: Vec3::new(8.0, 1.5, 2.5),
            }
        );
    }

    #[test]
    fn a_dig_ends_the_job_and_releases_the_claim_keeping_the_mark() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);

        let (_, action) = scene.walk_until_dig(start());

        assert!(matches!(action, JobAction::Dig { .. }));
        assert!(scene.is_idle());
        assert!(
            scene.marks.contains([8, 1, 2]),
            "the mark goes with the block"
        );
        assert_eq!(scene.claimed_by([8, 1, 2]), None);
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let cases = [
            (
                Vec3::new(8.2, 0.6, 2.5),
                Vec3::new(8.0, 1.5, 2.5),
                "below the block: the bottom face is buried in the floor, so the west face",
            ),
            (
                Vec3::new(7.5, 2.9, 2.5),
                Vec3::new(8.5, 2.0, 2.5),
                "above and beside the block: the top face is nearer than the west face",
            ),
        ];

        for (eye, expected, name) in cases {
            let mut scene = Scene::new().marked_block([8, 1, 2]);
            scene.step(pose_at(start()));
            for x in 3..=6 {
                scene.step(pose_at(centre(x, 1, 2)));
            }
            let mut at_stand = pose_at(centre(7, 1, 2));
            at_stand.eye = eye;

            let action = scene.step(at_stand);

            let JobAction::Dig { aim, .. } = action else {
                panic!("{name}: expected a dig, got {action:?}");
            };
            assert_eq!(aim, expected, "{name}");
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        for dug_by_another_pawn in [false, true] {
            let mut scene = Scene::new().marked_block([8, 1, 2]);
            scene.step(pose_at(start()));
            scene.step(pose_at(centre(3, 1, 2)));
            if dug_by_another_pawn {
                scene.grid.mutator().remove(block(8, 1, 2));
                scene.marks.block_removed([8, 1, 2]);
            } else {
                scene.marks.toggle([8, 1, 2]);
            }

            let action = scene.step(pose_at(centre(4, 1, 2)));

            assert_eq!(
                action,
                JobAction::Wait,
                "dug_by_another_pawn: {dug_by_another_pawn}"
            );
            assert!(
                scene.is_idle(),
                "dug_by_another_pawn: {dug_by_another_pawn}"
            );
        }
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        scene.marks.claim([8, 1, 2], PawnId(1));

        let action = scene.step(pose_at(start()));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert_eq!(scene.claimed_by([8, 1, 2]), Some(PawnId(1)));
    }

    // Progress means a new best (smaller) horizontal distance to the current waypoint;
    // standing still, or only matching an earlier distance, is a stalled tick.
    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        scene.step(pose_at(start()));
        let mut released_at = None;

        for tick in 1..=STALL_TICKS + 5 {
            scene.step(pose_at(start()));
            if scene.is_idle() {
                released_at = Some(tick);
                break;
            }
        }

        let tick = released_at.expect("the pawn gives up");
        assert!(tick >= STALL_TICKS - 1, "gave up after only {tick} ticks");
        assert!(tick <= STALL_TICKS + 2, "gave up after {tick} ticks");
        assert!(scene.marks.contains([8, 1, 2]));
        assert_eq!(scene.claimed_by([8, 1, 2]), None);
    }

    // Each tick closes 0.02 m on the waypoint, a new best horizontal distance every time.
    #[test]
    fn steady_progress_never_counts_as_a_stall() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        let mut feet = start();
        let mut dug = false;

        for _ in 0..600 {
            match scene.step(pose_at(feet)) {
                JobAction::Walk { to } => {
                    let toward = (to - feet).with_y(0.0);
                    feet += toward.normalize_or_zero() * toward.length().min(0.02);
                }
                JobAction::Dig { .. } => {
                    dug = true;
                    break;
                }
                JobAction::Wait => panic!("a pawn making progress was released"),
            }
        }

        assert!(dug, "the pawn reaches the block at 0.02 m a tick");
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        scene.step(pose_at(start()));
        scene.grid.mutator().remove(block(3, 0, 2));

        let action = scene.step(pose_at(start()));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert!(scene.marks.contains([8, 1, 2]));
        assert_eq!(scene.claimed_by([8, 1, 2]), None);
    }

    #[test]
    fn a_mark_that_loses_its_exposed_face_after_claim_releases_the_pawn() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        scene.step(pose_at(start()));
        for pos in [[7, 1, 2], [9, 1, 2], [8, 1, 1], [8, 1, 3], [8, 2, 2]] {
            scene.solid(pos);
        }

        let action = scene.step(pose_at(start()));

        assert_eq!(action, JobAction::Wait);
        assert!(scene.is_idle());
        assert!(scene.marks.contains([8, 1, 2]));
        assert_eq!(scene.claimed_by([8, 1, 2]), None);
    }

    #[test]
    fn the_cell_on_top_of_the_marked_block_is_not_a_stand_cell() {
        let mut scene = Scene::new().marked_block([8, 1, 2]);
        let mut pose = pose_at(centre(8, 2, 2));

        let first = scene.step(pose);

        let JobAction::Walk { to } = first else {
            panic!("standing on the block is not a place to dig it from: {first:?}");
        };
        pose = pose_at(to);
        let mut dig_pose = None;
        for _ in 0..200 {
            match scene.step(pose) {
                JobAction::Walk { to } => pose = pose_at(to),
                JobAction::Dig { .. } => {
                    dig_pose = Some(pose);
                    break;
                }
                JobAction::Wait => panic!("the pawn stopped before digging"),
            }
        }
        let dig_pose = dig_pose.expect("the pawn digs from beside the block");
        assert_ne!(
            (
                dig_pose.feet.x.floor() as i32,
                dig_pose.feet.z.floor() as i32
            ),
            (8, 2),
            "the pawn digs from beside the block, not from on it"
        );
    }

    const MAX_TURN: f32 = TURN_RATE / TICK_HZ as f32;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let right = steer(facing(0.0), Vec3::new(5.5, 1.0, 0.5));
        let left = steer(facing(0.0), Vec3::new(-4.5, 1.0, 0.5));
        let across_the_wrap = steer(
            facing(3.0),
            Vec3::new(
                0.5 + (-3.0_f32).sin() * 4.0,
                1.0,
                0.5 + (-3.0_f32).cos() * 4.0,
            ),
        );

        assert_near(right.yaw_delta, MAX_TURN);
        assert_near(left.yaw_delta, -MAX_TURN);
        assert_near(across_the_wrap.yaw_delta, MAX_TURN);
    }

    #[test]
    fn steer_does_not_overshoot_a_small_turn() {
        let off_by = 0.05_f32;
        let target = Vec3::new(0.5 + off_by.sin() * 4.0, 1.0, 0.5 + off_by.cos() * 4.0);

        let step = steer(facing(0.0), target);

        assert_near(step.yaw_delta, off_by);
    }

    #[test]
    fn steer_walks_straight_at_a_target_dead_ahead() {
        let step = steer(facing(0.0), Vec3::new(0.5, 2.5, 0.9));

        assert_near(step.yaw_delta, 0.0);
        assert_eq!(
            step.forward, 1.0,
            "the target is 75 degrees off in 3D but dead ahead horizontally"
        );
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let at = |degrees: f32| {
            let angle = degrees.to_radians();
            steer(
                facing(0.0),
                Vec3::new(0.5 + angle.sin() * 4.0, 1.0, 0.5 + angle.cos() * 4.0),
            )
            .forward
        };

        assert_eq!(at(0.0), 1.0);
        assert_eq!(at(-30.0), 1.0);
        assert_eq!(at(55.0), 1.0);
        assert_eq!(at(65.0), 0.0);
        assert_eq!(at(-65.0), 0.0);
        assert_eq!(at(180.0), 0.0, "turns in place when the target is behind");
    }

    proptest! {
        #[test]
        fn steer_never_turns_past_the_rate_or_away_from_the_target(
            yaw in -10.0f32..10.0,
            dx in -8.0f32..8.0,
            dz in -8.0f32..8.0,
        ) {
            prop_assume!(dx.abs() + dz.abs() > 0.5);
            let target = Vec3::new(0.5 + dx, 1.0, 0.5 + dz);
            let wanted = dx.atan2(dz);
            let error = |yaw: f32| {
                let wrapped = (wanted - yaw).rem_euclid(std::f32::consts::TAU);
                wrapped.min(std::f32::consts::TAU - wrapped)
            };

            let step = steer(facing(yaw), target);

            prop_assert!(step.yaw_delta.abs() <= MAX_TURN + 1e-5);
            prop_assert!(error(yaw + step.yaw_delta) <= error(yaw) + 1e-4);
        }

        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 6i32..12,
            bz in 6i32..12,
            height in 1i32..=2,
            sx in 0i32..4,
            sz in 0i32..4,
        ) {
            let mut scene = Scene::new();
            for y in 1..=height {
                scene.solid([bx, y, bz]);
            }
            scene.marks.toggle([bx, height, bz]);

            let (pose, action) = scene.walk_until_dig(centre(sx, 1, sz));

            let JobAction::Dig { block: dug, aim } = action else {
                unreachable!("walk_until_dig returns a dig");
            };
            prop_assert_eq!(dug, block(bx, height, bz));
            let feet_cell = (pose.feet.x.floor() as i32, pose.feet.y.floor() as i32, pose.feet.z.floor() as i32);
            prop_assert!((feet_cell.0 - bx).abs() <= 1 && (feet_cell.2 - bz).abs() <= 1);
            prop_assert!(feet_cell.1 >= height - 1 && feet_cell.1 <= height + 2);
            prop_assert!(pose.eye.distance(aim) <= MINE_REACH);
            prop_assert!(pose.eye.distance(centre(bx, height, bz) + Vec3::new(0.0, 0.5, 0.0)) <= MINE_REACH);
        }
    }
}
