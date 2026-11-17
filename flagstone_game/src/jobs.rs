//! The job loop of an idle pawn: take a marked block, walk to it, dig it.

use flagstone_voxel::{BlockPos, VoxelGrid};
use glam::Vec3;

use crate::TICK_HZ;
use crate::colony::MineMarks;
use crate::pawn::{Pawn, PawnId, TOOL_SLOT};
use crate::routing::{ROUTE_NODE_BUDGET, feet_cell, route, standable};

/// Marks an idle pawn tries to route to in one tick; the rest wait for the next.
const ROUTE_ATTEMPTS: usize = 4;

/// Height of a pawn's eye above its feet, in metres.
const EYE_HEIGHT: f32 = 1.15;

/// How far off its heading a pawn may be and still walk, in radians (60 degrees).
const WALK_CONE: f32 = std::f32::consts::FRAC_PI_3;

/// How close, horizontally, the feet must be to a waypoint's centre to count as there, in metres.
pub const ARRIVE_RADIUS: f32 = 0.35;

/// Ticks without progress toward a waypoint before the pawn gives up its claim.
pub const STALL_TICKS: u32 = 120;

/// The fastest a pawn turns, in radians per second: one turn per second.
pub const TURN_RATE: f32 = std::f32::consts::TAU;

/// Counts the ticks a walk has made no progress.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StallGuard {
    best_distance: Option<f32>,
    ticks: u32,
}

/// What a pawn is doing on its own.
#[derive(Clone, Debug, Default)]
pub enum Job {
    #[default]
    Idle,
    /// Walking the `route` to a stand cell, then digging `mark`; `next` is the waypoint being
    /// walked to.
    Mine {
        mark: [i32; 3],
        route: Vec<BlockPos>,
        next: usize,
        stall: StallGuard,
    },
}

/// Where a pawn stands and which way it faces. `feet` is the point on the ground under the
/// capsule; `yaw` is radians, forward being `(sin yaw, 0, cos yaw)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub feet: Vec3,
    pub yaw: f32,
}

/// What the pawn does this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JobAction {
    Wait,
    /// Walk toward `to`, the centre of a waypoint cell at feet level.
    Walk {
        to: Vec3,
    },
    /// Dig `block` by aiming at `aim`.
    Dig {
        block: BlockPos,
        aim: Vec3,
    },
}

/// A walk step in plain numbers: turn by `yaw_delta` radians, then move `forward` (0 to 1) of
/// full walking speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStep {
    pub yaw_delta: f32,
    pub forward: f32,
}

/// Advances the job in `pawn.job` one tick. An idle pawn claims the nearest reachable
/// unclaimed mark for `id` and equips its tool; a mining pawn walks its route and then digs.
/// A job that cannot continue releases its claim, keeps the mark and goes idle.
pub fn step(
    id: PawnId,
    pawn: &mut Pawn,
    pose: Pose,
    marks: &mut MineMarks,
    grid: &VoxelGrid,
) -> JobAction {
    if matches!(pawn.job, Job::Idle) && !claim(id, pawn, pose, marks, grid) {
        return JobAction::Wait;
    }
    mine(pawn, pose, marks, grid)
}

/// Takes the nearest unclaimed mark that has an exposed face and a route to a stand cell,
/// trying at most [`ROUTE_ATTEMPTS`] marks. Returns whether the pawn is now on a job.
fn claim(id: PawnId, pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> bool {
    let start = feet_cell(pose.feet);
    let squared_distance = |pos: [i32; 3]| {
        let d = BlockPos::from(pos) - start;
        i64::from(d.x).pow(2) + i64::from(d.y).pow(2) + i64::from(d.z).pow(2)
    };
    let mut candidates: Vec<[i32; 3]> = marks
        .unclaimed()
        .filter(|&pos| exposed_face_toward(grid, BlockPos::from(pos), Vec3::ZERO).is_some())
        .collect();
    candidates.sort_by_key(|&pos| squared_distance(pos));

    for pos in candidates.into_iter().take(ROUTE_ATTEMPTS) {
        let block = BlockPos::from(pos);
        let is_stand = |cell: BlockPos| {
            let d = cell - block;
            d.x.abs() <= 1 && d.z.abs() <= 1 && (-1..=2).contains(&d.y) && standable(grid, cell)
        };
        if let Some(route) = route(grid, start, is_stand, block, ROUTE_NODE_BUDGET)
            && marks.claim(pos, id)
        {
            pawn.select_slot(TOOL_SLOT);
            pawn.job = Job::Mine {
                mark: pos,
                route,
                next: 0,
                stall: StallGuard::default(),
            };
            return true;
        }
    }
    false
}

/// Advances a pawn that is on a mining job: walk the route, then dig.
fn mine(pawn: &mut Pawn, pose: Pose, marks: &mut MineMarks, grid: &VoxelGrid) -> JobAction {
    let Job::Mine {
        mark,
        route,
        next,
        stall,
    } = &mut pawn.job
    else {
        return JobAction::Wait;
    };
    let mark = *mark;

    if !marks.contains(mark) {
        pawn.job = Job::Idle;
        return JobAction::Wait;
    }

    if *next < route.len() && arrived(pose.feet, route[*next]) {
        *next += 1;
        *stall = StallGuard::default();
    }

    let Some(&waypoint) = route.get(*next) else {
        let block = BlockPos::from(mark);
        let eye = pose.feet + Vec3::Y * EYE_HEIGHT;
        return match exposed_face_toward(grid, block, eye) {
            Some(aim) => JobAction::Dig { block, aim },
            None => give_up(pawn, marks, mark),
        };
    };

    let to = waypoint_centre(waypoint);
    if !standable(grid, waypoint) || stall.stalled(horizontal_distance(pose.feet, to)) {
        return give_up(pawn, marks, mark);
    }
    JobAction::Walk { to }
}

/// Drops the claim on `mark`, which stays marked, and idles the pawn.
fn give_up(pawn: &mut Pawn, marks: &mut MineMarks, mark: [i32; 3]) -> JobAction {
    marks.release(mark);
    pawn.job = Job::Idle;
    JobAction::Wait
}

fn waypoint_centre(cell: BlockPos) -> Vec3 {
    cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5)
}

fn horizontal_distance(a: Vec3, b: Vec3) -> f32 {
    (a - b).with_y(0.0).length()
}

fn arrived(feet: Vec3, waypoint: BlockPos) -> bool {
    horizontal_distance(feet, waypoint_centre(waypoint)) <= ARRIVE_RADIUS
}

/// The centre of the face of `block` that borders air and lies nearest `eye`, or `None` when
/// every neighbour is solid.
fn exposed_face_toward(grid: &VoxelGrid, block: BlockPos, eye: Vec3) -> Option<Vec3> {
    const NEIGHBOURS: [BlockPos; 6] = [
        BlockPos::X,
        BlockPos::NEG_X,
        BlockPos::Y,
        BlockPos::NEG_Y,
        BlockPos::Z,
        BlockPos::NEG_Z,
    ];
    let centre = block.as_vec3() + Vec3::splat(0.5);
    NEIGHBOURS
        .into_iter()
        .filter(|&side| !grid.is_solid_at(block + side))
        .map(|side| centre + side.as_vec3() * 0.5)
        .min_by(|a, b| a.distance_squared(eye).total_cmp(&b.distance_squared(eye)))
}

impl StallGuard {
    /// Records a tick at `distance` from the waypoint; true once [`STALL_TICKS`] ticks in a row
    /// have come no closer than the best distance so far.
    fn stalled(&mut self, distance: f32) -> bool {
        if self.best_distance.is_none_or(|best| distance < best) {
            self.best_distance = Some(distance);
            self.ticks = 0;
        } else {
            self.ticks += 1;
        }
        self.ticks >= STALL_TICKS
    }
}

/// Steers a walking pawn toward `to`: turns at [`TURN_RATE`] (clamped to one tick) and walks at
/// full speed only within 60 degrees of facing it, else turns in place.
pub fn steer(pose: Pose, to: Vec3) -> WalkStep {
    let offset = to - pose.feet;
    let wanted = offset.x.atan2(offset.z);
    let turn = (wanted - pose.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let per_tick = TURN_RATE / TICK_HZ as f32;
    WalkStep {
        yaw_delta: turn.clamp(-per_tick, per_tick),
        forward: if turn.abs() <= WALK_CONE { 1.0 } else { 0.0 },
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

    const ME: PawnId = PawnId(0);
    const EYE_HEIGHT: f32 = 1.15;

    fn place(grid: &mut VoxelGrid, x: i32, y: i32, z: i32) {
        grid.mutator().place(BlockPos::new(x, y, z), 1, None);
    }

    /// A floor at y = 0 over `-10..=10`; feet cells are at y = 1.
    fn ground() -> VoxelGrid {
        let mut grid = VoxelGrid::new(16);
        for x in -10..=10 {
            for z in -10..=10 {
                place(&mut grid, x, 0, z);
            }
        }
        grid
    }

    fn center(cell: BlockPos) -> Vec3 {
        cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5)
    }

    fn pose_at(cell: BlockPos) -> Pose {
        Pose {
            feet: center(cell),
            yaw: 0.0,
        }
    }

    fn origin() -> BlockPos {
        BlockPos::new(0, 1, 0)
    }

    fn tick(pawn: &mut Pawn, marks: &mut MineMarks, grid: &VoxelGrid, pose: Pose) -> JobAction {
        step(ME, pawn, pose, marks, grid)
    }

    fn claimed_by(marks: &MineMarks, pos: [i32; 3]) -> Option<PawnId> {
        marks
            .iter()
            .find(|(p, _)| *p == pos)
            .and_then(|(_, mark)| mark.claimed_by)
    }

    fn claimed_mark(pawn: &Pawn) -> [i32; 3] {
        match &pawn.job {
            Job::Mine { mark, .. } => *mark,
            Job::Idle => panic!("the pawn should be on a job"),
        }
    }

    fn route_of(pawn: &Pawn) -> Vec<BlockPos> {
        match &pawn.job {
            Job::Mine { route, .. } => route.clone(),
            Job::Idle => panic!("the pawn should be on a job"),
        }
    }

    /// A marked block at (4, 1, 0) on flat ground, the pawn idle at the origin cell.
    fn marked_block() -> (VoxelGrid, MineMarks, Pawn) {
        let mut grid = ground();
        place(&mut grid, 4, 1, 0);
        let mut marks = MineMarks::default();
        marks.toggle([4, 1, 0]);
        (grid, marks, Pawn::default())
    }

    /// A pawn already on the job to dig `mark` from the single stand cell `from`.
    fn on_job(marks: &mut MineMarks, mark: [i32; 3], from: BlockPos) -> Pawn {
        marks.claim(mark, ME);
        Pawn {
            job: Job::Mine {
                mark,
                route: vec![from],
                next: 0,
                stall: StallGuard::default(),
            },
            ..Pawn::default()
        }
    }

    #[test]
    fn no_marks_leaves_the_pawn_waiting() {
        let grid = ground();
        let mut marks = MineMarks::default();
        let mut pawn = Pawn::default();

        let action = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(pawn.job, Job::Idle));

        marks.toggle([4, 1, 0]);
        let mut grid = grid;
        place(&mut grid, 4, 1, 0);
        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert!(
            matches!(pawn.job, Job::Mine { .. }),
            "a mark gives the pawn work"
        );
    }

    #[test]
    fn a_mark_with_no_exposed_face_is_not_claimed() {
        let mut grid = ground();
        for (x, y, z) in [
            (4, 1, 0),
            (3, 1, 0),
            (5, 1, 0),
            (4, 1, 1),
            (4, 1, -1),
            (4, 2, 0),
        ] {
            place(&mut grid, x, y, z);
        }
        place(&mut grid, 0, 1, 6);
        let mut marks = MineMarks::default();
        marks.toggle([4, 1, 0]);
        marks.toggle([0, 1, 6]);
        let mut pawn = Pawn::default();

        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(
            claimed_by(&marks, [4, 1, 0]),
            None,
            "buried, nearer, skipped"
        );
        assert_eq!(claimed_mark(&pawn), [0, 1, 6]);
    }

    #[test]
    fn an_idle_pawn_claims_the_nearest_reachable_mark_and_equips_the_tool() {
        let mut grid = ground();
        for (x, z) in [(-5, 0), (0, 6), (2, 0)] {
            place(&mut grid, x, 1, z);
        }
        let mut marks = MineMarks::default();
        for pos in [[-5, 1, 0], [0, 1, 6], [2, 1, 0]] {
            marks.toggle(pos);
        }
        let mut pawn = Pawn::default();
        pawn.select_slot(3);
        assert_eq!(pawn.equipped_tool, None);

        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(claimed_mark(&pawn), [2, 1, 0]);
        assert_eq!(claimed_by(&marks, [2, 1, 0]), Some(ME));
        assert_eq!(claimed_by(&marks, [-5, 1, 0]), None);
        assert_eq!(claimed_by(&marks, [0, 1, 6]), None);
        assert_eq!(pawn.equipped_tool, Some(STARTING_TOOL));
        assert_eq!(pawn.selected_slot, TOOL_SLOT);
    }

    #[test]
    fn the_pawn_walks_the_waypoints_then_digs_from_the_stand_cell() {
        let (grid, mut marks, mut pawn) = marked_block();

        let first = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        let route = route_of(&pawn);
        let stand = *route.last().expect("a route has cells");
        assert_eq!(
            route.first(),
            Some(&origin()),
            "the route starts under the pawn"
        );
        assert_eq!(
            stand,
            BlockPos::new(3, 1, 0),
            "the nearest stand cell by route"
        );
        assert_eq!(
            first,
            JobAction::Walk {
                to: center(route[1])
            }
        );

        for &waypoint in &route[1..route.len() - 1] {
            let action = tick(&mut pawn, &mut marks, &grid, pose_at(waypoint));
            let next = route.iter().position(|&c| c == waypoint).unwrap() + 1;
            assert_eq!(
                action,
                JobAction::Walk {
                    to: center(route[next])
                }
            );
        }
        let arrived = tick(&mut pawn, &mut marks, &grid, pose_at(stand));

        let JobAction::Dig { block, aim } = arrived else {
            panic!("at the stand cell the pawn digs, got {arrived:?}");
        };
        assert_eq!(block, BlockPos::new(4, 1, 0));
        assert!(
            aim.cmpge(Vec3::new(4.0, 1.0, 0.0)).all() && aim.cmple(Vec3::new(5.0, 2.0, 1.0)).all(),
            "the aim {aim} lies on the block"
        );
    }

    #[test]
    fn the_dig_aims_at_the_exposed_face_nearest_the_eye() {
        let cases = [
            (BlockPos::new(3, 1, 0), Vec3::new(4.0, 1.5, 0.5)),
            (BlockPos::new(4, 1, -1), Vec3::new(4.5, 1.5, 0.0)),
            (BlockPos::new(4, 1, 1), Vec3::new(4.5, 1.5, 1.0)),
        ];

        for (stand, face_centre) in cases {
            let mut grid = ground();
            place(&mut grid, 4, 1, 0);
            place(&mut grid, 4, 2, 0);
            let mut marks = MineMarks::default();
            marks.toggle([4, 1, 0]);
            let mut pawn = on_job(&mut marks, [4, 1, 0], stand);

            let action = tick(&mut pawn, &mut marks, &grid, pose_at(stand));

            assert_eq!(
                action,
                JobAction::Dig {
                    block: BlockPos::new(4, 1, 0),
                    aim: face_centre
                },
                "from {stand}"
            );
        }
    }

    proptest! {
        #[test]
        fn every_stand_cell_is_within_the_mining_reach(
            bx in -6..=6i32,
            bz in -6..=6i32,
            height in 1..=2i32,
        ) {
            prop_assume!((bx, bz) != (0, 0));
            let mut grid = ground();
            for y in 1..=height {
                place(&mut grid, bx, y, bz);
            }
            let mut marks = MineMarks::default();
            marks.toggle([bx, height, bz]);
            let mut pawn = Pawn::default();
            let mut feet = center(origin());

            let mut dig = None;
            for _ in 0..64 {
                let pose = Pose { feet, yaw: 0.0 };
                match tick(&mut pawn, &mut marks, &grid, pose) {
                    JobAction::Walk { to } => feet = to,
                    JobAction::Dig { block, aim } => {
                        dig = Some((block, aim));
                        break;
                    }
                    JobAction::Wait => break,
                }
            }

            let (block, aim) = dig.ok_or_else(|| TestCaseError::fail("the pawn never dug"))?;
            let stand = BlockPos::new(feet.x.floor() as i32, feet.y.floor() as i32, feet.z.floor() as i32);
            prop_assert_eq!(block, BlockPos::new(bx, height, bz));
            prop_assert!(standable(&grid, stand));
            prop_assert!((stand.x - bx).abs() <= 1 && (stand.z - bz).abs() <= 1);
            prop_assert!(stand.y >= height - 1 && stand.y <= height + 2);
            let eye = feet + Vec3::Y * EYE_HEIGHT;
            prop_assert!((aim - eye).length() <= MINE_REACH);
        }
    }

    #[test]
    fn a_cancelled_mark_releases_the_pawn_mid_walk() {
        let (grid, mut marks, mut pawn) = marked_block();
        let walking = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));
        assert!(matches!(walking, JobAction::Walk { .. }));
        let mid_walk = pose_at(BlockPos::new(1, 1, 0));

        marks.toggle([4, 1, 0]);
        let action = tick(&mut pawn, &mut marks, &grid, mid_walk);

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(pawn.job, Job::Idle));
        assert!(!marks.contains([4, 1, 0]));
    }

    #[test]
    fn a_mark_dug_by_another_pawn_releases_the_claim() {
        let (mut grid, mut marks, mut pawn) = marked_block();
        let walking = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));
        assert!(matches!(walking, JobAction::Walk { .. }));

        grid.mutator().remove(BlockPos::new(4, 1, 0));
        marks.block_removed([4, 1, 0]);
        let action = tick(
            &mut pawn,
            &mut marks,
            &grid,
            pose_at(BlockPos::new(1, 1, 0)),
        );

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(pawn.job, Job::Idle));
    }

    #[test]
    fn a_claimed_mark_is_not_offered_to_a_second_pawn() {
        let (mut grid, mut marks, mut pawn) = marked_block();
        marks.claim([4, 1, 0], PawnId(1));

        let alone = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(alone, JobAction::Wait);
        assert!(matches!(pawn.job, Job::Idle));
        assert_eq!(claimed_by(&marks, [4, 1, 0]), Some(PawnId(1)));

        place(&mut grid, 0, 1, 6);
        marks.toggle([0, 1, 6]);
        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(
            claimed_mark(&pawn),
            [0, 1, 6],
            "the free mark, though farther"
        );
        assert_eq!(claimed_by(&marks, [4, 1, 0]), Some(PawnId(1)));
    }

    #[test]
    fn a_stalled_walk_releases_the_claim_and_keeps_the_mark() {
        let (grid, mut marks, mut pawn) = marked_block();
        let stuck = pose_at(origin());
        let walking = tick(&mut pawn, &mut marks, &grid, stuck);
        assert!(matches!(walking, JobAction::Walk { .. }));

        let mut released_after = None;
        for n in 1..=STALL_TICKS + 5 {
            tick(&mut pawn, &mut marks, &grid, stuck);
            if matches!(pawn.job, Job::Idle) {
                released_after = Some(n);
                break;
            }
        }

        let n = released_after.expect("a stalled pawn gives up");
        assert!(
            (STALL_TICKS - 1..=STALL_TICKS + 1).contains(&n),
            "released after {n} stalled ticks"
        );
        assert!(marks.contains([4, 1, 0]), "the mark stays");
        assert_eq!(claimed_by(&marks, [4, 1, 0]), None);
    }

    #[test]
    fn progress_toward_the_waypoint_resets_the_stall_guard() {
        let (grid, mut marks, mut pawn) = marked_block();
        let mut feet = center(origin());
        tick(&mut pawn, &mut marks, &grid, Pose { feet, yaw: 0.0 });

        for _ in 0..STALL_TICKS + 30 {
            feet.x += 0.003;
            let action = tick(&mut pawn, &mut marks, &grid, Pose { feet, yaw: 0.0 });

            assert!(matches!(action, JobAction::Walk { .. }), "{action:?}");
        }

        assert!(matches!(pawn.job, Job::Mine { .. }));
        assert_eq!(claimed_by(&marks, [4, 1, 0]), Some(ME));
    }

    #[test]
    fn the_arrive_radius_is_horizontal_and_decides_when_a_waypoint_is_reached() {
        let waypoint_1 = center(BlockPos::new(1, 1, 0));
        let waypoint_2 = center(BlockPos::new(2, 1, 0));
        let cases = [
            ("0.3 m away", waypoint_1 - Vec3::X * 0.3, waypoint_2),
            ("0.45 m away", waypoint_1 - Vec3::X * 0.45, waypoint_1),
            ("0.4 m higher", waypoint_1 + Vec3::Y * 0.4, waypoint_2),
        ];

        for (name, feet, expected) in cases {
            let (grid, mut marks, mut pawn) = marked_block();
            tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

            let action = tick(&mut pawn, &mut marks, &grid, Pose { feet, yaw: 0.0 });

            assert_eq!(action, JobAction::Walk { to: expected }, "{name}");
        }
        const { assert!(ARRIVE_RADIUS > 0.3 && ARRIVE_RADIUS < 0.45) };
    }

    #[test]
    fn a_nearer_mark_with_no_reachable_stand_cell_is_skipped() {
        let mut grid = ground();
        place(&mut grid, 4, 1, 0);
        place(&mut grid, 0, 1, 6);
        for x in 2..=6 {
            for z in -2..=2 {
                if x == 2 || x == 6 || z == -2 || z == 2 {
                    place(&mut grid, x, 1, z);
                    place(&mut grid, x, 2, z);
                }
            }
        }
        let mut marks = MineMarks::default();
        marks.toggle([4, 1, 0]);
        marks.toggle([0, 1, 6]);
        let mut pawn = Pawn::default();

        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(claimed_mark(&pawn), [0, 1, 6]);
        assert_eq!(
            claimed_by(&marks, [4, 1, 0]),
            None,
            "walled in, not claimed"
        );
    }

    #[test]
    fn the_stand_cell_is_chosen_by_route_length_not_straight_distance() {
        let (mut grid, mut marks, mut pawn) = marked_block();
        for z in [-1, 0] {
            place(&mut grid, 2, 1, z);
            place(&mut grid, 2, 2, z);
        }

        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        let route = route_of(&pawn);
        assert_eq!(route.last(), Some(&BlockPos::new(3, 1, 1)));
        assert_eq!(route.len(), 5, "four moves around the wall");
    }

    #[test]
    fn marks_at_equal_distance_are_taken_in_key_order() {
        let mut grid = ground();
        place(&mut grid, 2, 1, 0);
        place(&mut grid, 0, 1, 2);
        let mut marks = MineMarks::default();
        marks.toggle([2, 1, 0]);
        marks.toggle([0, 1, 2]);
        let mut pawn = Pawn::default();

        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(claimed_mark(&pawn), [0, 1, 2]);
    }

    #[test]
    fn a_waypoint_that_stops_being_standable_releases_the_claim() {
        let (mut grid, mut marks, mut pawn) = marked_block();
        tick(&mut pawn, &mut marks, &grid, pose_at(origin()));
        let route = route_of(&pawn);
        assert!(route.len() > 2, "a walk of several waypoints");
        let blocked = route[1];

        place(&mut grid, blocked.x, blocked.y + 1, blocked.z);
        let action = tick(&mut pawn, &mut marks, &grid, pose_at(origin()));

        assert_eq!(action, JobAction::Wait);
        assert!(matches!(pawn.job, Job::Idle));
        assert!(marks.contains([4, 1, 0]), "the mark stays");
        assert_eq!(claimed_by(&marks, [4, 1, 0]), None);
    }

    const TURN_PER_TICK: f32 = std::f32::consts::TAU / TICK_HZ as f32;
    const TOLERANCE: f32 = 1e-4;

    fn facing(yaw: f32, bearing: f32) -> WalkStep {
        let feet = Vec3::new(2.0, 1.0, -3.0);
        let to = feet + Vec3::new(bearing.sin() * 5.0, 1.0, bearing.cos() * 5.0);
        steer(Pose { feet, yaw }, to)
    }

    #[test]
    fn steer_turns_toward_the_target_no_faster_than_the_turn_rate() {
        let right = facing(0.0, std::f32::consts::FRAC_PI_2);
        let left = facing(0.0, -std::f32::consts::FRAC_PI_2);
        let across_the_seam = facing(3.0, -3.0);
        let small = facing(0.0, 0.01);
        let on_target = facing(0.7, 0.7);

        assert!(
            (right.yaw_delta - TURN_PER_TICK).abs() < TOLERANCE,
            "{right:?}"
        );
        assert!(
            (left.yaw_delta + TURN_PER_TICK).abs() < TOLERANCE,
            "{left:?}"
        );
        assert!(
            (across_the_seam.yaw_delta - TURN_PER_TICK).abs() < TOLERANCE,
            "the short way round: {across_the_seam:?}"
        );
        assert!(
            (small.yaw_delta - 0.01).abs() < TOLERANCE,
            "no overshoot: {small:?}"
        );
        assert!(on_target.yaw_delta.abs() < TOLERANCE, "{on_target:?}");
    }

    #[test]
    fn steer_walks_only_when_facing_the_target() {
        let ahead = facing(0.0, 0.0);
        let near_edge = facing(0.0, 59.0_f32.to_radians());
        let past_edge = facing(0.0, 61.0_f32.to_radians());
        let behind = facing(0.0, std::f32::consts::PI);

        assert!((ahead.forward - 1.0).abs() < TOLERANCE, "{ahead:?}");
        assert!((near_edge.forward - 1.0).abs() < TOLERANCE, "{near_edge:?}");
        assert!(past_edge.forward.abs() < TOLERANCE, "{past_edge:?}");
        assert!(behind.forward.abs() < TOLERANCE, "{behind:?}");
        assert!(past_edge.yaw_delta > 0.0, "it turns in place instead");
        assert!(behind.yaw_delta.abs() > 0.0);
    }
}
