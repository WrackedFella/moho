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

    fn assert_valid(self) {
        match self {
            Self::Box { half_extents } => assert!(
                half_extents.is_finite() && half_extents.min_element() > 0.0,
                "half extents must be positive and finite, got {half_extents}"
            ),
            Self::Capsule {
                half_height,
                radius,
            }
            | Self::Cylinder {
                half_height,
                radius,
            } => assert!(
                half_height.is_finite() && half_height > 0.0 && radius.is_finite() && radius > 0.0,
                "half height and radius must be positive and finite, got {half_height} and {radius}"
            ),
        }
    }
}

/// Steepest ground, radians from level, on which loose objects are braked to rest.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0 * std::f32::consts::PI / 180.0;

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
        self.shape.assert_valid();
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
    /// braked to rest. 0 turns the brake off. Default 5°.
    ///
    /// # Panics
    /// If `slope` is not finite or not in `[0, π/2)`.
    pub fn set_rest_slope(&mut self, slope: f32) {
        self.rest_slope = slope;
    }

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
            assert_spawn_reads_back_as_given(shape);
        }
    }

    fn assert_spawn_reads_back_as_given(shape: LooseShape) {
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
            "{state:?}"
        );
        assert!(
            state.rotation.abs_diff_eq(spawn.rotation, 1e-6),
            "{state:?}"
        );
        assert!(
            state
                .linear_velocity
                .abs_diff_eq(spawn.linear_velocity, 1e-6),
            "{state:?}"
        );
        assert!(
            state
                .angular_velocity
                .abs_diff_eq(spawn.angular_velocity, 1e-6),
            "{state:?}"
        );
        assert!(!state.asleep, "a fresh object is awake");
        let mass = world.rigid_body_set[world.loose[&id]].mass();
        assert!(
            (mass - 3.5).abs() <= 1e-4,
            "the body weighs the stated mass, mass={mass}"
        );
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
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        type Make = fn(f32, f32) -> LooseShape;
        let makes: [(&str, Make); 2] = [("capsule", capsule), ("cylinder", cylinder)];
        let mut rows = Vec::new();
        for (shape_name, make) in makes {
            for (name, value) in BAD_POSITIVE {
                rows.push((
                    format!("{name} half height on a {shape_name}"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} radius on a {shape_name}"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }

    fn rest_slope_of(degrees: f32) -> f32 {
        degrees.to_radians()
    }

    /// Deterministic noise for seeded drops.
    struct XorShift(u32);

    impl XorShift {
        /// Uniform in [-1, 1).
        fn signed(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
        }

        /// Uniform in [0, 1).
        fn unit(&mut self) -> f32 {
            (self.signed() + 1.0) / 2.0
        }

        fn vec3(&mut self, scale: f32) -> Vec3 {
            Vec3::new(self.signed(), self.signed(), self.signed()) * scale
        }

        fn rotation(&mut self) -> Quat {
            let axis = self.vec3(1.0).try_normalize().unwrap_or(Vec3::Y);
            Quat::from_axis_angle(axis, self.unit() * std::f32::consts::PI)
        }
    }

    /// How far below the floor plane y = 0 the object's lowest point sits; negative above it.
    fn lowest_point(shape: LooseShape, state: &LooseState) -> f32 {
        let up = state.rotation.inverse() * Vec3::Y;
        let reach = match shape {
            LooseShape::Box { half_extents } => {
                half_extents.x * up.x.abs()
                    + half_extents.y * up.y.abs()
                    + half_extents.z * up.z.abs()
            }
            LooseShape::Capsule {
                half_height,
                radius,
            } => half_height * up.y.abs() + radius,
            LooseShape::Cylinder {
                half_height,
                radius,
            } => half_height * up.y.abs() + radius * (up.x * up.x + up.z * up.z).sqrt(),
        };
        state.position.y - reach
    }

    fn collider_of(world: &PhysicsWorld, id: LooseId) -> rapier3d::prelude::ColliderHandle {
        world.rigid_body_set[world.loose[&id]].colliders()[0]
    }

    /// How deep, in metres, the two objects overlap; 0 when apart.
    fn overlap_depth(world: &PhysicsWorld, a: LooseId, b: LooseId) -> f32 {
        let first = &world.collider_set[collider_of(world, a)];
        let second = &world.collider_set[collider_of(world, b)];
        let pos12 = first.position().inv_mul(second.position());
        let contact = world
            .narrow_phase
            .query_dispatcher()
            .contact(&pos12, first.shape(), second.shape(), 0.0)
            .expect("convex shapes have a contact query");
        contact.map_or(0.0, |found| (-found.dist).max(0.0))
    }

    fn lying_along_x() -> Quat {
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    }

    const TEST_CAPSULE: (f32, f32) = (0.1, 0.04);
    const TEST_CYLINDER: (f32, f32) = (0.08, 0.05);

    /// A capsule lying along X on level ground at `z`, rolling toward +Z at `speed`.
    fn rolling_capsule(world: &mut PhysicsWorld, z: f32, speed: f32) -> LooseId {
        let (half_height, radius) = TEST_CAPSULE;
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, radius + 0.0005, z),
            rotation: lying_along_x(),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / radius, 0.0, 0.0),
            ..LooseSpawn::new(capsule(half_height, radius), 0.2)
        })
    }

    /// A world whose ground rises along +Z at `degrees`, through the origin.
    fn slope_world(degrees: f32) -> PhysicsWorld {
        let rise = rest_slope_of(degrees).tan() * 10.0;
        world_with_floor(&slope_mesh(-10.0, -rise, 10.0, rise))
    }

    /// Drops `shape` lying across the slope at `degrees`, touching it, at rest; returns where
    /// it was placed.
    fn place_on_slope(
        world: &mut PhysicsWorld,
        shape: LooseShape,
        radius: f32,
        degrees: f32,
    ) -> (LooseId, Vec3) {
        let angle = rest_slope_of(degrees);
        let normal = Vec3::new(0.0, angle.cos(), -angle.sin());
        let position = normal * (radius + 0.0005);
        let id = world.spawn_loose(&LooseSpawn {
            position,
            rotation: lying_along_x(),
            ..LooseSpawn::new(shape, 0.2)
        });
        (id, position)
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        let shapes = [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", capsule(TEST_CAPSULE.0, TEST_CAPSULE.1)),
            ("cylinder", cylinder(TEST_CYLINDER.0, TEST_CYLINDER.1)),
        ];
        let mut failures = Vec::new();

        for (name, shape) in shapes {
            let mut noise = XorShift(0x0804_0001);
            for drop in 0..8 {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let sideways = noise.vec3(3.0);
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    rotation: noise.rotation(),
                    linear_velocity: Vec3::new(sideways.x, 0.0, sideways.z),
                    angular_velocity: noise.vec3(20.0),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(shape, &state);
                if !state.asleep || bottom.abs() > 0.001 {
                    failures.push(format!(
                        "{name} drop {drop}: asleep={}, bottom {bottom} from the floor, state {state:?}",
                        state.asleep
                    ));
                }
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = rolling_capsule(&mut world, -5.0, 2.0);
        // Along the ground: the half-millimetre settle onto the floor adds vertical speed that is
        // not rolling.
        let along_ground = |world: &PhysicsWorld| {
            let velocity = read(world, id).linear_velocity;
            Vec3::new(velocity.x, 0.0, velocity.z).length()
        };
        let mut previous = along_ground(&world);

        for tick in 1..=600 {
            world.step(DT);
            let speed = along_ground(&world);
            assert!(
                speed <= previous + 1e-5,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        let rolled = state.position.z + 5.0;
        assert!(rolled <= 1.5, "rolled {rolled} m before stopping");
        assert!(state.asleep, "the capsule stopped, state {state:?}");
    }

    #[test]
    fn the_rest_slope_decides() {
        let capsule_shape = capsule(TEST_CAPSULE.0, TEST_CAPSULE.1);
        let cylinder_shape = cylinder(TEST_CYLINDER.0, TEST_CYLINDER.1);
        let rows = [
            ("capsule", capsule_shape, TEST_CAPSULE.1, 2.0, false),
            ("capsule", capsule_shape, TEST_CAPSULE.1, 4.9, false),
            ("cylinder", cylinder_shape, TEST_CYLINDER.1, 4.9, false),
            ("capsule", capsule_shape, TEST_CAPSULE.1, 5.5, true),
            ("cylinder", cylinder_shape, TEST_CYLINDER.1, 10.0, true),
        ];
        let default = PhysicsWorld::new().rest_slope();
        assert!(
            (default - rest_slope_of(5.0)).abs() <= 1e-6,
            "the default rest slope is 5 degrees, got {default} rad"
        );
        let mut failures = Vec::new();

        for (name, shape, radius, degrees, rolls) in rows {
            let mut world = slope_world(degrees);
            let (id, start) = place_on_slope(&mut world, shape, radius, degrees);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = state.position.distance(start);
            if rolls && moved <= 1.0 {
                failures.push(format!("{name} on {degrees} degrees moved only {moved} m"));
            }
            if !rolls && (moved >= 0.02 || !state.asleep) {
                failures.push(format!(
                    "{name} on {degrees} degrees moved {moved} m, asleep={}",
                    state.asleep
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let (half_height, radius) = TEST_CAPSULE;
        let mut tilted = slope_world(8.0);
        tilted.set_rest_slope(rest_slope_of(10.0));
        let (on_slope, _) = place_on_slope(&mut tilted, capsule(half_height, radius), radius, 8.0);
        let mut level = world_with_floor(&floor_mesh(0.0));
        level.set_rest_slope(0.0);
        let rolling = rolling_capsule(&mut level, -5.0, 2.0);

        run(&mut tilted, 300);
        run(&mut level, 120);

        assert_eq!(tilted.rest_slope(), rest_slope_of(10.0));
        let resting = read(&tilted, on_slope);
        assert!(
            resting.asleep,
            "a capsule sleeps on 8 degrees with the slope at 10, state {resting:?}"
        );
        assert_eq!(level.rest_slope(), 0.0);
        let state = read(&level, rolling);
        assert!(
            state.linear_velocity.z > 1.9 && !state.asleep,
            "with the brake off the capsule is still rolling, state {state:?}"
        );
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let bad = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.1),
            ("a right angle", std::f32::consts::FRAC_PI_2),
            ("past a right angle", 2.0),
        ];
        let mut accepted = Vec::new();

        for (name, slope) in bad {
            let mut world = PhysicsWorld::new();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));
            if outcome.is_ok() {
                accepted.push(name);
            }
        }

        assert!(accepted.is_empty(), "accepted: {accepted:?}");
        let mut world = PhysicsWorld::new();
        world.set_rest_slope(0.0);
        assert_eq!(world.rest_slope(), 0.0, "zero turns the brake off");
        world.set_rest_slope(1.5);
        assert_eq!(world.rest_slope(), 1.5, "just under a right angle is valid");
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let shapes = [
            ("box", cube(0.05), 0.05),
            (
                "capsule",
                capsule(TEST_CAPSULE.0, TEST_CAPSULE.1),
                TEST_CAPSULE.1,
            ),
            (
                "cylinder",
                cylinder(TEST_CYLINDER.0, TEST_CYLINDER.1),
                TEST_CYLINDER.1,
            ),
        ];

        for (name, shape, half) in shapes {
            let mut braked = world_with_floor(&floor_mesh(0.0));
            let mut free = world_with_floor(&floor_mesh(0.0));
            free.set_rest_slope(0.0);
            let spawn = LooseSpawn {
                restitution: 0.5,
                position: Vec3::new(0.0, 1.0 + half, 0.0),
                rotation: lying_along_x(),
                ..LooseSpawn::new(shape, 0.2)
            };
            let braked_id = braked.spawn_loose(&spawn);
            let free_id = free.spawn_loose(&spawn);

            for tick in 1..=120 {
                braked.step(DT);
                free.step(DT);
                let with_brake = read(&braked, braked_id).linear_velocity.y;
                let without = read(&free, free_id).linear_velocity.y;
                assert!(
                    (with_brake - without).abs() <= 1e-4,
                    "{name} tick {tick}: vertical velocity {with_brake} with the brake, {without} without"
                );
            }
        }
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        let (half_height, radius) = TEST_CYLINDER;
        let mut failures = Vec::new();

        for degrees in [10.0f32, 15.0, 20.0, 25.0, 27.0, 30.0, 35.0, 40.0, 45.0] {
            let tilt = rest_slope_of(degrees);
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(
                    0.0,
                    half_height * tilt.cos() + radius * tilt.sin() + 0.001,
                    0.0,
                ),
                rotation: Quat::from_rotation_x(tilt),
                ..LooseSpawn::new(cylinder(half_height, radius), 0.2)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            let axis_up = (state.rotation * Vec3::Y).y.abs();
            let standing = axis_up >= 1.0f32.to_radians().cos();
            let lying = axis_up <= 1.0f32.to_radians().sin();
            if !(state.asleep && (standing || lying)) {
                failures.push(format!(
                    "{degrees} degrees: asleep={}, axis up {axis_up}",
                    state.asleep
                ));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let (half_height, radius) = TEST_CAPSULE;
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, radius + 0.019, -5.0),
            rotation: lying_along_x(),
            linear_velocity: Vec3::new(0.0, 0.0, 2.0),
            angular_velocity: Vec3::new(2.0 / radius, 0.0, 0.0),
            ..LooseSpawn::new(capsule(half_height, radius), 0.2)
        };
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        run(&mut braked, 2);
        run(&mut free, 2);

        let (with_brake, without) = (read(&braked, braked_id), read(&free, free_id));
        assert!(
            with_brake.position.y > radius,
            "the capsule is still off the floor, state {with_brake:?}"
        );
        assert_eq!(bits(&with_brake), bits(&without));
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let (half_height, radius) = TEST_CAPSULE;
        let mut world = world_with_floor(&floor_mesh(0.0));
        let side = |z: f32| LooseSpawn {
            position: Vec3::new(0.0, radius, z),
            rotation: lying_along_x(),
            ..LooseSpawn::new(capsule(half_height, radius), 0.2)
        };
        let overlap = 0.01;
        let first = world.spawn_loose(&side(-(radius - overlap / 2.0)));
        let second = world.spawn_loose(&side(radius - overlap / 2.0));
        assert!(
            overlap_depth(&world, first, second) > 0.0,
            "the capsules start overlapped"
        );

        run(&mut world, 300);

        let depth = overlap_depth(&world, first, second);
        assert!(depth <= 0.005, "the capsules still overlap by {depth} m");
        assert!(read(&world, first).asleep && read(&world, second).asleep);
    }

    /// Twenty objects of all three shapes in a 1 m square, one above the other.
    fn spawn_pile(world: &mut PhysicsWorld, seed: u32) -> Vec<(LooseShape, LooseId)> {
        let mut noise = XorShift(seed);
        (0..20)
            .map(|index| {
                let shape = match index % 3 {
                    0 => cuboid(Vec3::new(
                        0.04 + 0.06 * noise.unit(),
                        0.04 + 0.06 * noise.unit(),
                        0.04 + 0.06 * noise.unit(),
                    )),
                    1 => capsule(TEST_CAPSULE.0, TEST_CAPSULE.1),
                    _ => cylinder(TEST_CYLINDER.0, TEST_CYLINDER.1),
                };
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(
                        noise.signed() * 0.5,
                        0.2 + 0.25 * index as f32,
                        noise.signed() * 0.5,
                    ),
                    rotation: noise.rotation(),
                    linear_velocity: noise.vec3(0.5),
                    angular_velocity: noise.vec3(3.0),
                    ..LooseSpawn::new(shape, 0.2)
                });
                (shape, id)
            })
            .collect()
    }

    const PILE_SEEDS: [u32; 4] = [0x9E37_79B1, 0x85EB_CA6B, 0xC2B2_AE35, 0x27D4_EB2F];

    #[test]
    fn a_pile_of_every_shape_settles() {
        let mut failures = Vec::new();

        for seed in PILE_SEEDS {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let pile = spawn_pile(&mut world, seed);

            run(&mut world, 600);

            for (shape, id) in &pile {
                let state = read(&world, *id);
                let into_floor = -lowest_point(*shape, &state);
                if !state.asleep {
                    failures.push(format!("seed {seed:#x}: {id:?} is awake, state {state:?}"));
                }
                if into_floor > 0.001 {
                    failures.push(format!(
                        "seed {seed:#x}: {id:?} is {into_floor} m into the floor"
                    ));
                }
            }
            for (index, (_, a)) in pile.iter().enumerate() {
                for (_, b) in &pile[index + 1..] {
                    let depth = overlap_depth(&world, *a, *b);
                    if depth > 0.005 {
                        failures.push(format!(
                            "seed {seed:#x}: {a:?} and {b:?} overlap by {depth} m"
                        ));
                    }
                }
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let mut first = world_with_floor(&floor_mesh(0.0));
        let mut second = world_with_floor(&floor_mesh(0.0));
        let first_pile = spawn_pile(&mut first, PILE_SEEDS[0]);
        let second_pile = spawn_pile(&mut second, PILE_SEEDS[0]);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for ((_, a), (_, b)) in first_pile.iter().zip(&second_pile) {
                let (left, right) = (read(&first, *a), read(&second, *b));
                assert_eq!(bits(&left), bits(&right), "tick {tick}: {a:?} differs");
                assert_eq!(
                    left.asleep, right.asleep,
                    "tick {tick}: {a:?} asleep differs"
                );
            }
        }
        assert!(
            first_pile
                .iter()
                .all(|(_, id)| read(&first, *id).position.y < 1.0),
            "the pile fell"
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
}
