//! What an unpossessed pawn does on its own: take a marked block, walk to it, dig it.
//!
//! Everything here is a pure function of the pawn, its pose, the marks and the grid. The
//! caller applies the resulting [`JobAction`] through the pawn's own physics step and
//! [`Pawn::dig`](crate::pawn::Pawn::dig), the same ones a possessed pawn uses.

use std::f32::consts::{FRAC_PI_3, PI, TAU};

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::{Vec2, Vec3};

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};
use crate::selection::PAWN_HALF_EXTENTS;

/// How close, horizontally, the feet must come to a waypoint's centre to have reached it.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without getting closer to the current waypoint before the pawn gives the mark up.
pub const STALL_TICKS: u32 = 120;

/// Fastest turn towards a target, in radians per second: one full turn a second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Route searches an idle pawn may spend in one tick.
pub const ROUTE_ATTEMPTS_PER_TICK: usize = 4;

/// Where a pawn stands and which way it faces; yaw 0 faces +Z and turns towards +X.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// The point on the ground under the capsule's centre.
    pub feet: Vec3,
    pub yaw: f32,
}

/// Counts the ticks a walking pawn has made no progress towards its waypoint.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StallGuard {
    pub still_ticks: u32,
    /// The smallest distance to the waypoint seen so far.
    pub closest: f32,
}

/// A pawn's current occupation.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Job {
    #[default]
    Idle,
    /// Walking `route` (the cells after the pawn's own, ending at the stand cell) to dig the
    /// marked block `mark`; `next` indexes the waypoint being walked to.
    Mine {
        mark: BlockPos,
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// What the pawn does this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk towards the world point `to`, the centre of a waypoint cell at its feet height.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at the world point `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// One tick of walking: turn by `yaw_delta` radians, and move along the facing at `forward`
/// (`1.0` full speed, `0.0` stand still).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Whether a pawn with its feet in `cell` can dig the block `block` from there: within one
/// column of it in x and z, and with the feet from one below to two above the block.
pub fn is_stand_cell(block: BlockPos, cell: BlockPos) -> bool {
    let offset = cell - block;
    offset.x.abs() <= 1 && offset.z.abs() <= 1 && (-1..=2).contains(&offset.y)
}

/// Advances `pawn`'s job one tick and returns what to do about it.
///
/// An idle pawn takes the nearest unclaimed, exposed mark it can route to (claiming it for
/// `id` and equipping the tool) and starts walking in the same call. Releasing a claim on
/// a vanished mark, a waypoint that is no longer standable or a stalled walk sends the pawn
/// back to idle and returns [`JobAction::Wait`]; the mark stays. The tick that returns a
/// [`JobAction::Dig`] also releases the claim and ends the job.
pub fn step(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if pawn.job == Job::Idle && !take_a_mark(id, pawn, pose, marks, grid) {
        return JobAction::Wait;
    }
    work(pawn, pose, marks, grid)
}

fn centre_of(cell: BlockPos) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

fn has_exposed_face(grid: &VoxelGrid, block: BlockPos) -> bool {
    FACES.iter().any(|&face| !grid.is_solid_at(block + face))
}

const FACES: [BlockPos; 6] = [
    BlockPos::X,
    BlockPos::NEG_X,
    BlockPos::Y,
    BlockPos::NEG_Y,
    BlockPos::Z,
    BlockPos::NEG_Z,
];

/// Claims the nearest unclaimed, exposed mark that routes to a stand cell and starts the
/// job; `false` when none was taken this tick.
fn take_a_mark(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> bool {
    let from = feet_cell(pose.feet);
    let mut candidates: Vec<BlockPos> = marks
        .unclaimed()
        .map(BlockPos::from_array)
        .filter(|&block| has_exposed_face(grid, block))
        .collect();
    candidates.sort_by_key(|&block| {
        let offset = (block - from).as_i64vec3();
        offset.length_squared()
    });

    let first = if pawn.search_cursor < candidates.len() {
        pawn.search_cursor
    } else {
        0
    };
    pawn.search_cursor = 0;
    for (attempts, (index, &block)) in candidates.iter().enumerate().skip(first).enumerate() {
        if attempts == ROUTE_ATTEMPTS_PER_TICK {
            pawn.search_cursor = index;
            return false;
        }
        let found = route(
            grid,
            from,
            |cell| is_stand_cell(block, cell),
            block,
            ROUTE_NODE_BUDGET,
        );
        if let Some(route) = found
            && marks.claim(block.to_array(), id)
        {
            pawn.select_slot(TOOL_SLOT);
            pawn.job = Job::Mine {
                mark: block,
                route,
                next: 0,
                stall: StallGuard::fresh(),
            };
            return true;
        }
    }
    false
}

/// What walking the current job's route comes to this tick.
enum Progress {
    Walk(Vec3),
    Dig(Vec3),
    GiveUp,
}

/// Runs the pawn's `Mine` job one tick, ending it when it digs or gives up.
fn work(pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = &mut pawn.job
    else {
        return JobAction::Wait;
    };
    let block = *mark;
    let progress = progress(block, route, next, stall, pose, marks, grid);

    match progress {
        Progress::Walk(to) => JobAction::Walk { to },
        Progress::Dig(aim) => {
            marks.release(block.to_array());
            pawn.job = Job::Idle;
            JobAction::Dig { block, aim }
        }
        Progress::GiveUp => {
            marks.release(block.to_array());
            pawn.job = Job::Idle;
            JobAction::Wait
        }
    }
}

fn progress(
    block: BlockPos,
    route: &[BlockPos],
    next: &mut usize,
    stall: &mut StallGuard,
    pose: Pose,
    marks: &MineMarks,
    grid: &VoxelGrid,
) -> Progress {
    if !marks.contains(block.to_array()) {
        return Progress::GiveUp;
    }
    while route
        .get(*next)
        .is_some_and(|&cell| horizontal_distance(pose.feet, centre_of(cell)) <= ARRIVE_RADIUS)
    {
        *next += 1;
        *stall = StallGuard::fresh();
    }
    let Some(&cell) = route.get(*next) else {
        return nearest_exposed_face(grid, block, pose).map_or(Progress::GiveUp, Progress::Dig);
    };
    if !standable(grid, cell) {
        return Progress::GiveUp;
    }
    let to = centre_of(cell);
    let distance = horizontal_distance(pose.feet, to);
    if distance < stall.closest {
        stall.closest = distance;
        stall.still_ticks = 0;
    } else {
        stall.still_ticks += 1;
        if stall.still_ticks >= STALL_TICKS {
            return Progress::GiveUp;
        }
    }
    Progress::Walk(to)
}

/// The centre of the exposed face of `block` nearest the pawn's eye.
fn nearest_exposed_face(grid: &VoxelGrid, block: BlockPos, pose: Pose) -> Option<Vec3> {
    let eye = pose.feet + Vec3::Y * PAWN_HALF_EXTENTS.y;
    let middle = block.as_vec3() + Vec3::splat(0.5);
    FACES
        .iter()
        .filter(|&&face| !grid.is_solid_at(block + face))
        .map(|&face| middle + face.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

impl StallGuard {
    /// A guard for a waypoint not yet approached.
    fn fresh() -> Self {
        Self {
            still_ticks: 0,
            closest: f32::INFINITY,
        }
    }
}

/// The turn and walk that take a pawn at `pose` towards the world point `to`: turn by the
/// shortest way at no more than [`TURN_RATE`] (per tick), and walk only once the target is
/// within 60 degrees of the facing, turning in place until then.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let bearing = (to.x - pose.feet.x).atan2(to.z - pose.feet.z);
    let error = (bearing - pose.yaw + PI).rem_euclid(TAU) - PI;
    let max_turn = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: error.clamp(-max_turn, max_turn),
        forward: if error.abs() <= WALK_CONE { 1.0 } else { 0.0 },
    }
}

/// The widest angle between facing and target at which a pawn walks rather than turns in
/// place: 60 degrees.
const WALK_CONE: f32 = FRAC_PI_3;

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use super::*;
    use crate::TICK_HZ;
    use crate::pawn::MINE_REACH;
    use crate::routing::{feet_cell, standable};
    use crate::selection::PAWN_HALF_EXTENTS;
    use crate::tools::STARTING_TOOL;

    const ME: PawnId = PawnId(0);

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    /// A 16 by 16 floor of blocks at y = 0, so feet cells are y = 1.
    fn floor() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in 0..16 {
            for z in 0..16 {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn marked(blocks: &[[i32; 3]]) -> MineMarks {
        let mut marks = MineMarks::default();
        for &block in blocks {
            marks.toggle(block);
        }
        marks
    }

    fn pose_in(x: i32, y: i32, z: i32) -> Pose {
        Pose {
            feet: Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
            yaw: 0.0,
        }
    }

    fn centre_of(x: i32, y: i32, z: i32) -> Vec3 {
        Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5)
    }

    /// Where the capsule centre, and so the eye, is for a pawn with these feet.
    fn eye_of(pose: Pose) -> Vec3 {
        pose.feet + Vec3::Y * PAWN_HALF_EXTENTS.y
    }

    fn claimed_by(marks: &MineMarks, block: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(pos, _)| *pos == block)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn mining(pawn: &Pawn, block: [i32; 3]) -> bool {
        matches!(&pawn.job, Job::Mine { mark, .. } if mark.to_array() == block)
    }

    /// What a pawn did walking to a block, teleporting to each waypoint it is told to walk to.
    struct Trip {
        walks: Vec<Vec3>,
        dig: Option<(BlockPos, Vec3)>,
        pose: Pose,
    }

    fn trip(pawn: &mut Pawn, marks: &mut MineMarks, grid: &VoxelGrid, mut pose: Pose) -> Trip {
        let mut walks = Vec::new();
        for _ in 0..64 {
            match step(ME, pawn, pose, marks, grid) {
                JobAction::Walk { to } => {
                    walks.push(to);
                    pose.feet = to;
                }
                JobAction::Dig { block, aim } => {
                    return Trip {
                        walks,
                        dig: Some((block, aim)),
                        pose,
                    };
                }
                JobAction::Wait => break,
            }
        }
        Trip {
            walks,
            dig: None,
            pose,
        }
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = floor();
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let action = step(ME, &mut pawn, pose_in(3, 1, 3), &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert_eq!(pawn.equipped_tool, None, "nothing to dig, nothing equipped");
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = floor();
        for x in 0..16 {
            for z in 0..16 {
                place(&mut grid, x, -1, z);
            }
        }
        place(&mut grid, 8, 1, 8);
        place(&mut grid, 14, 1, 8);
        // The buried mark's six neighbours are solid, yet (7, 1, 8) beside it is standable.
        let mut marks = marked(&[[8, 0, 8], [14, 1, 8]]);
        let mut pawn = Pawn::default();

        let action = step(ME, &mut pawn, pose_in(0, 1, 8), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(
            claimed_by(&marks, [8, 0, 8]),
            None,
            "buried in solid blocks"
        );
        assert_eq!(claimed_by(&marks, [14, 1, 8]), Some(ME));
        assert!(mining(&pawn, [14, 1, 8]));
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = floor();
        place(&mut grid, 3, 4, 8);
        place(&mut grid, 7, 1, 8);
        place(&mut grid, 0, 1, 12);
        // Nearest is out of anyone's reach (nothing to stand on); the farthest sorts first.
        let mut marks = marked(&[[3, 4, 8], [7, 1, 8], [0, 1, 12]]);
        let mut pawn = Pawn::default();
        pawn.select_slot(1);

        let action = step(ME, &mut pawn, pose_in(3, 1, 8), &mut marks, &grid);

        assert_eq!(
            action,
            JobAction::Walk {
                to: centre_of(4, 1, 8)
            }
        );
        assert_eq!(claimed_by(&marks, [7, 1, 8]), Some(ME));
        assert_eq!(
            claimed_by(&marks, [3, 4, 8]),
            None,
            "no stand cell reaches it"
        );
        assert_eq!(
            claimed_by(&marks, [0, 1, 12]),
            None,
            "farther than the nearest"
        );
        assert!(mining(&pawn, [7, 1, 8]));
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        let mut marks = marked(&[[8, 1, 8]]);
        let mut pawn = Pawn::default();

        let trip = trip(&mut pawn, &mut marks, &grid, pose_in(0, 1, 8));

        let expected: Vec<Vec3> = (1..=7).map(|x| centre_of(x, 1, 8)).collect();
        assert_eq!(trip.walks, expected);
        let (block, _) = trip.dig.expect("the pawn digs at the end of the walk");
        assert_eq!(block, BlockPos::new(8, 1, 8));
        let stand = feet_cell(trip.pose.feet);
        assert_eq!(stand, BlockPos::new(7, 1, 8));
        assert!(is_stand_cell(block, stand));
        assert_eq!(pawn.job, Job::Idle, "the dig ends the job");
        assert_eq!(claimed_by(&marks, [8, 1, 8]), None, "the claim is released");
        assert!(
            marks.contains([8, 1, 8]),
            "the mark stays until the block goes"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let mut open = floor();
        place(&mut open, 8, 1, 8);
        let mut pillar = floor();
        for (x, z) in [(8, 8), (7, 8), (9, 8), (8, 7), (8, 9)] {
            place(&mut pillar, x, 1, z);
        }
        // Dug from the diagonal cell (7, 1, 7), the west and south faces are nearest the eye
        // but covered by blocks; of the exposed ones the top is nearest.
        let mut shaded = floor();
        for (x, z) in [(8, 8), (7, 8), (8, 7)] {
            place(&mut shaded, x, 1, z);
        }
        // From the east the block's east face is nearest; boxed in, only the top shows.
        let cases = [
            (open, pose_in(15, 1, 8), Vec3::new(9.0, 1.5, 8.5)),
            (pillar, pose_in(0, 1, 8), Vec3::new(8.5, 2.0, 8.5)),
            (shaded, pose_in(6, 1, 7), Vec3::new(8.5, 2.0, 8.5)),
        ];

        for (grid, start, expected) in cases {
            let mut marks = marked(&[[8, 1, 8]]);
            let mut pawn = Pawn::default();

            let trip = trip(&mut pawn, &mut marks, &grid, start);

            let (block, aim) = trip.dig.expect("the pawn digs");
            assert_eq!(block, BlockPos::new(8, 1, 8));
            assert!(
                (aim - expected).length() < 1e-4,
                "aim {aim:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        let mut marks = marked(&[[8, 1, 8]]);
        let mut pawn = Pawn::default();
        let mut pose = pose_in(0, 1, 8);
        for _ in 0..3 {
            let JobAction::Walk { to } = step(ME, &mut pawn, pose, &mut marks, &grid) else {
                panic!("the pawn should be walking to the mark");
            };
            pose.feet = to;
        }

        marks.toggle([8, 1, 8]);
        let after = step(ME, &mut pawn, pose, &mut marks, &grid);
        let later = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(after, JobAction::Wait);
        assert_eq!(later, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        place(&mut grid, 12, 1, 3);
        let mut marks = marked(&[[8, 1, 8], [12, 1, 3]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(0, 1, 8);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        assert!(mining(&pawn, [8, 1, 8]));

        grid.mutator().remove(BlockPos::new(8, 1, 8));
        marks.block_removed([8, 1, 8]);
        let released = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(released, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);

        let next = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert!(matches!(next, JobAction::Walk { .. }), "{next:?}");
        assert!(mining(&pawn, [12, 1, 3]), "free to take the next mark");
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let mut grid = floor();
        place(&mut grid, 6, 1, 8);
        place(&mut grid, 12, 1, 8);
        let mut marks = marked(&[[6, 1, 8], [12, 1, 8]]);
        assert!(marks.claim([6, 1, 8], PawnId(7)));
        let mut pawn = Pawn::default();

        let action = step(ME, &mut pawn, pose_in(0, 1, 8), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(
            claimed_by(&marks, [6, 1, 8]),
            Some(PawnId(7)),
            "still theirs"
        );
        assert_eq!(claimed_by(&marks, [12, 1, 8]), Some(ME));
        assert!(mining(&pawn, [12, 1, 8]));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        let mut marks = marked(&[[8, 1, 8]]);
        let mut pawn = Pawn::default();
        let mut pose = pose_in(0, 1, 8);
        step(ME, &mut pawn, pose, &mut marks, &grid);
        for _ in 0..100 {
            let action = step(ME, &mut pawn, pose, &mut marks, &grid);
            assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        }

        pose.feet.x += 0.4;
        let progress = step(ME, &mut pawn, pose, &mut marks, &grid);
        assert!(matches!(progress, JobAction::Walk { .. }), "{progress:?}");
        for tick in 1..STALL_TICKS {
            let action = step(ME, &mut pawn, pose, &mut marks, &grid);
            assert!(
                matches!(action, JobAction::Walk { .. }),
                "{tick} ticks after the last progress is too soon to give up: {action:?}"
            );
        }
        let released_at =
            (0..2).find(|_| step(ME, &mut pawn, pose, &mut marks, &grid) == JobAction::Wait);

        assert!(
            released_at.is_some(),
            "no progress for {STALL_TICKS} ticks gives up"
        );
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains([8, 1, 8]), "the mark stays");
        assert_eq!(claimed_by(&marks, [8, 1, 8]), None);
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        let mut marks = marked(&[[8, 1, 8]]);
        let mut pawn = Pawn::default();
        let pose = pose_in(0, 1, 8);
        let first = step(ME, &mut pawn, pose, &mut marks, &grid);
        assert_eq!(
            first,
            JobAction::Walk {
                to: centre_of(1, 1, 8)
            }
        );

        place(&mut grid, 1, 1, 8);
        assert!(!standable(&grid, BlockPos::new(1, 1, 8)));
        let action = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(action, JobAction::Wait);
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.contains([8, 1, 8]), "the mark stays");
        assert_eq!(claimed_by(&marks, [8, 1, 8]), None);
    }

    #[test]
    fn the_next_tick_tries_the_marks_left_over_from_the_attempt_limit() {
        let mut grid = floor();
        // Floating blocks: exposed, but nothing within reach to stand on.
        let unreachable = [[3, 6, 6], [3, 6, 7], [3, 6, 8], [3, 6, 9], [3, 6, 10]];
        assert!(unreachable.len() > ROUTE_ATTEMPTS_PER_TICK);
        for [x, y, z] in unreachable {
            place(&mut grid, x, y, z);
        }
        place(&mut grid, 14, 1, 8);
        let mut marks = marked(&unreachable);
        marks.toggle([14, 1, 8]);
        let mut pawn = Pawn::default();
        let pose = pose_in(0, 1, 8);

        let first = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert_eq!(first, JobAction::Wait, "four failed searches use the tick");
        assert_eq!(pawn.job, Job::Idle);
        assert!(marks.iter().all(|(_, mark)| mark.claimed_by.is_none()));

        let second = step(ME, &mut pawn, pose, &mut marks, &grid);

        assert!(
            matches!(second, JobAction::Walk { .. }),
            "the next tick goes on with the untried marks: {second:?}"
        );
        assert_eq!(claimed_by(&marks, [14, 1, 8]), Some(ME));
        assert!(mining(&pawn, [14, 1, 8]));
    }

    #[test]
    fn arrival_is_horizontal_within_the_arrive_radius() {
        let mut grid = floor();
        place(&mut grid, 8, 1, 8);
        let first_waypoint = centre_of(1, 1, 8);
        let second_waypoint = centre_of(2, 1, 8);
        // (how far off the waypoint centre along x, how far above it, the waypoint walked to)
        let cases = [
            (
                0.3,
                0.6,
                second_waypoint,
                "close horizontally: passed, height is ignored",
            ),
            (
                0.4,
                0.0,
                first_waypoint,
                "outside the arrive radius: not passed",
            ),
        ];

        for (along, above, expected, why) in cases {
            let mut marks = marked(&[[8, 1, 8]]);
            let mut pawn = Pawn::default();
            let mut pose = pose_in(0, 1, 8);
            let claim = step(ME, &mut pawn, pose, &mut marks, &grid);
            assert_eq!(claim, JobAction::Walk { to: first_waypoint });
            pose.feet = first_waypoint + Vec3::new(along, above, 0.0);

            let action = step(ME, &mut pawn, pose, &mut marks, &grid);

            assert_eq!(action, JobAction::Walk { to: expected }, "{why}");
        }
    }

    #[test]
    fn equidistant_marks_are_taken_in_key_order() {
        let mut grid = floor();
        place(&mut grid, 6, 1, 8);
        place(&mut grid, 10, 1, 8);
        let mut marks = marked(&[[10, 1, 8], [6, 1, 8]]);
        let mut pawn = Pawn::default();

        let action = step(ME, &mut pawn, pose_in(8, 1, 8), &mut marks, &grid);

        assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        assert_eq!(claimed_by(&marks, [6, 1, 8]), Some(ME), "the lower key");
        assert_eq!(claimed_by(&marks, [10, 1, 8]), None);
    }

    fn per_tick_turn() -> f32 {
        TURN_RATE / TICK_HZ as f32
    }

    fn facing(yaw: f32, target_yaw: f32) -> (Pose, Vec3) {
        let pose = Pose {
            feet: Vec3::new(2.0, 1.0, 2.0),
            yaw,
        };
        let to = pose.feet + Vec3::new(target_yaw.sin(), 0.0, target_yaw.cos()) * 5.0;
        (pose, to)
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let max = per_tick_turn();
        let cases = [
            (0.0, FRAC_PI_2, max, "a quarter turn right is clamped"),
            (0.0, -FRAC_PI_2, -max, "a quarter turn left is clamped"),
            (0.0, 0.02, 0.02, "a small error is closed exactly"),
            (
                0.0,
                -0.03,
                -0.03,
                "a small error to the left is closed exactly",
            ),
            (
                3.0,
                -3.0,
                max,
                "turning across the wrap takes the short way",
            ),
            (
                -3.0,
                3.0,
                -max,
                "turning back across the wrap takes the short way",
            ),
            (1.0, 1.0, 0.0, "facing the target needs no turn"),
        ];

        for (yaw, target, expected, why) in cases {
            let (pose, to) = facing(yaw, target);

            let step = steer(pose, to);

            assert!(
                (step.yaw_delta - expected).abs() < 1e-4,
                "{why}: {}",
                step.yaw_delta
            );
        }
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let cases = [
            (0.0, 0.0, 1.0),
            (0.0, 0.5, 1.0),
            (0.0, -0.7, 1.0),
            (0.0, 0.9, 1.0),
            (0.0, 1.2, 0.0),
            (1.0, 1.5, 1.0),
            (0.0, FRAC_PI_2, 0.0),
            (0.0, -FRAC_PI_2, 0.0),
            (0.0, 2.1, 0.0),
            (0.0, PI - 0.1, 0.0),
            (3.0, -3.0, 1.0),
        ];

        for (yaw, target, forward) in cases {
            let (pose, to) = facing(yaw, target);

            let step = steer(pose, to);

            assert_eq!(step.forward, forward, "yaw {yaw}, target bearing {target}");
        }
    }

    #[test]
    fn every_stand_cell_is_within_the_mining_reach() {
        let blocks = [
            BlockPos::new(0, 0, 0),
            BlockPos::new(8, 70, 8),
            BlockPos::new(-5, -3, -7),
            BlockPos::new(-1, -1, -1),
            BlockPos::new(13, 4, -2),
        ];

        for block in blocks {
            for dx in -3..=3 {
                for dy in -3..=4 {
                    for dz in -3..=3 {
                        let cell = block + BlockPos::new(dx, dy, dz);

                        let stands = is_stand_cell(block, cell);

                        let in_box = dx.abs() <= 1 && dz.abs() <= 1 && (-1..=2).contains(&dy);
                        assert_eq!(stands, in_box, "block {block}, offset ({dx}, {dy}, {dz})");
                        if stands {
                            let eye = eye_of(Pose {
                                feet: centre_of(cell.x, cell.y, cell.z),
                                yaw: 0.0,
                            });
                            let farthest = (0..8)
                                .map(|corner| {
                                    block.as_vec3()
                                        + Vec3::new(
                                            (corner & 1) as f32,
                                            ((corner >> 1) & 1) as f32,
                                            ((corner >> 2) & 1) as f32,
                                        )
                                })
                                .map(|point| (point - eye).length())
                                .fold(0.0, f32::max);
                            assert!(
                                farthest <= MINE_REACH,
                                "{farthest} m from the eye to the far corner, block {block}, cell {cell}"
                            );
                        }
                    }
                }
            }
        }
    }
}
