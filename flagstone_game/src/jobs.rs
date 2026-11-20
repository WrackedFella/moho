//! What an idle pawn does on its own: take a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::{Vec2, Vec3};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Route searches an idle pawn may make in one tick.
const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// How far a pawn may be turned away from its target and still walk, in radians (60 degrees).
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// The squared horizontal distance, in square metres, under which a pawn is at its target and
/// has no direction to turn toward: 2^-30, so that 2^-15 m is the first distance that has one.
const AT_TARGET_SQUARED: f32 = 1.0 / 1_073_741_824.0;

/// How much closer to the waypoint, in metres, counts as progress.
const PROGRESS_STEP: f32 = 0.01;

/// The six face normals of a block.
const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// How close, horizontally in metres, the feet must be to a waypoint's centre to have arrived.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives its mark back.
pub const STALL_TICKS: u32 = 120;

/// Turning speed, radians per second: one full turn a second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Tracks a walk's progress so a pawn that is stuck gives up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StallGuard {
    pub ticks_without_progress: u32,
    /// The closest the pawn has been to the waypoint since it started walking to it.
    pub best_distance: Option<f32>,
}

impl StallGuard {
    /// Notes the pawn is `distance` from its waypoint this tick; true once it has gone
    /// [`STALL_TICKS`] ticks without getting closer.
    fn is_stalled_at(&mut self, distance: f32) -> bool {
        if self
            .best_distance
            .is_none_or(|best| distance < best - PROGRESS_STEP)
        {
            self.best_distance = Some(distance);
            self.ticks_without_progress = 0;
        } else {
            self.ticks_without_progress += 1;
        }
        self.ticks_without_progress >= STALL_TICKS
    }
}

/// A pawn's current job.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Idle,
    Mine {
        /// The marked block being walked to and dug.
        mark: BlockPos,
        /// The cells to walk through, the stand cell last.
        route: Vec<BlockPos>,
        /// The waypoint being walked to; the route's length once all are reached.
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn is and which way it faces, as the glue reads it from the physics character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The point on the ground under the pawn.
    pub feet: Vec3,
    /// Where the pawn's mining action starts.
    pub eye: Vec3,
    /// Facing in radians; 0 faces +Z and positive turns toward +X.
    pub yaw: f32,
}

/// What the glue should do for a pawn this tick.
#[derive(Clone, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, a point on the ground.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming the pawn's mining action at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: the turn and the share of full walking speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    /// Turn this tick, in radians; positive turns toward +X.
    pub yaw_delta: f32,
    /// Share of full walking speed, 0 to 1.
    pub forward: f32,
}

/// Advances `pawn`'s job one tick: an idle pawn claims the nearest reachable unclaimed mark
/// for `id`, a pawn on a job follows it, and a job that cannot go on is dropped with its claim.
pub fn step(
    id: PawnId,
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    let job = match std::mem::replace(&mut pawn.job, Job::Idle) {
        Job::Idle => take_a_mark(id, pawn, pose, marks, grid),
        mining @ Job::Mine { .. } => mining,
    };
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = job
    else {
        return JobAction::Wait;
    };
    // A mark that was cancelled, dug by someone else, or cancelled and marked again is not
    // this pawn's any more; whoever holds it now keeps it.
    if marks.claimant(mark.to_array()) != Some(id) {
        return JobAction::Wait;
    }
    let (job, action) = follow(mark, route, next, stall, pose, marks, grid);
    pawn.job = job;
    action
}

/// Ends `pawn`'s job after its dig, handing a mark that is still there back to the colony.
pub fn finish(pawn: &mut Pawn, marks: &mut MineMarks) {
    if let Job::Mine { mark, .. } = &pawn.job {
        marks.release(mark.to_array());
    }
    pawn.job = Job::Idle;
}

/// The turn and speed that take a pawn at `pose` toward `to` this tick.
pub fn steer(pose: &Pose, to: Vec3) -> WalkStep {
    use std::f32::consts::{PI, TAU};
    let (dx, dz) = (to.x - pose.feet.x, to.z - pose.feet.z);
    if dx * dx + dz * dz < AT_TARGET_SQUARED {
        return WalkStep {
            yaw_delta: 0.0,
            forward: 0.0,
        };
    }
    let error = (dx.atan2(dz) - pose.yaw + PI).rem_euclid(TAU) - PI;
    let most = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-most, most),
        forward: if error.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

/// Takes the nearest reachable unclaimed mark that has an exposed face, if there is one.
fn take_a_mark(
    id: PawnId,
    pawn: &mut Pawn,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> Job {
    let feet = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| grid.is_solid_at(block) && exposed_faces(grid, block).next().is_some())
        .collect();
    candidates.sort_by_key(|&block| (block - feet).length_squared());

    for block in candidates.into_iter().take(ROUTE_ATTEMPTS_PER_TICK) {
        let Some(route) = route(
            grid,
            feet,
            |cell| is_stand_cell(cell, block),
            block,
            ROUTE_NODE_BUDGET,
        ) else {
            continue;
        };
        if marks.claim(block.to_array(), id) {
            pawn.select_slot(TOOL_SLOT);
            return Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::default(),
            };
        }
    }
    Job::Idle
}

/// Whether a pawn with its feet in `cell` can dig `block` from there: within one column of it
/// and with the feet from one cell below to two above, which keeps the dig inside the mining
/// reach.
fn is_stand_cell(cell: BlockPos, block: BlockPos) -> bool {
    let offset = cell - block;
    offset.x.abs() <= 1 && offset.z.abs() <= 1 && (-1..=2).contains(&offset.y)
}

/// The normals of the faces of `block` that border air.
fn exposed_faces(grid: &VoxelGrid, block: BlockPos) -> impl Iterator<Item = BlockPos> + '_ {
    FACES
        .into_iter()
        .filter(move |&normal| !grid.is_solid_at(block + normal))
}

/// Where a pawn walking to `cell` aims for: the middle of the cell, at its feet.
fn waypoint(cell: BlockPos) -> Vec3 {
    cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5)
}

fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

/// Gives the mark back and drops the job.
fn give_up(mark: BlockPos, marks: &mut MineMarks) -> (Job, JobAction) {
    marks.release(mark.to_array());
    (Job::Idle, JobAction::Wait)
}

/// One tick of a mining job: the next waypoint to walk to, or the dig once the stand cell is
/// reached.
fn follow(
    mark: BlockPos,
    route: Vec<BlockPos>,
    mut next: usize,
    mut stall: StallGuard,
    pose: &Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> (Job, JobAction) {
    if let Some(&cell) = route.get(next)
        && flat_distance(pose.feet, waypoint(cell)) <= ARRIVE_RADIUS
    {
        next += 1;
        stall = StallGuard::default();
    }
    let action = if let Some(&cell) = route.get(next) {
        if !standable(grid, cell) {
            return give_up(mark, marks);
        }
        let to = waypoint(cell);
        if stall.is_stalled_at(flat_distance(pose.feet, to)) {
            return give_up(mark, marks);
        }
        JobAction::Walk { to }
    } else {
        let centre = mark.as_vec3() + Vec3::splat(0.5);
        let Some(aim) = exposed_faces(grid, mark)
            .map(|normal| centre + normal.as_vec3() * 0.5)
            .min_by(|a, b| {
                a.distance_squared(pose.eye)
                    .total_cmp(&b.distance_squared(pose.eye))
            })
        else {
            return give_up(mark, marks);
        };
        JobAction::Dig { block: mark, aim }
    };
    let job = Job::Mine {
        mark,
        route,
        next,
        stall,
    };
    (job, action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::{MINE_REACH, TOOL_SLOT};
    use crate::tools::STARTING_TOOL;
    use proptest::prelude::*;

    /// The capsule centre above the feet, where a pawn's eye is.
    const EYE_HEIGHT: f32 = 1.15;
    const ME: PawnId = PawnId(0);

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos::new(x, y, z)
    }

    fn put(grid: &mut VoxelGrid, cells: impl IntoIterator<Item = BlockPos>) {
        for cell in cells {
            grid.mutator().place(cell, 1, None);
        }
    }

    /// Ground whose top blocks are at y = 0 over x in `0..width`, z in `0..depth`.
    fn ground(width: i32, depth: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        put(
            &mut grid,
            (0..width).flat_map(|x| (0..depth).map(move |z| at(x, 0, z))),
        );
        grid
    }

    fn pose_in(cell: BlockPos) -> Pose {
        let feet = Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5);
        Pose {
            feet,
            eye: feet + Vec3::Y * EYE_HEIGHT,
            yaw: 0.0,
        }
    }

    fn marks_on(blocks: &[BlockPos]) -> MineMarks {
        let mut marks = MineMarks::default();
        for block in blocks {
            marks.toggle(block.to_array());
        }
        marks
    }

    #[derive(Debug, PartialEq)]
    enum Holder {
        Unmarked,
        Open,
        Claimed(PawnId),
    }

    fn holder(marks: &MineMarks, block: BlockPos) -> Holder {
        match marks.iter().find(|(pos, _)| *pos == block.to_array()) {
            None => Holder::Unmarked,
            Some((_, mark)) => mark.claimed_by.map_or(Holder::Open, Holder::Claimed),
        }
    }

    fn mining(pawn: &Pawn) -> Option<(BlockPos, Vec<BlockPos>, usize)> {
        match &pawn.job {
            Job::Mine {
                mark, route, next, ..
            } => Some((*mark, route.clone(), *next)),
            Job::Idle => None,
        }
    }

    /// Steps until the pawn has a job, at most `ticks` times.
    fn until_it_has_a_job(
        pawn: &mut Pawn,
        pose: &Pose,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
        ticks: usize,
    ) {
        for _ in 0..ticks {
            step(ME, pawn, pose, marks, grid);
            if mining(pawn).is_some() {
                return;
            }
        }
    }

    /// A stand cell is within one column of the block and has its feet from one below to two
    /// above the block's cell.
    fn is_stand_cell_for(cell: BlockPos, block: BlockPos) -> bool {
        let d = cell - block;
        d.x.abs() <= 1 && d.z.abs() <= 1 && (-1..=2).contains(&d.y)
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = ground(8, 8);
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();

        let actions: Vec<_> = (0..5)
            .map(|_| step(ME, &mut pawn, &pose_in(at(2, 1, 2)), &mut marks, &grid))
            .collect();

        assert!(actions.iter().all(|a| *a == JobAction::Wait), "{actions:?}");
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = ground(14, 8);
        let buried = at(5, 1, 5);
        put(
            &mut grid,
            [
                buried,
                at(4, 1, 5),
                at(6, 1, 5),
                at(5, 1, 4),
                at(5, 1, 6),
                at(5, 2, 5),
            ],
        );
        let open = at(9, 0, 2);
        let mut marks = marks_on(&[buried]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 0));

        let alone = step(ME, &mut pawn, &pose, &mut marks, &grid);
        let held_alone = (pawn.job.clone(), holder(&marks, buried));
        marks.toggle(open.to_array());
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);

        assert_eq!(alone, JobAction::Wait);
        assert_eq!(held_alone, (Job::Idle, Holder::Open), "nothing to take");
        assert_eq!(
            mining(&pawn).map(|(mark, ..)| mark),
            Some(open),
            "the buried mark is skipped for the open one, though it is nearer"
        );
        assert_eq!(holder(&marks, buried), Holder::Open);
        assert_eq!(holder(&marks, open), Holder::Claimed(ME));
    }

    #[test]
    fn a_mark_with_no_way_to_it_is_skipped_for_a_reachable_one() {
        let mut grid = ground(24, 3);
        let island = at(3, 0, 6);
        let reachable = at(20, 0, 0);
        put(&mut grid, [island]);
        let mut marks = marks_on(&[island, reachable]);
        let mut pawn = Pawn::default();

        until_it_has_a_job(&mut pawn, &pose_in(at(0, 1, 0)), &mut marks, &grid, 3);

        assert_eq!(mining(&pawn).map(|(mark, ..)| mark), Some(reachable));
        assert_eq!(
            holder(&marks, island),
            Holder::Open,
            "the island stays open"
        );
    }

    #[test]
    fn at_most_four_route_attempts_are_made_in_a_tick() {
        let mut grid = ground(24, 3);
        let islands: Vec<BlockPos> = (2..=6).map(|x| at(x, 0, 6)).collect();
        let reachable = at(20, 0, 0);
        put(&mut grid, islands.iter().copied());
        let mut marks = marks_on(&islands);
        marks.toggle(reachable.to_array());
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 0));

        let first = step(ME, &mut pawn, &pose, &mut marks, &grid);

        assert_eq!(first, JobAction::Wait, "four failed searches use the tick");
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(
            holder(&marks, reachable),
            Holder::Open,
            "the fifth attempt waits for the next tick"
        );
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let grid = ground(16, 4);
        let (taken, near, far) = (at(2, 0, 1), at(4, 0, 1), at(12, 0, 1));
        let mut marks = marks_on(&[far, near, taken]);
        assert!(marks.claim(taken.to_array(), PawnId(9)));
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        assert_eq!(pawn.equipped_tool, None);
        let pose = pose_in(at(0, 1, 1));

        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);

        let (mark, route, next) = mining(&pawn).expect("the pawn took a job");
        assert_eq!(mark, near, "the nearest unclaimed mark");
        assert_eq!(next, 0);
        assert!(!route.is_empty(), "it is not yet standing by the block");
        assert_eq!(holder(&marks, near), Holder::Claimed(ME));
        assert_eq!(holder(&marks, far), Holder::Open);
        assert_eq!(
            holder(&marks, taken),
            Holder::Claimed(PawnId(9)),
            "not stolen"
        );
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    /// Runs the job, putting the pawn on each waypoint it is sent to, until it digs.
    fn walk_to_the_dig(
        pawn: &mut Pawn,
        marks: &mut MineMarks,
        grid: &VoxelGrid,
        mut pose: Pose,
    ) -> (Vec<Vec3>, (BlockPos, Vec3), Pose) {
        let mut walks = Vec::new();
        for _ in 0..200 {
            match step(ME, pawn, &pose, marks, grid) {
                JobAction::Wait => {}
                JobAction::Walk { to } => {
                    walks.push(to);
                    pose.feet = to;
                    pose.eye = to + Vec3::Y * EYE_HEIGHT;
                }
                JobAction::Dig { block, aim } => return (walks, (block, aim), pose),
            }
        }
        panic!("no dig after 200 ticks; walked {walks:?}");
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let grid = ground(12, 4);
        let block = at(6, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let start = pose_in(at(0, 1, 1));

        let (walks, (dug, _), pose) = walk_to_the_dig(&mut pawn, &mut marks, &grid, start);

        assert_eq!(dug, block, "digs the marked block");
        assert_eq!(
            walks.len(),
            5,
            "the nearest stand cell is five steps away: {walks:?}"
        );
        let mut previous = start.feet;
        for to in &walks {
            let (dx, dz) = ((to.x - previous.x).abs(), (to.z - previous.z).abs());
            assert!(
                (dx - 1.0).abs() < 1e-3 && dz < 1e-3 || (dz - 1.0).abs() < 1e-3 && dx < 1e-3,
                "{previous:?} to {to:?} is not one block along an axis"
            );
            assert_eq!(
                (to.x.fract(), to.z.fract()),
                (0.5, 0.5),
                "waypoints are cell centres"
            );
            previous = *to;
        }
        let stood_in = crate::routing::feet_cell(pose.feet);
        assert_eq!(stood_in, at(5, 1, 1), "the stand cell nearest by route");
        assert!(
            is_stand_cell_for(stood_in, block),
            "dug from {stood_in:?}, too far from {block:?}"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let block = at(5, 1, 5);
        let world = |extra: &[BlockPos]| {
            let mut grid = ground(10, 10);
            put(&mut grid, [block]);
            put(&mut grid, extra.iter().copied());
            grid
        };
        let open = world(&[]);
        let covered_on_the_west = world(&[at(4, 1, 5)]);
        let aim_in = |grid: &VoxelGrid, eye: Vec3| {
            let mut marks = marks_on(&[block]);
            assert!(marks.claim(block.to_array(), ME));
            let mut pawn = Pawn {
                job: Job::Mine {
                    mark: block,
                    route: Vec::new(),
                    next: 0,
                    stall: StallGuard::default(),
                },
                ..Pawn::default()
            };
            let pose = Pose {
                feet: eye - Vec3::Y * EYE_HEIGHT,
                eye,
                yaw: 0.0,
            };
            step(ME, &mut pawn, &pose, &mut marks, grid)
        };

        let from_the_west = aim_in(&open, Vec3::new(3.5, 2.15, 5.5));
        let from_above = aim_in(&open, Vec3::new(5.5, 6.0, 5.5));
        let from_the_south = aim_in(&open, Vec3::new(5.5, 1.5, 2.0));
        // The west face is the nearest in space but a neighbour covers it.
        let west_covered = aim_in(&covered_on_the_west, Vec3::new(3.5, 2.15, 5.5));
        // From the eye the top face is nearer; from the feet, 1.15 m lower, the south one is.
        let eye_not_feet = aim_in(&open, Vec3::new(5.5, 4.0, 3.5));

        let faces = [
            ("west", from_the_west, Vec3::new(5.0, 1.5, 5.5)),
            ("top", from_above, Vec3::new(5.5, 2.0, 5.5)),
            ("south", from_the_south, Vec3::new(5.5, 1.5, 5.0)),
            ("covered west", west_covered, Vec3::new(5.5, 2.0, 5.5)),
            ("eye, not feet", eye_not_feet, Vec3::new(5.5, 2.0, 5.5)),
        ];
        for (name, action, face) in faces {
            match action {
                JobAction::Dig { block: dug, aim } => {
                    assert_eq!(dug, block, "{name}");
                    assert!(aim.distance(face) < 1e-3, "{name}: aimed at {aim:?}");
                }
                other => panic!("{name}: expected a dig, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);
        let walking = mining(&pawn).map(|(mark, ..)| mark);
        marks.toggle(block.to_array());

        let action = step(ME, &mut pawn, &pose, &mut marks, &grid);
        let after_cancel = (pawn.job.clone(), marks.iter().count());
        marks.toggle(block.to_array());
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);

        assert_eq!(walking, Some(block));
        assert_eq!(
            action,
            JobAction::Wait,
            "it does not walk on to a cancelled mark"
        );
        assert_eq!(after_cancel, (Job::Idle, 0), "free, and nothing is marked");
        assert_eq!(
            mining(&pawn).map(|(mark, ..)| mark),
            Some(block),
            "the freed pawn takes the same block again when it is re-marked"
        );
        assert_eq!(holder(&marks, block), Holder::Claimed(ME));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);
        grid.mutator().remove(block);
        marks.block_removed(block.to_array());

        let action = step(ME, &mut pawn, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait, "nothing left to dig");
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(marks.iter().count(), 0);
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let grid = ground(12, 3);
        let block = at(6, 0, 1);
        let mut marks = marks_on(&[block]);
        let (mut first, mut second) = (Pawn::default(), Pawn::default());
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut first, &pose, &mut marks, &grid, 3);

        let actions: Vec<_> = (0..3)
            .map(|_| step(PawnId(1), &mut second, &pose, &mut marks, &grid))
            .collect();

        assert!(actions.iter().all(|a| *a == JobAction::Wait), "{actions:?}");
        assert_eq!(second.job, Job::Idle);
        assert_eq!(
            holder(&marks, block),
            Holder::Claimed(ME),
            "the first keeps it"
        );
        assert_eq!(mining(&first).map(|(mark, ..)| mark), Some(block));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        let mut released_at = None;
        let mut last = JobAction::Dig {
            block,
            aim: Vec3::ZERO,
        };

        for tick in 1..=STALL_TICKS + 10 {
            last = step(ME, &mut pawn, &pose, &mut marks, &grid);
            if pawn.job == Job::Idle {
                released_at = Some(tick);
                break;
            }
        }

        let released_at = released_at.expect("a pawn that never moves gives up");
        assert!(
            (STALL_TICKS - 1..=STALL_TICKS + 2).contains(&released_at),
            "gave up after {released_at} ticks, not about {STALL_TICKS}"
        );
        assert_eq!(last, JobAction::Wait);
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert_eq!(holder(&marks, block), Holder::Open, "but is open again");
    }

    #[test]
    fn a_pawn_that_keeps_making_progress_is_never_stalled() {
        let grid = ground(24, 3);
        let block = at(20, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let mut pose = pose_in(at(0, 1, 1));
        let mut ticks = 0;

        let dug = loop {
            ticks += 1;
            assert!(ticks < 1500, "never arrived");
            match step(ME, &mut pawn, &pose, &mut marks, &grid) {
                JobAction::Dig { block, .. } => break block,
                JobAction::Walk { to } => {
                    let toward = Vec3::new(to.x - pose.feet.x, 0.0, to.z - pose.feet.z);
                    let moved = toward.clamp_length_max(0.05);
                    pose.feet += moved;
                    pose.feet.y = to.y;
                    pose.eye = pose.feet + Vec3::Y * EYE_HEIGHT;
                }
                JobAction::Wait => {}
            }
            assert!(
                mining(&pawn).is_some() || ticks == 1,
                "gave up at tick {ticks} though it kept moving"
            );
        };

        assert_eq!(dug, block);
        assert!(ticks > STALL_TICKS * 2, "the walk outlasts the stall limit");
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);
        let (_, route, next) = mining(&pawn).expect("the pawn took a job");
        put(&mut grid, [route[next]]);

        let action = step(ME, &mut pawn, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains(block.to_array()), "the mark stays");
        assert_eq!(holder(&marks, block), Holder::Open);
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in 2i32..=12,
            bz in 2i32..=12,
            height in 0i32..=2,
            sx in 0i32..=14,
            sz in 0i32..=14,
        ) {
            prop_assume!((sx, sz) != (bx, bz));
            let mut grid = ground(16, 16);
            put(&mut grid, (1..=height).map(|y| at(bx, y, bz)));
            let block = at(bx, height, bz);
            let mut marks = marks_on(&[block]);
            let mut pawn = Pawn::default();
            let pose = pose_in(at(sx, 1, sz));

            until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);

            let (mark, route, _) = mining(&pawn).expect("a reachable mark is taken");
            prop_assert_eq!(mark, block);
            let stand = route.last().copied().unwrap_or(at(sx, 1, sz));
            prop_assert!(is_stand_cell_for(stand, block), "{stand:?} for {block:?}");
            let eye = Vec3::new(stand.x as f32 + 0.5, stand.y as f32 + EYE_HEIGHT, stand.z as f32 + 0.5);
            let centre = Vec3::new(bx as f32 + 0.5, height as f32 + 0.5, bz as f32 + 0.5);
            prop_assert!(
                eye.distance(centre) <= MINE_REACH,
                "the eye at {eye:?} is {} from {centre:?}",
                eye.distance(centre)
            );
        }
    }

    fn facing(yaw: f32) -> Pose {
        Pose {
            yaw,
            ..pose_in(at(0, 1, 0))
        }
    }

    /// A point `distance` metres from the feet along the bearing `bearing` radians from +Z
    /// toward +X.
    fn along(bearing: f32, distance: f32) -> Vec3 {
        Vec3::new(
            0.5 + bearing.sin() * distance,
            1.0,
            0.5 + bearing.cos() * distance,
        )
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let per_tick = TURN_RATE / TICK_HZ as f32;
        let pi = std::f32::consts::PI;

        let right = steer(&facing(0.0), along(pi / 2.0, 5.0));
        let left = steer(&facing(0.0), along(-pi / 2.0, 5.0));
        let behind_left = steer(&facing(0.0), along(-0.99 * pi, 5.0));
        let nearly_there = steer(&facing(0.0), along(0.01, 5.0));
        let across_the_seam = steer(&facing(3.0), along(-3.0, 5.0));
        let on_target = steer(&facing(0.0), along(0.0, 5.0));

        assert!((right.yaw_delta - per_tick).abs() < 1e-4, "{right:?}");
        assert!((left.yaw_delta + per_tick).abs() < 1e-4, "{left:?}");
        assert!(behind_left.yaw_delta < 0.0, "{behind_left:?}");
        assert!(
            (nearly_there.yaw_delta - 0.01).abs() < 1e-4,
            "a small error is closed exactly, not overshot: {nearly_there:?}"
        );
        let wrapped = 2.0 * pi - 6.0;
        assert!(
            (across_the_seam.yaw_delta - wrapped.min(per_tick)).abs() < 1e-4,
            "takes the short way round: {across_the_seam:?}"
        );
        assert!(on_target.yaw_delta.abs() < 1e-5, "{on_target:?}");
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let pi = std::f32::consts::PI;
        let degrees = |d: f32| d.to_radians();

        let ahead = steer(&facing(0.0), along(0.0, 5.0));
        let off_a_little = steer(&facing(0.0), along(degrees(50.0), 5.0));
        let off_the_other_way = steer(&facing(0.0), along(degrees(-50.0), 5.0));
        let off_a_lot = steer(&facing(0.0), along(degrees(70.0), 5.0));
        let behind = steer(&facing(0.0), along(pi, 5.0));
        let facing_it_after_turning = steer(&facing(degrees(100.0)), along(degrees(120.0), 5.0));

        assert_eq!(ahead.forward, 1.0, "full speed straight at it");
        assert_eq!(off_a_little.forward, 1.0, "within 60 degrees");
        assert_eq!(off_the_other_way.forward, 1.0, "either side");
        assert_eq!(off_a_lot.forward, 0.0, "turn in place first");
        assert_eq!(behind.forward, 0.0);
        assert_eq!(
            facing_it_after_turning.forward, 1.0,
            "yaw is the pawn's, not the world's"
        );
    }

    #[test]
    fn steer_toward_where_the_pawn_stands_does_not_produce_nan() {
        let pose = facing(0.4);

        let walk = steer(&pose, pose.feet);

        assert!(
            walk.yaw_delta.is_finite() && walk.forward.is_finite(),
            "{walk:?}"
        );
        assert_eq!(walk.yaw_delta, 0.0);
    }

    #[test]
    fn a_pawn_whose_mark_was_cancelled_and_marked_again_loses_its_claim() {
        let grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);
        marks.toggle(block.to_array());
        marks.toggle(block.to_array());

        let action = step(ME, &mut pawn, &pose, &mut marks, &grid);
        let after = (pawn.job.clone(), holder(&marks, block));
        let second = {
            let mut other = Pawn::default();
            until_it_has_a_job(&mut other, &pose, &mut marks, &grid, 3);
            mining(&other).map(|(mark, ..)| mark)
        };

        assert_eq!(
            action,
            JobAction::Wait,
            "it does not walk on to a stale order"
        );
        assert_eq!(after, (Job::Idle, Holder::Open));
        assert_eq!(second, Some(block), "another pawn can take the new mark");
    }

    #[test]
    fn a_pawn_that_lost_its_claim_does_not_release_the_new_holders() {
        let grid = ground(12, 3);
        let block = at(8, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        let mut other = Pawn::default();
        let pose = pose_in(at(0, 1, 1));
        until_it_has_a_job(&mut pawn, &pose, &mut marks, &grid, 3);
        marks.toggle(block.to_array());
        marks.toggle(block.to_array());
        until_it_has_a_job(&mut other, &pose, &mut marks, &grid, 3);
        assert_eq!(holder(&marks, block), Holder::Claimed(ME));

        let action = step(PawnId(1), &mut pawn, &pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(
            holder(&marks, block),
            Holder::Claimed(ME),
            "the holder keeps it"
        );
    }

    #[test]
    fn steer_turns_the_short_way_across_the_yaw_seam() {
        let gap = std::f32::consts::TAU - 6.2;

        let positive = steer(&facing(3.1), along(-3.1, 5.0));
        let negative = steer(&facing(-3.1), along(3.1, 5.0));

        assert!((positive.yaw_delta - gap).abs() < 1e-3, "{positive:?}");
        assert!((negative.yaw_delta + gap).abs() < 1e-3, "{negative:?}");
        assert_eq!(positive.forward, 1.0, "the target is almost dead ahead");
        assert_eq!(negative.forward, 1.0);
    }

    #[test]
    fn a_pawn_already_on_a_stand_cell_digs_in_the_claiming_call() {
        let grid = ground(12, 4);
        let block = at(6, 0, 1);
        let mut marks = marks_on(&[block]);
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        let pose = pose_in(at(5, 1, 1));

        let action = step(ME, &mut pawn, &pose, &mut marks, &grid);
        let claimed = (mining(&pawn), holder(&marks, block));
        finish(&mut pawn, &mut marks);

        assert!(
            matches!(action, JobAction::Dig { block: dug, .. } if dug == block),
            "{action:?}"
        );
        assert_eq!(claimed, (Some((block, Vec::new(), 0)), Holder::Claimed(ME)));
        assert_eq!(pawn.equipped_tool, Some(crate::tools::STARTING_TOOL));
        assert_eq!(pawn.job, Job::Idle, "finish ends the job");
        assert_eq!(
            holder(&marks, block),
            Holder::Open,
            "and frees a mark that is left"
        );
    }

    #[test]
    fn a_walk_that_keeps_closing_in_never_stalls_however_long_it_takes() {
        let mut guard = StallGuard::default();

        let ever_stalled =
            (0..400u16).any(|tick| guard.is_stalled_at(100.0 - 0.02 * f32::from(tick)));

        assert!(!ever_stalled, "{guard:?}");
        assert_eq!(guard.ticks_without_progress, 0);
    }

    #[test]
    fn progress_is_a_whole_step_closer_than_the_best_so_far() {
        let boundary = 1.0_f32 - PROGRESS_STEP;
        let mut guard = StallGuard::default();
        guard.is_stalled_at(1.0);

        guard.is_stalled_at(boundary);
        let at_the_step = (guard.ticks_without_progress, guard.best_distance);
        guard.is_stalled_at(boundary - 0.001);
        let past_the_step = (guard.ticks_without_progress, guard.best_distance);

        assert_eq!(
            at_the_step,
            (1, Some(1.0)),
            "exactly one step is not progress"
        );
        assert_eq!(
            past_the_step,
            (0, Some(boundary - 0.001)),
            "more than one is"
        );
    }

    #[test]
    fn a_walk_that_stops_closing_in_stalls_after_the_limit() {
        let mut guard = StallGuard::default();
        guard.is_stalled_at(5.0);

        let stalled_at = (1..=STALL_TICKS + 5).find(|_| guard.is_stalled_at(5.0));

        assert_eq!(stalled_at, Some(STALL_TICKS));
    }

    #[test]
    fn steer_has_a_direction_from_the_smallest_distance_it_can_resolve() {
        let pose = facing(0.0);
        let resolved = 1.0 / 32768.0;

        let at_the_limit = steer(&pose, pose.feet + Vec3::X * resolved);
        let inside = steer(&pose, pose.feet + Vec3::X * (resolved / 2.0));

        assert!(at_the_limit.yaw_delta > 0.0, "{at_the_limit:?}");
        assert_eq!(inside.yaw_delta, 0.0, "{inside:?}");
    }

    #[test]
    fn the_nearest_mark_is_nearest_to_where_the_pawn_stands() {
        let grid = ground(24, 3);
        let (near, far) = (at(12, 0, 1), at(0, 0, 1));
        let mut marks = marks_on(&[far, near]);
        let mut pawn = Pawn::default();

        until_it_has_a_job(&mut pawn, &pose_in(at(10, 1, 1)), &mut marks, &grid, 3);

        assert_eq!(mining(&pawn).map(|(mark, ..)| mark), Some(near));
    }
}
