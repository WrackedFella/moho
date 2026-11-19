use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{ColliderBuilder, Pose, RigidBodyBuilder};

/// Stable, engine-issued name of a loose object.
///
/// Issued in increasing order from zero and never reused, so the same call sequence issues the
/// same ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LooseId(u64);

impl LooseId {
    /// The issue number: how many ids the world had issued before this one.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// The collision shape of a loose object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LooseShape {
    /// A box centred on the body, `half_extents` m from the centre to each face.
    Box { half_extents: Vec3 },
    /// A capsule along the body's local Y: a cylinder of `half_height` capped by hemispheres.
    Capsule { half_height: f32, radius: f32 },
    /// A cylinder along the body's local Y.
    Cylinder { half_height: f32, radius: f32 },
}

/// Steepest ground, radians from level, on which loose objects are braked to rest: 5 degrees.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0 * std::f32::consts::PI / 180.0;

impl LooseShape {
    fn collider(self) -> ColliderBuilder {
        match self {
            Self::Box { half_extents } => {
                ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            Self::Capsule {
                half_height,
                radius,
            } => ColliderBuilder::capsule_y(half_height, radius),
            Self::Cylinder {
                half_height,
                radius,
            } => ColliderBuilder::cylinder(half_height, radius),
        }
    }
}

/// Everything a game states when it drops a loose object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LooseSpawn {
    pub shape: LooseShape,
    /// Kilograms.
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

impl LooseSpawn {
    /// A resting spawn at the origin with friction 0.5, no bounce and no damping.
    pub fn new(shape: LooseShape, mass: f32) -> Self {
        Self {
            shape,
            mass,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }

    /// Panics on any field that would poison the simulation; see [`PhysicsWorld::spawn_loose`].
    fn assert_valid(&self) {
        assert!(
            self.mass.is_finite() && self.mass > 0.0,
            "mass must be positive and finite, got {}",
            self.mass
        );
        match self.shape {
            LooseShape::Box { half_extents } => assert!(
                half_extents.is_finite() && half_extents.min_element() > 0.0,
                "half extents must be positive and finite, got {half_extents}"
            ),
            LooseShape::Capsule {
                half_height,
                radius,
            }
            | LooseShape::Cylinder {
                half_height,
                radius,
            } => assert!(
                half_height.is_finite() && half_height > 0.0 && radius.is_finite() && radius > 0.0,
                "half height and radius must be positive and finite, got {half_height} and {radius}"
            ),
        }
        assert!(
            self.linear_velocity.is_finite() && self.angular_velocity.is_finite(),
            "velocity must be finite, got linear {} and angular {}",
            self.linear_velocity,
            self.angular_velocity
        );
        for (name, value) in [
            ("friction", self.friction),
            ("restitution", self.restitution),
            ("linear damping", self.linear_damping),
            ("angular damping", self.angular_damping),
        ] {
            assert!(
                value.is_finite() && value >= 0.0,
                "{name} must be non-negative and finite, got {value}"
            );
        }
        assert!(
            self.position.is_finite(),
            "position must be finite, got {}",
            self.position
        );
        assert!(
            self.rotation.is_finite() && self.rotation.is_normalized(),
            "rotation must be finite and normalized, got {}",
            self.rotation
        );
    }
}

/// A loose object as the game reads it back: plain data, no engine handle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LooseState {
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub asleep: bool,
}

/// The id names a loose object that was never spawned or was removed.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("unknown loose object")]
pub struct UnknownLoose;

impl PhysicsWorld {
    /// Drop a loose object into the world and return its id.
    ///
    /// The object advances inside [`PhysicsWorld::step`] under the world's gravity.
    ///
    /// Characters do not ignore loose objects: a loose box blocks a character or is stepped
    /// onto like geometry, and any overlap shoves the box because the solver treats the
    /// character as infinitely heavy. Rays skip loose boxes because their colliders carry no
    /// tag.
    ///
    /// # Panics
    /// If the spawn would poison the simulation: a mass or half extent that is not positive and
    /// finite, a friction, restitution or damping that is negative or not finite, a position or
    /// velocity that is not finite, or a rotation that is not finite or whose squared length is
    /// more than 2e-4 from 1 (glam's `Quat::is_normalized`).
    pub fn spawn_loose(&mut self, spawn: &LooseSpawn) -> LooseId {
        spawn.assert_valid();

        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(spawn.position, spawn.rotation))
            .linvel(spawn.linear_velocity)
            .angvel(spawn.angular_velocity)
            .linear_damping(spawn.linear_damping)
            .angular_damping(spawn.angular_damping)
            .build();
        let handle = self.rigid_body_set.insert(body);
        let collider = spawn
            .shape
            .collider()
            .mass(spawn.mass)
            .friction(spawn.friction)
            .restitution(spawn.restitution)
            .build();
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        let id = LooseId(self.next_loose);
        self.next_loose += 1;
        self.loose.insert(id, handle);
        id
    }

    /// The object's current state; `None` for an id never spawned or removed.
    pub fn loose(&self, id: LooseId) -> Option<LooseState> {
        let body = self.rigid_body_set.get(*self.loose.get(&id)?)?;
        Some(LooseState {
            position: body.translation(),
            rotation: *body.rotation(),
            linear_velocity: body.linvel(),
            angular_velocity: body.angvel(),
            asleep: body.is_sleeping(),
        })
    }

    /// Remove the object's body and collider.
    ///
    /// # Errors
    /// [`UnknownLoose`] if `id` was never spawned or was already removed.
    pub fn remove_loose(&mut self, id: LooseId) -> Result<(), UnknownLoose> {
        let handle = self.loose.remove(&id).ok_or(UnknownLoose)?;
        self.rigid_body_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            true,
        );
        Ok(())
    }

    /// Steepest ground, radians from level against gravity, on which loose objects are
    /// braked to rest. 0 turns the brake off. Default 5 degrees.
    ///
    /// # Panics
    /// If `slope` is not finite or not in `[0, pi/2)`.
    pub fn set_rest_slope(&mut self, slope: f32) {
        assert!(
            slope.is_finite() && (0.0..std::f32::consts::FRAC_PI_2).contains(&slope),
            "rest slope must be finite and in [0, pi/2), got {slope}"
        );
        self.rest_slope = slope;
    }

    /// The rest slope, radians.
    pub fn rest_slope(&self) -> f32 {
        self.rest_slope
    }

    /// How many loose objects are alive.
    pub fn loose_count(&self) -> usize {
        self.loose.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::RayQuery;
    use crate::world::test_support::{TEST_TAG, floor_mesh, slab_mesh, slope_mesh};
    use moho_render_api::{WorldMesh, WorldMeshId};
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;

    fn world_with_floor(floor: &WorldMesh) -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), floor, TEST_TAG);
        world
    }

    fn run(world: &mut PhysicsWorld, ticks: u32) {
        for _ in 0..ticks {
            world.step(DT);
        }
    }

    fn read(world: &PhysicsWorld, id: LooseId) -> LooseState {
        world.loose(id).expect("the object is alive")
    }

    fn cuboid(half_extents: Vec3) -> LooseShape {
        LooseShape::Box { half_extents }
    }

    fn cube(half: f32) -> LooseShape {
        cuboid(Vec3::splat(half))
    }

    fn capsule(half_height: f32, radius: f32) -> LooseShape {
        LooseShape::Capsule {
            half_height,
            radius,
        }
    }

    fn cylinder(half_height: f32, radius: f32) -> LooseShape {
        LooseShape::Cylinder {
            half_height,
            radius,
        }
    }

    const FLAT_BOX: Vec3 = Vec3::new(0.1, 0.05, 0.07);

    fn flat_box_spawn() -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, 2.0, 0.0),
            linear_velocity: Vec3::new(1.5, 0.0, 0.0),
            ..LooseSpawn::new(cuboid(FLAT_BOX), 0.2)
        }
    }

    fn tumbling_spawn(angular: Vec3, horizontal: Vec3, down: f32) -> LooseSpawn {
        LooseSpawn {
            linear_velocity: horizontal - Vec3::Y * down,
            angular_velocity: angular,
            ..flat_box_spawn()
        }
    }

    fn angle_from_vertical_degrees(axis: Vec3) -> f32 {
        axis.dot(Vec3::Y).clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// Whether some local axis is within 0.5 degrees of vertical and the centre sits that
    /// axis's half-extent (within 1 mm) above the floor at y = 0.
    fn rests_on_a_face(state: &LooseState) -> Result<(), String> {
        let faces = [
            (Vec3::X, FLAT_BOX.x),
            (Vec3::Y, FLAT_BOX.y),
            (Vec3::Z, FLAT_BOX.z),
        ];
        for (local, half) in faces {
            let axis = state.rotation * local;
            let tilt = angle_from_vertical_degrees(axis).min(angle_from_vertical_degrees(-axis));
            if tilt <= 0.5 {
                return if (state.position.y - half).abs() <= 0.001 {
                    Ok(())
                } else {
                    Err(format!(
                        "y {} is not {half} above the floor",
                        state.position.y
                    ))
                };
            }
        }
        Err(format!(
            "no local axis is vertical, rotation {:?}",
            state.rotation
        ))
    }

    fn bits(state: &LooseState) -> [u32; 13] {
        let [px, py, pz] = state.position.to_array();
        let [rx, ry, rz, rw] = state.rotation.to_array();
        let [lx, ly, lz] = state.linear_velocity.to_array();
        let [ax, ay, az] = state.angular_velocity.to_array();
        [px, py, pz, rx, ry, rz, rw, lx, ly, lz, ax, ay, az].map(f32::to_bits)
    }

    #[test]
    fn a_dropped_box_comes_to_rest_flat_and_asleep() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&flat_box_spawn());

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(
            (state.position.y - 0.05).abs() <= 0.001,
            "the box rests on its half-height, y={}",
            state.position.y
        );
        let tilt = angle_from_vertical_degrees(state.rotation * Vec3::Y);
        assert!(
            tilt <= 0.5,
            "local up is within 0.5 degrees of world up, tilt={tilt}"
        );
        assert!(state.asleep, "the box sleeps after 5 s");
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 16,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x776),
            ..ProptestConfig::default()
        })]

        #[test]
        fn a_tumbling_box_rests_on_a_face(
            angular in (-20.0f32..20.0, -20.0f32..20.0, -20.0f32..20.0),
            horizontal in (-5.0f32..5.0, -5.0f32..5.0),
            down in 0.0f32..30.0,
        ) {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let spawn = tumbling_spawn(
                Vec3::new(angular.0, angular.1, angular.2),
                Vec3::new(horizontal.0, 0.0, horizontal.1),
                down,
            );
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = world
                .loose(id)
                .ok_or_else(|| TestCaseError::fail("the spawned box reads back"))?;
            prop_assert!(state.asleep, "asleep after 5 s, state {state:?}");
            if let Err(why) = rests_on_a_face(&state) {
                prop_assert!(false, "{why}, state {state:?}");
            }
        }
    }

    /// Drops a 5 cm cube on `floor` in each row and checks it ends resting on top, asleep.
    fn assert_fast_small_box_rests_on(floor: &WorldMesh) {
        const HALF: f32 = 0.025;
        let rows = [(20.0, 0.0), (2.0, 30.0), (2.0, 100.0)];

        for (height, speed) in rows {
            let mut world = world_with_floor(floor);
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, height, 0.0),
                linear_velocity: Vec3::new(0.0, -speed, 0.0),
                ..LooseSpawn::new(cube(HALF), 0.1)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            let y = state.position.y;
            assert!(
                y >= HALF - 0.001,
                "{height} m at {speed} m/s: box ended below the floor, y={y}"
            );
            assert!(
                y <= HALF + 0.001,
                "{height} m at {speed} m/s: box never came down to the floor, y={y}"
            );
            assert!(
                state.asleep,
                "{height} m at {speed} m/s: box is not asleep, state {state:?}"
            );
        }
    }

    #[test]
    fn a_fast_small_box_stays_above_a_thin_slab() {
        assert_fast_small_box_rests_on(&slab_mesh(0.0, 0.05));
    }

    #[test]
    fn a_fast_small_box_stays_above_a_one_sided_quad() {
        assert_fast_small_box_rests_on(&floor_mesh(0.0));
    }

    #[test]
    fn rays_pass_through_a_loose_box() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.2, 0.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        run(&mut world, 60);
        let y = read(&world, id).position.y;
        assert!(
            (y - 0.2).abs() <= 0.001,
            "the box rests on the floor, y={y}"
        );

        let hit = world
            .cast_ray(&RayQuery {
                origin: Vec3::new(0.0, 3.0, 0.0),
                direction: Vec3::NEG_Y,
                max_distance: 10.0,
                ignore: None,
            })
            .expect("the floor is in range");

        assert_eq!(hit.tag, TEST_TAG);
        assert!(
            hit.point.y.abs() <= 0.001,
            "hit the floor, not the box: {hit:?}"
        );
        assert!((hit.distance - 3.0).abs() <= 0.001, "{hit:?}");
    }

    #[test]
    fn the_same_drop_twice_ends_bit_equal() {
        let spawn = tumbling_spawn(Vec3::new(7.0, -3.0, 11.0), Vec3::new(2.0, 0.0, -1.5), 4.0);
        let mut first = world_with_floor(&floor_mesh(0.0));
        let mut second = world_with_floor(&floor_mesh(0.0));
        let first_id = first.spawn_loose(&spawn);
        let second_id = second.spawn_loose(&spawn);

        for tick in 1..=300 {
            first.step(DT);
            second.step(DT);
            let (a, b) = (read(&first, first_id), read(&second, second_id));
            assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
            assert_eq!(a.asleep, b.asleep, "tick {tick}: asleep differs");
        }
        assert!(read(&first, first_id).position.y < 1.0, "the box fell");
    }

    #[test]
    fn an_unknown_id_reads_nothing() {
        let spawn = flat_box_spawn();
        let mut larger = PhysicsWorld::new();
        let mut smaller = PhysicsWorld::new();
        larger.spawn_loose(&spawn);
        larger.spawn_loose(&spawn);
        let foreign = larger.spawn_loose(&spawn);
        let own = smaller.spawn_loose(&spawn);
        assert!(smaller.loose(own).is_some(), "a live id reads a pose");

        let from_larger_world = smaller.loose(foreign);
        smaller.remove_loose(own).expect("the object is alive");
        let after_removal = smaller.loose(own);

        assert_eq!(from_larger_world, None);
        assert_eq!(after_removal, None);
    }

    #[test]
    fn removing_a_loose_box_frees_its_body_and_collider() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let (bodies, colliders) = (world.rigid_body_set.len(), world.collider_set.len());
        let id = world.spawn_loose(&flat_box_spawn());
        let other = world.spawn_loose(&flat_box_spawn());
        assert_eq!(world.rigid_body_set.len(), bodies + 2);
        assert_eq!(world.collider_set.len(), colliders + 2);

        let first_removal = world.remove_loose(id);
        let second_removal = world.remove_loose(id);
        world
            .remove_loose(other)
            .expect("the other object is alive");

        assert_eq!(first_removal, Ok(()));
        assert_eq!(second_removal, Err(UnknownLoose));
        assert_eq!(world.rigid_body_set.len(), bodies);
        assert_eq!(world.collider_set.len(), colliders);
    }

    proptest! {
        #[test]
        fn loose_ids_are_issued_in_order_and_never_reused(
            ops in proptest::collection::vec((any::<bool>(), 0usize..8), 0..30)
        ) {
            let mut world = PhysicsWorld::new();
            let mut live: Vec<LooseId> = Vec::new();
            let mut removed: Vec<LooseId> = Vec::new();
            let mut last_issued: Option<LooseId> = None;

            for (spawn, pick) in ops {
                if spawn || live.is_empty() {
                    let id = world.spawn_loose(&flat_box_spawn());
                    prop_assert!(last_issued.is_none_or(|last| id > last), "{id:?} follows {last_issued:?}");
                    prop_assert_eq!(id.get(), (live.len() + removed.len()) as u64);
                    last_issued = Some(id);
                    live.push(id);
                } else {
                    let id = live.remove(pick % live.len());
                    prop_assert_eq!(world.remove_loose(id), Ok(()));
                    removed.push(id);
                }
            }

            prop_assert_eq!(world.loose_count(), live.len());
            for id in &live {
                prop_assert!(world.loose(*id).is_some(), "{id:?} is alive");
            }
            for id in &removed {
                prop_assert!(world.loose(*id).is_none(), "{id:?} was removed");
            }
        }
    }

    #[test]
    fn a_spawn_reads_back_as_given_before_any_step() {
        let shapes = [
            cuboid(Vec3::new(0.1, 0.2, 0.3)),
            capsule(0.2, 0.1),
            cylinder(0.2, 0.1),
        ];
        for shape in shapes {
            let mut world = PhysicsWorld::new();
            let spawn = LooseSpawn {
                friction: 0.3,
                restitution: 0.2,
                linear_damping: 0.1,
                angular_damping: 0.4,
                position: Vec3::new(1.0, 2.0, 3.0),
                rotation: Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7),
                linear_velocity: Vec3::new(0.5, -1.0, 2.0),
                angular_velocity: Vec3::new(0.1, 0.2, 0.3),
                ..LooseSpawn::new(shape, 3.5)
            };

            let id = world.spawn_loose(&spawn);

            let state = read(&world, id);
            assert!(
                state.position.abs_diff_eq(spawn.position, 1e-6),
                "{shape:?}: {state:?}"
            );
            assert!(
                state.rotation.abs_diff_eq(spawn.rotation, 1e-6),
                "{shape:?}: {state:?}"
            );
            assert!(
                state
                    .linear_velocity
                    .abs_diff_eq(spawn.linear_velocity, 1e-6),
                "{shape:?}: {state:?}"
            );
            assert!(
                state
                    .angular_velocity
                    .abs_diff_eq(spawn.angular_velocity, 1e-6),
                "{shape:?}: {state:?}"
            );
            assert!(!state.asleep, "{shape:?}: a fresh object is awake");
            let mass = world.rigid_body_set[world.loose[&id]].mass();
            assert!(
                (mass - 3.5).abs() <= 1e-4,
                "{shape:?}: the body weighs the stated mass, mass={mass}"
            );
        }
    }

    #[test]
    fn loose_boxes_fall_under_the_worlds_gravity() {
        let mut world = PhysicsWorld::new();
        world.set_gravity(9.0);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 10.0, 0.0),
            ..LooseSpawn::new(cube(0.1), 1.0)
        });

        run(&mut world, 60);

        let y = read(&world, id).position.y;
        assert!(
            (5.2..5.7).contains(&y),
            "a box falls about 4.5 m in 1 s at 9, y={y}"
        );
    }

    /// Height above the resting centre `rest` that the object climbs to after first touching
    /// down, over 3 s.
    fn peak_rebound(world: &mut PhysicsWorld, id: LooseId, rest: f32) -> f32 {
        let mut landed = false;
        let mut peak = 0.0f32;
        for _ in 0..180 {
            world.step(DT);
            let y = read(world, id).position.y;
            landed |= y <= rest + 0.01;
            if landed {
                peak = peak.max(y - rest);
            }
        }
        peak
    }

    #[test]
    fn higher_restitution_bounces_higher() {
        let rebound = |restitution: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                restitution,
                position: Vec3::new(0.0, 2.0, 0.0),
                ..LooseSpawn::new(cube(0.05), 0.2)
            });
            peak_rebound(&mut world, id, 0.05)
        };

        let dead = rebound(0.0);
        let lively = rebound(1.0);

        assert!(dead < 0.05, "a dead box stays down, rebound={dead}");
        assert!(
            lively > dead + 0.2,
            "dead rebound {dead}, lively rebound {lively}"
        );
    }

    #[test]
    fn lower_friction_slides_farther() {
        let slide = |friction: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                friction,
                position: Vec3::new(-5.0, 0.06, 0.0),
                linear_velocity: Vec3::new(4.0, 0.0, 0.0),
                ..LooseSpawn::new(cube(0.05), 0.2)
            });
            run(&mut world, 300);
            read(&world, id).position.x + 5.0
        };

        let grippy = slide(0.9);
        let slick = slide(0.1);

        assert!(
            slick > grippy + 0.3,
            "grippy slid {grippy}, slick slid {slick}"
        );
    }

    fn drifting_world() -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        world.set_gravity(0.0);
        world
    }

    #[test]
    fn linear_damping_slows_a_box_in_the_air() {
        let speed_after_a_second = |linear_damping: f32| {
            let mut world = drifting_world();
            let id = world.spawn_loose(&LooseSpawn {
                linear_damping,
                linear_velocity: Vec3::new(10.0, 0.0, 0.0),
                ..LooseSpawn::new(cube(0.1), 1.0)
            });
            run(&mut world, 60);
            read(&world, id).linear_velocity.length()
        };

        let free = speed_after_a_second(0.0);
        let damped = speed_after_a_second(2.0);

        assert!(free > 9.9, "an undamped box keeps its speed, speed={free}");
        assert!(damped < free - 5.0, "free {free}, damped {damped}");
    }

    #[test]
    fn angular_damping_slows_a_spinning_box() {
        let spin_after_a_second = |angular_damping: f32| {
            let mut world = drifting_world();
            let id = world.spawn_loose(&LooseSpawn {
                angular_damping,
                angular_velocity: Vec3::new(0.0, 5.0, 0.0),
                ..LooseSpawn::new(cube(0.1), 1.0)
            });
            run(&mut world, 60);
            read(&world, id).angular_velocity.length()
        };

        let free = spin_after_a_second(0.0);
        let damped = spin_after_a_second(2.0);

        assert!(free > 4.9, "an undamped box keeps its spin, spin={free}");
        assert!(damped < free - 2.5, "free {free}, damped {damped}");
    }

    fn valid_spawn() -> LooseSpawn {
        LooseSpawn::new(cube(0.1), 1.0)
    }

    /// The panic message of spawning `spawn` into a fresh world; `None` if it did not panic.
    fn spawn_panic_message(spawn: &LooseSpawn) -> Option<String> {
        let mut world = PhysicsWorld::new();
        let payload =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.spawn_loose(spawn)))
                .err()?;
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Some(message)
    }

    /// Every row must panic with a message containing `expected`.
    fn assert_rows_rejected(rows: &[(String, LooseSpawn)], expected: &str) {
        for (name, spawn) in rows {
            match spawn_panic_message(spawn) {
                None => panic!("{name}: the spawn was accepted"),
                Some(message) => assert!(
                    message.contains(expected),
                    "{name}: panic message {message:?} lacks {expected:?}"
                ),
            }
        }
    }

    const BAD_POSITIVE: [(&str, f32); 4] = [
        ("zero", 0.0),
        ("negative", -0.1),
        ("NaN", f32::NAN),
        ("infinite", f32::INFINITY),
    ];

    const BAD_NON_NEGATIVE: [(&str, f32); 3] = [
        ("negative", -0.1),
        ("NaN", f32::NAN),
        ("infinite", f32::INFINITY),
    ];

    const NON_FINITE: [(&str, f32); 3] = [
        ("NaN", f32::NAN),
        ("+inf", f32::INFINITY),
        ("-inf", f32::NEG_INFINITY),
    ];

    const AXES: [(&str, usize); 3] = [("x", 0), ("y", 1), ("z", 2)];

    fn with_component(base: Vec3, axis: usize, value: f32) -> Vec3 {
        let mut parts = base.to_array();
        parts[axis] = value;
        Vec3::from_array(parts)
    }

    #[test]
    fn spawn_rejects_a_bad_mass() {
        let rows: Vec<_> = BAD_POSITIVE
            .iter()
            .map(|&(name, mass)| (format!("{name} mass"), LooseSpawn::new(cube(0.1), mass)))
            .collect();

        assert_rows_rejected(&rows, "mass must be positive and finite");
    }

    #[test]
    fn spawn_rejects_a_bad_half_extent() {
        let mut rows = Vec::new();
        for (axis_name, axis) in AXES {
            for (name, value) in BAD_POSITIVE {
                let half = with_component(Vec3::splat(0.1), axis, value);
                rows.push((
                    format!("{name} half extent on {axis_name}"),
                    LooseSpawn::new(cuboid(half), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half extents must be positive and finite");
    }

    #[test]
    fn spawn_rejects_a_non_finite_velocity() {
        let mut rows = Vec::new();
        for (axis_name, axis) in AXES {
            for (name, value) in NON_FINITE {
                let bad = with_component(Vec3::ZERO, axis, value);
                rows.push((
                    format!("{name} linear velocity on {axis_name}"),
                    LooseSpawn {
                        linear_velocity: bad,
                        ..valid_spawn()
                    },
                ));
                rows.push((
                    format!("{name} angular velocity on {axis_name}"),
                    LooseSpawn {
                        angular_velocity: bad,
                        ..valid_spawn()
                    },
                ));
            }
        }

        assert_rows_rejected(&rows, "velocity must be finite");
    }

    #[test]
    fn spawn_rejects_a_bad_material_or_damping() {
        type Setter = fn(&mut LooseSpawn, f32);
        let fields: [(&str, &str, Setter); 4] = [
            (
                "friction",
                "friction must be non-negative and finite",
                |s, v| {
                    s.friction = v;
                },
            ),
            (
                "restitution",
                "restitution must be non-negative and finite",
                |s, v| s.restitution = v,
            ),
            (
                "linear damping",
                "damping must be non-negative and finite",
                |s, v| s.linear_damping = v,
            ),
            (
                "angular damping",
                "damping must be non-negative and finite",
                |s, v| s.angular_damping = v,
            ),
        ];

        for (field, expected, set) in fields {
            let rows: Vec<_> = BAD_NON_NEGATIVE
                .iter()
                .map(|&(name, value)| {
                    let mut spawn = valid_spawn();
                    set(&mut spawn, value);
                    (format!("{name} {field}"), spawn)
                })
                .collect();

            assert_rows_rejected(&rows, expected);
        }
    }

    #[test]
    fn spawn_rejects_a_non_finite_pose() {
        let mut position_rows = Vec::new();
        let mut rotation_rows = Vec::new();
        for (axis_name, axis) in AXES {
            for (name, value) in NON_FINITE {
                position_rows.push((
                    format!("{name} position on {axis_name}"),
                    LooseSpawn {
                        position: with_component(Vec3::ZERO, axis, value),
                        ..valid_spawn()
                    },
                ));
            }
        }
        for component in 0..4 {
            for (name, value) in NON_FINITE {
                let mut parts = Quat::IDENTITY.to_array();
                parts[component] = value;
                rotation_rows.push((
                    format!("{name} rotation component {component}"),
                    LooseSpawn {
                        rotation: Quat::from_array(parts),
                        ..valid_spawn()
                    },
                ));
            }
        }

        assert_rows_rejected(&position_rows, "position must be finite");
        assert_rows_rejected(&rotation_rows, "rotation must be finite");
    }

    #[test]
    fn spawn_rejects_a_non_unit_rotation() {
        let rows: Vec<_> = [
            ("zero quaternion", Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)),
            ("length-2 quaternion", Quat::from_xyzw(0.0, 0.0, 0.0, 2.0)),
        ]
        .into_iter()
        .map(|(name, rotation)| {
            (
                name.to_owned(),
                LooseSpawn {
                    rotation,
                    ..valid_spawn()
                },
            )
        })
        .collect();

        assert_rows_rejected(&rows, "rotation must be finite and normalized");
    }

    #[test]
    fn spawn_accepts_a_normalised_non_identity_rotation() {
        let mut world = PhysicsWorld::new();
        let rotation = Quat::from_rotation_y(0.7);
        let spawn = LooseSpawn {
            rotation,
            ..valid_spawn()
        };

        let message = spawn_panic_message(&spawn);
        let id = world.spawn_loose(&spawn);

        assert_eq!(message, None, "a unit rotation is valid");
        let state = read(&world, id);
        assert!(
            state.rotation.abs_diff_eq(rotation, 1e-6),
            "{:?}",
            state.rotation
        );
    }

    #[test]
    fn removing_a_box_wakes_the_box_resting_on_it() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let bottom = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.2, 0.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        let top = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.5, 0.0),
            ..LooseSpawn::new(cube(0.1), 1.0)
        });
        let mut ticks = 0;
        while ticks < 300 && !(read(&world, bottom).asleep && read(&world, top).asleep) {
            world.step(DT);
            ticks += 1;
        }
        assert!(
            read(&world, bottom).asleep && read(&world, top).asleep,
            "both boxes sleep within 300 ticks"
        );

        world.remove_loose(bottom).expect("the bottom box is alive");
        run(&mut world, 60);

        let state = read(&world, top);
        assert!(
            (state.position.y - 0.1).abs() <= 0.005,
            "the top box falls to the floor, y={}",
            state.position.y
        );
    }

    #[test]
    fn spawn_accepts_zero_friction_restitution_and_damping() {
        let mut world = PhysicsWorld::new();
        let spawn = LooseSpawn {
            friction: 0.0,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            position: Vec3::new(1.0, 2.0, 3.0),
            ..valid_spawn()
        };

        let message = spawn_panic_message(&spawn);
        let id = world.spawn_loose(&spawn);

        assert_eq!(message, None, "zero material values are valid");
        let state = read(&world, id);
        assert!(
            state.position.abs_diff_eq(spawn.position, 1e-6),
            "{state:?}"
        );
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        type Make = fn(f32, f32) -> LooseShape;
        let kinds: [(&str, Make); 2] = [("capsule", capsule), ("cylinder", cylinder)];
        let mut rows = Vec::new();
        for (kind, make) in kinds {
            for (name, value) in BAD_POSITIVE {
                rows.push((
                    format!("{name} {kind} half height"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} {kind} radius"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }

    const CAPSULE_RADIUS: f32 = 0.04;

    /// One of each shape, sized so that each is a plausible small prop.
    fn all_shapes() -> [LooseShape; 3] {
        [
            cuboid(FLAT_BOX),
            capsule(0.1, CAPSULE_RADIUS),
            cylinder(0.08, 0.05),
        ]
    }

    /// A small deterministic xorshift generator, so seeded scenes are the same on every run.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
        }

        /// Uniform in [0, 1).
        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let top = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 41;
            let mantissa = u32::try_from(top).expect("23 bits fit a u32");
            f32::from_bits(0x3f80_0000 | mantissa) - 1.0
        }

        fn range(&mut self, low: f32, high: f32) -> f32 {
            low + (high - low) * self.unit()
        }

        fn vec(&mut self, low: f32, high: f32) -> Vec3 {
            Vec3::new(
                self.range(low, high),
                self.range(low, high),
                self.range(low, high),
            )
        }

        fn rotation(&mut self) -> Quat {
            let axis = self.vec(-1.0, 1.0);
            let axis = if axis.length() > 1e-3 {
                axis.normalize()
            } else {
                Vec3::Y
            };
            Quat::from_axis_angle(axis, self.range(0.0, std::f32::consts::TAU))
        }
    }

    /// The lowest point of the object's colliders' bounding box.
    fn lowest_point(world: &PhysicsWorld, id: LooseId) -> f32 {
        let body = &world.rigid_body_set[world.loose[&id]];
        body.colliders()
            .iter()
            .map(|&collider| world.collider_set[collider].compute_aabb().mins.y)
            .fold(f32::INFINITY, f32::min)
    }

    /// A capsule whose axis runs along X, so that rolling carries it along Z, resting on level
    /// ground at height `y` of its centre, moving at `speed` m/s along +Z and spinning to match.
    fn rolling_capsule(y: f32, speed: f32) -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, y, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / CAPSULE_RADIUS, 0.0, 0.0),
            ..LooseSpawn::new(capsule(0.1, CAPSULE_RADIUS), 0.2)
        }
    }

    /// Ground rising `degrees` along +Z through the origin, so the downhill way is -Z.
    fn slope_world(degrees: f32) -> PhysicsWorld {
        let rise = 10.0 * degrees.to_radians().tan();
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(
            WorldMeshId(0),
            &slope_mesh(-10.0, -rise, 10.0, rise),
            TEST_TAG,
        );
        world
    }

    /// `shape`, whose axis runs along X, touching a `degrees` slope from `slope_world`, at rest.
    fn lying_on_slope(shape: LooseShape, radius: f32, degrees: f32) -> LooseSpawn {
        let angle = degrees.to_radians();
        LooseSpawn {
            position: Vec3::new(0.0, angle.cos() * radius, -angle.sin() * radius),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for shape in all_shapes() {
            for seed in 0..8 {
                let mut rng = Rng::new(seed + 1);
                let mut world = world_with_floor(&floor_mesh(0.0));
                let spawn = LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    rotation: rng.rotation(),
                    linear_velocity: Vec3::new(rng.range(-2.0, 2.0), 0.0, rng.range(-2.0, 2.0)),
                    angular_velocity: rng.vec(-15.0, 15.0),
                    ..LooseSpawn::new(shape, 0.2)
                };
                let id = world.spawn_loose(&spawn);

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(&world, id);
                assert!(
                    bottom.abs() <= 0.001,
                    "{shape:?} seed {seed}: bottom {bottom} is not on the floor, {state:?}"
                );
                assert!(
                    state.asleep,
                    "{shape:?} seed {seed}: not asleep after 5 s, {state:?}"
                );
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_capsule(CAPSULE_RADIUS, 2.0));
        let mut previous = read(&world, id).linear_velocity.length();

        for tick in 1..=300 {
            world.step(DT);
            let speed = read(&world, id).linear_velocity.length();
            assert!(
                speed <= previous + 1e-6,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        assert!(
            state.linear_velocity.length() < 0.01,
            "the capsule stopped, {state:?}"
        );
        assert!(
            state.position.length() <= 1.5,
            "it came to rest within 1.5 m, {state:?}"
        );
        assert!(state.asleep, "it sleeps once stopped, {state:?}");
    }

    #[test]
    fn the_rest_slope_decides() {
        // (shape, radius, degrees, rests)
        let rows = [
            (capsule(0.1, CAPSULE_RADIUS), CAPSULE_RADIUS, 2.0, true),
            (capsule(0.1, CAPSULE_RADIUS), CAPSULE_RADIUS, 4.9, true),
            (cylinder(0.08, 0.05), 0.05, 4.9, true),
            (capsule(0.1, CAPSULE_RADIUS), CAPSULE_RADIUS, 5.5, false),
            (cylinder(0.08, 0.05), 0.05, 10.0, false),
        ];

        for (shape, radius, degrees, rests) in rows {
            let mut world = slope_world(degrees);
            let spawn = lying_on_slope(shape, radius, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = (state.position - spawn.position).length();
            if rests {
                assert!(
                    moved < 0.02,
                    "{shape:?} on {degrees} degrees moved {moved} m"
                );
                assert!(
                    state.asleep,
                    "{shape:?} on {degrees} degrees is awake, {state:?}"
                );
            } else {
                assert!(
                    moved > 1.0,
                    "{shape:?} on {degrees} degrees moved only {moved} m"
                );
            }
        }
    }

    #[test]
    fn a_new_world_rests_objects_up_to_five_degrees() {
        let world = PhysicsWorld::new();

        assert!((world.rest_slope() - 5.0f32.to_radians()).abs() <= 1e-6);
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let mut steep = slope_world(8.0);
        steep.set_rest_slope(10.0f32.to_radians());
        let spawn = lying_on_slope(capsule(0.1, CAPSULE_RADIUS), CAPSULE_RADIUS, 8.0);
        let lying = steep.spawn_loose(&spawn);
        let mut flat = world_with_floor(&floor_mesh(0.0));
        flat.set_rest_slope(0.0);
        let rolling = flat.spawn_loose(&rolling_capsule(CAPSULE_RADIUS, 2.0));

        run(&mut steep, 300);
        run(&mut flat, 120);

        let held = read(&steep, lying);
        assert!(
            (steep.rest_slope() - 10.0f32.to_radians()).abs() <= 1e-6,
            "the setter stores the slope"
        );
        assert!(
            (held.position - spawn.position).length() < 0.02 && held.asleep,
            "a 10 degree rest slope holds the capsule on 8 degrees, {held:?}"
        );
        let free = read(&flat, rolling);
        assert!(
            free.linear_velocity.length() > 1.9,
            "with the brake off the capsule still rolls, {free:?}"
        );
        assert!(!free.asleep, "it is still awake, {free:?}");
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let rows = [
            ("NaN", f32::NAN),
            ("infinite", f32::INFINITY),
            ("negative infinity", f32::NEG_INFINITY),
            ("negative", -0.1),
            ("a quarter turn", std::f32::consts::FRAC_PI_2),
            ("past a quarter turn", 2.0),
        ];

        for (name, slope) in rows {
            let mut world = PhysicsWorld::new();
            let before = world.rest_slope();

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));

            assert!(outcome.is_err(), "{name}: the slope was accepted");
            assert_eq!(world.rest_slope(), before, "{name}: the slope changed");
        }
    }

    #[test]
    fn set_rest_slope_accepts_zero_and_just_under_a_quarter_turn() {
        let mut world = PhysicsWorld::new();

        world.set_rest_slope(0.0);
        let zero = world.rest_slope();
        world.set_rest_slope(1.5);
        let steep = world.rest_slope();

        assert_eq!(zero, 0.0);
        assert_eq!(steep, 1.5);
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        for shape in all_shapes() {
            let make = || {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    linear_velocity: Vec3::new(0.0, 0.0, 1.5),
                    ..LooseSpawn::new(shape, 0.2)
                });
                (world, id)
            };
            let (mut braked, braked_id) = make();
            let (mut free, free_id) = make();
            free.set_rest_slope(0.0);
            let mut compared = 0;

            for tick in 1..=60 {
                if lowest_point(&free, free_id) <= 0.0 {
                    break;
                }
                braked.step(DT);
                free.step(DT);
                let (with, without) = (read(&braked, braked_id), read(&free, free_id));
                assert_eq!(
                    with.linear_velocity.y.to_bits(),
                    without.linear_velocity.y.to_bits(),
                    "{shape:?} tick {tick}: vertical velocity {} vs {}",
                    with.linear_velocity.y,
                    without.linear_velocity.y
                );
                compared += 1;
            }

            assert!(compared >= 10, "{shape:?}: only {compared} ticks compared");
        }
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        let (half_height, radius) = (0.08, 0.05);

        for degrees in (10..=45).step_by(5) {
            let tilt = f32::from(u8::try_from(degrees).expect("small angle")).to_radians();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(
                    0.0,
                    half_height * tilt.cos() + radius * tilt.sin() + 0.0005,
                    0.0,
                ),
                rotation: Quat::from_rotation_z(tilt),
                ..LooseSpawn::new(cylinder(half_height, radius), 0.2)
            });

            run(&mut world, 600);

            let state = read(&world, id);
            let upright = (state.rotation * Vec3::Y).y.abs();
            let lying = upright <= 0.5f32.to_radians().sin();
            let standing = upright >= 0.5f32.to_radians().cos();
            assert!(
                lying || standing,
                "released at {degrees} degrees it froze tilted, axis y {upright}, {state:?}"
            );
            assert!(
                state.asleep,
                "released at {degrees} degrees it never slept, {state:?}"
            );
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let make = || {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&rolling_capsule(CAPSULE_RADIUS + 0.019, 2.0));
            (world, id)
        };
        let (mut braked, braked_id) = make();
        let (mut free, free_id) = make();
        free.set_rest_slope(0.0);

        run(&mut braked, 2);
        run(&mut free, 2);

        assert_eq!(
            bits(&read(&braked, braked_id)),
            bits(&read(&free, free_id)),
            "a predicted contact is not ground"
        );
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let side = |z: f32| LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_RADIUS, z),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(capsule(0.1, CAPSULE_RADIUS), 0.2)
        };
        let near = world.spawn_loose(&side(-0.035));
        let far = world.spawn_loose(&side(0.035));

        run(&mut world, 300);

        let (a, b) = (read(&world, near), read(&world, far));
        let gap = (b.position.z - a.position.z).abs();
        assert!(
            gap >= 2.0 * CAPSULE_RADIUS - 0.005,
            "the capsules are still {} m deep in each other",
            2.0 * CAPSULE_RADIUS - gap
        );
        assert!(a.asleep && b.asleep, "both sleep, {a:?} and {b:?}");
    }

    /// Twenty objects of all shapes in four layers over a 1 m square, seeded.
    fn pile_world(seed: u64) -> (PhysicsWorld, Vec<LooseId>) {
        const SLOTS: [(f32, f32); 5] = [
            (-0.3, -0.3),
            (0.3, -0.3),
            (0.0, 0.0),
            (-0.3, 0.3),
            (0.3, 0.3),
        ];
        let mut rng = Rng::new(seed);
        let shapes = all_shapes();
        let mut world = world_with_floor(&floor_mesh(0.0));
        let mut ids = Vec::new();
        for index in 0..20usize {
            let (x, z) = SLOTS[index % SLOTS.len()];
            let layer = f32::from(u8::try_from(index / SLOTS.len()).expect("four layers"));
            let spawn = LooseSpawn {
                position: Vec3::new(
                    x + rng.range(-0.03, 0.03),
                    0.3 + 0.4 * layer,
                    z + rng.range(-0.03, 0.03),
                ),
                rotation: rng.rotation(),
                linear_velocity: Vec3::new(rng.range(-1.0, 1.0), 0.0, rng.range(-1.0, 1.0)),
                angular_velocity: rng.vec(-5.0, 5.0),
                ..LooseSpawn::new(shapes[index % shapes.len()], 0.2)
            };
            ids.push(world.spawn_loose(&spawn));
        }
        (world, ids)
    }

    /// Deepest penetration, in metres, into the floor and into another loose object, over every
    /// contact the narrow phase holds.
    fn deepest_contacts(world: &PhysicsWorld) -> (f32, f32) {
        let (mut floor, mut pair) = (0.0f32, 0.0f32);
        for contact in world.narrow_phase.contact_pairs() {
            let touches_floor = [contact.collider1, contact.collider2]
                .iter()
                .any(|&collider| world.collider_set[collider].parent().is_none());
            for manifold in contact.manifolds() {
                for point in &manifold.points {
                    let depth = -point.dist;
                    if touches_floor {
                        floor = floor.max(depth);
                    } else {
                        pair = pair.max(depth);
                    }
                }
            }
        }
        (floor, pair)
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [1, 2, 3, 4] {
            let (mut world, ids) = pile_world(seed);

            run(&mut world, 600);

            for (index, id) in ids.iter().enumerate() {
                let state = read(&world, *id);
                assert!(
                    state.asleep,
                    "seed {seed} object {index} is awake, {state:?}"
                );
            }
            let (floor, pair) = deepest_contacts(&world);
            assert!(floor <= 0.001, "seed {seed}: {floor} m into the floor");
            assert!(pair <= 0.005, "seed {seed}: {pair} m into another object");
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let (mut first, first_ids) = pile_world(7);
        let (mut second, second_ids) = pile_world(7);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (index, (a, b)) in first_ids.iter().zip(&second_ids).enumerate() {
                let (a, b) = (read(&first, *a), read(&second, *b));
                assert_eq!(
                    bits(&a),
                    bits(&b),
                    "tick {tick} object {index}: state bits differ"
                );
                assert_eq!(
                    a.asleep, b.asleep,
                    "tick {tick} object {index}: asleep differs"
                );
            }
        }
        assert!(
            first_ids
                .iter()
                .all(|id| read(&first, *id).position.y < 1.0),
            "the pile fell"
        );
    }
}
