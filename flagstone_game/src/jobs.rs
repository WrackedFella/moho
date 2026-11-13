//! What a pawn does when nobody possesses it: take a marked block, walk to it, dig it.
//!
//! The loop reads the marks, the grid and the pawn's pose and answers with a plain action;
//! the caller applies it through the pawn's own movement and mining.

use std::f32::consts::{FRAC_PI_3, PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

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

#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Idle {
        /// Candidate marks earlier ticks already tried to route to, so a capped search
        /// resumes past them.
        tried: usize,
    },
    Mine {
        mark: [i32; 3],
        /// Cells to walk, from the pawn's cell to the stand cell.
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

impl Default for Job {
    fn default() -> Self {
        Self::Idle { tried: 0 }
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

/// Takes the nearest routable unclaimed mark, trying at most [`ROUTE_ATTEMPTS_PER_TICK`]
/// routes. `tried` counts the candidates already attempted by earlier ticks.
fn claim_nearest(
    tried: &mut usize,
    pawn: &mut Pawn,
    id: PawnId,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Option<Job> {
    let start = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| grid.is_solid_at(block) && has_exposed_face(grid, block))
        .collect();
    // Stable, so equal distances keep key order.
    candidates.sort_by_key(|&block| (block - start).length_squared());

    let attempts = candidates
        .iter()
        .skip(*tried)
        .take(ROUTE_ATTEMPTS_PER_TICK)
        .copied();
    let mut attempted = 0;
    for block in attempts {
        attempted += 1;
        let Some(route) = route(
            grid,
            start,
            |cell| is_stand_cell(block, cell),
            block,
            ROUTE_NODE_BUDGET,
        ) else {
            continue;
        };
        let mark = block.to_array();
        if marks.claim(mark, id) {
            pawn.select_slot(TOOL_SLOT);
            return Some(Job::Mine {
                mark,
                route,
                next: 0,
                stall: StallGuard::default(),
            });
        }
    }
    *tried = if *tried + attempted >= candidates.len() {
        0
    } else {
        *tried + attempted
    };
    None
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
    if let Job::Idle { tried } = job {
        match claim_nearest(tried, pawn, id, pose, marks, grid) {
            Some(claimed) => *job = claimed,
            None => return JobAction::Wait,
        }
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
    let held = *mark;
    let block = BlockPos::from_array(held);

    if !marks.contains(held) || !grid.is_solid_at(block) || !has_exposed_face(grid, block) {
        release(job, marks, held);
        return JobAction::Wait;
    }

    let arrived =
        |cell: BlockPos| horizontal_distance(pose.feet, waypoint_centre(cell)) <= ARRIVE_RADIUS;
    let reached = route[*next..]
        .iter()
        .take_while(|&&cell| arrived(cell))
        .count();
    if reached > 0 {
        *next += reached;
        *stall = StallGuard::default();
    }

    let Some(&waypoint) = route.get(*next) else {
        let aim = nearest_exposed_face(grid, block, eye(pose.feet));
        release(job, marks, held);
        return JobAction::Dig { block, aim };
    };
    let to = waypoint_centre(waypoint);
    if !standable(grid, waypoint) {
        release(job, marks, held);
        return JobAction::Wait;
    }

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

        #[test]
        fn route_attempts_are_capped_per_tick_and_resume_next_tick() {
            let mut site = Site::flat();
            // Five marks nearer than the reachable one, each a block floating where no
            // ground lets the pawn stand beside it.
            for x in 1..=5 {
                site.mark_block(cell(x, 5, 0));
            }
            site.mark_block(cell(11, 1, 0));

            let first = site.tick();
            let claimed_after_first = site.claimed_by([11, 1, 0]);
            let second = site.tick();

            assert_eq!(first, JobAction::Wait, "four failed routes use the tick up");
            assert_eq!(claimed_after_first, None);
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
            }
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
        fn marks_that_all_failed_to_route_are_tried_again_from_the_nearest_next_tick() {
            let mut site = Site::flat();
            // Two floating blocks no ground lets the pawn stand beside.
            site.mark_block(cell(1, 5, 0));
            site.mark_block(cell(2, 5, 0));

            let first = site.tick();
            site.mark_block(cell(4, 1, 0));
            let second = site.tick();

            assert_eq!(first, JobAction::Wait);
            assert_ne!(second, JobAction::Wait);
            assert_eq!(site.claimed_by([4, 1, 0]), Some(ME));
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
}
