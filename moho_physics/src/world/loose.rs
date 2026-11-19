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
        let LooseShape::Box { half_extents } = self.shape;
        assert!(
            self.mass.is_finite() && self.mass > 0.0,
            "mass must be positive and finite, got {}",
            self.mass
        );
        assert!(
            half_extents.is_finite() && half_extents.min_element() > 0.0,
            "half extents must be positive and finite, got {half_extents}"
        );
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
        let LooseShape::Box { half_extents } = spawn.shape;

        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(spawn.position, spawn.rotation))
            .linvel(spawn.linear_velocity)
            .angvel(spawn.angular_velocity)
            .linear_damping(spawn.linear_damping)
            .angular_damping(spawn.angular_damping)
            .build();
        let handle = self.rigid_body_set.insert(body);
        let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
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

    /// How many loose objects are alive.
    pub fn loose_count(&self) -> usize {
        self.loose.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::test_support::{floor_mesh, slab_mesh, slope_mesh, TEST_TAG};
    use crate::world::RayQuery;
    use moho_render_api::{WorldMesh, WorldMeshId};
    use proptest::prelude::*;
    use std::f32::consts::{FRAC_PI_2, PI};

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
            LooseShape::Capsule {
                half_height: 0.2,
                radius: 0.1,
            },
            LooseShape::Cylinder {
                half_height: 0.2,
                radius: 0.1,
            },
        ];

        for shape in shapes {
            assert_spawn_reads_back(shape);
        }
    }

    fn assert_spawn_reads_back(shape: LooseShape) {
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
        let mass = world.rigid_body_set[world.loose[&id].body].mass();
        assert!(
            (mass - 3.5).abs() <= 1e-4,
            "{shape:?}: the body weighs the stated mass, mass={mass}"
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

    const GRAVITY: f32 = 18.0;
    const BOX: LooseShape = LooseShape::Box {
        half_extents: FLAT_BOX,
    };
    const CAPSULE: LooseShape = LooseShape::Capsule {
        half_height: 0.1,
        radius: 0.04,
    };
    const CYLINDER: LooseShape = LooseShape::Cylinder {
        half_height: 0.08,
        radius: 0.05,
    };

    fn gravity_world(floor: &WorldMesh) -> PhysicsWorld {
        let mut world = world_with_floor(floor);
        world.set_gravity(GRAVITY);
        world
    }

    /// A level floor at y = 0 under gravity 18 with the rest slope set to `slope` radians.
    fn floor_world_with_rest_slope(slope: f32) -> PhysicsWorld {
        let mut world = gravity_world(&floor_mesh(0.0));
        world.set_rest_slope(slope);
        world
    }

    /// A plane through the origin rising toward +z at `degrees`, under gravity 18.
    fn slope_world(degrees: f32) -> PhysicsWorld {
        let rise = 10.0 * degrees.to_radians().tan();
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(
            WorldMeshId(0),
            &slope_mesh(-10.0, -rise, 10.0, rise),
            TEST_TAG,
        );
        world.set_gravity(GRAVITY);
        world
    }

    /// The radius of a capsule or cylinder: how high its centre sits when it lies on its side.
    fn lying_height(shape: LooseShape) -> f32 {
        match shape {
            LooseShape::Capsule { radius, .. } | LooseShape::Cylinder { radius, .. } => radius,
            LooseShape::Box { .. } => panic!("a box does not lie on its side"),
        }
    }

    /// `shape` lying 1 mm above the slope through the origin at `degrees`, its axis along x so it
    /// can roll down toward -z.
    fn lying_across_slope(shape: LooseShape, degrees: f32) -> LooseSpawn {
        let angle = degrees.to_radians();
        let normal = Vec3::new(0.0, angle.cos(), -angle.sin());
        LooseSpawn {
            position: normal * (lying_height(shape) + 0.001),
            rotation: Quat::from_rotation_z(FRAC_PI_2),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// The 0.04 m capsule lying along z on the floor and rolling along +x at `speed` without
    /// slipping.
    fn rolling_capsule(speed: f32) -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, 0.04, 0.0),
            rotation: Quat::from_rotation_x(FRAC_PI_2),
            linear_velocity: Vec3::new(speed, 0.0, 0.0),
            angular_velocity: Vec3::new(0.0, 0.0, -speed / 0.04),
            ..LooseSpawn::new(CAPSULE, 0.2)
        }
    }

    /// The height of the shape's lowest point above y = 0.
    fn lowest_point(state: &LooseState, shape: LooseShape) -> f32 {
        let reach = match shape {
            LooseShape::Box { half_extents } => {
                (state.rotation * Vec3::X).y.abs() * half_extents.x
                    + (state.rotation * Vec3::Y).y.abs() * half_extents.y
                    + (state.rotation * Vec3::Z).y.abs() * half_extents.z
            }
            LooseShape::Capsule {
                half_height,
                radius,
            } => (state.rotation * Vec3::Y).y.abs() * half_height + radius,
            LooseShape::Cylinder {
                half_height,
                radius,
            } => {
                let along = (state.rotation * Vec3::Y).y.abs();
                along * half_height + radius * (1.0 - along * along).max(0.0).sqrt()
            }
        };
        state.position.y - reach
    }

    /// A deterministic source of floats for the tests; it needs no dependency and never changes.
    struct SplitMix(u64);

    impl SplitMix {
        fn next_bits(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// Uniform in `[low, high)`.
        fn range(&mut self, low: f32, high: f32) -> f32 {
            let unit = f32::from((self.next_bits() >> 48) as u16) / 65536.0;
            low + (high - low) * unit
        }

        fn vec3(&mut self, low: f32, high: f32) -> Vec3 {
            Vec3::new(
                self.range(low, high),
                self.range(low, high),
                self.range(low, high),
            )
        }
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in [("box", BOX), ("capsule", CAPSULE), ("cylinder", CYLINDER)] {
            for seed in 0..8 {
                let mut rng = SplitMix(0x804_0000 + seed);
                let horizontal = rng.vec3(-5.0, 5.0) * Vec3::new(1.0, 0.0, 1.0);
                let spin = rng.vec3(-20.0, 20.0);
                let mut world = gravity_world(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    linear_velocity: horizontal,
                    angular_velocity: spin,
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(&state, shape);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} seed {seed}: bottom {bottom} m off the floor, state {state:?}"
                );
                assert!(
                    state.asleep,
                    "{name} seed {seed}: not asleep after 5 s, state {state:?}"
                );
            }
        }
    }

    #[test]
    fn an_upright_capsule_and_cylinder_stand_at_their_own_heights() {
        let rows = [(CAPSULE, 0.1 + 0.04), (CYLINDER, 0.08)];

        for (shape, centre_height) in rows {
            let mut world = gravity_world(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, centre_height + 0.001, 0.0),
                ..LooseSpawn::new(shape, 0.2)
            });

            run(&mut world, 120);

            let state = read(&world, id);
            assert!(
                (state.position.y - centre_height).abs() <= 0.001,
                "{shape:?}: centre at {} not {centre_height}",
                state.position.y
            );
            assert!(state.asleep, "{shape:?}: upright shape sleeps, {state:?}");
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = gravity_world(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_capsule(2.0));
        let mut previous = read(&world, id).linear_velocity.length();

        for tick in 1..=300 {
            world.step(DT);
            let speed = read(&world, id).linear_velocity.length();
            assert!(
                speed <= previous + 1e-4,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        assert!(
            (0.5..1.5).contains(&state.position.x),
            "it rolls a while but stops within 1.5 m, x={}",
            state.position.x
        );
        assert!(state.asleep, "the capsule came to rest, {state:?}");
    }

    #[test]
    fn the_rest_slope_decides() {
        let rows = [
            (CAPSULE, 2.0, false),
            (CAPSULE, 4.9, false),
            (CYLINDER, 4.9, false),
            (CAPSULE, 5.5, true),
            (CYLINDER, 10.0, true),
        ];

        for (shape, degrees, rolls) in rows {
            let mut world = slope_world(degrees);
            let spawn = lying_across_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = (state.position - spawn.position).length();
            if rolls {
                assert!(
                    moved > 1.0,
                    "{shape:?} on {degrees} deg should roll down, moved {moved}"
                );
            } else {
                assert!(
                    moved < 0.02,
                    "{shape:?} on {degrees} deg should stay put, moved {moved}"
                );
                assert!(
                    state.asleep,
                    "{shape:?} on {degrees} deg should sleep, {state:?}"
                );
            }
        }
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let mut steeper = slope_world(8.0);
        steeper.set_rest_slope(10.0f32.to_radians());
        let spawn = lying_across_slope(CAPSULE, 8.0);
        let held = steeper.spawn_loose(&spawn);
        let mut flat = floor_world_with_rest_slope(0.0);
        let rolling = flat.spawn_loose(&rolling_capsule(2.0));

        run(&mut steeper, 300);
        run(&mut flat, 120);

        let held_state = read(&steeper, held);
        assert!(
            (held_state.position - spawn.position).length() < 0.02 && held_state.asleep,
            "a 10 degree rest slope holds the capsule on 8 degrees, {held_state:?}"
        );
        let rolling_state = read(&flat, rolling);
        assert!(
            rolling_state.linear_velocity.length() > 1.5 && !rolling_state.asleep,
            "with no brake the capsule keeps rolling, {rolling_state:?}"
        );
    }

    #[test]
    fn a_new_world_rests_at_five_degrees_and_reads_back_what_was_set() {
        let mut world = PhysicsWorld::new();
        let default = world.rest_slope();

        world.set_rest_slope(0.3);
        let set = world.rest_slope();
        world.set_rest_slope(0.0);
        let zero = world.rest_slope();

        assert!((default - 5.0f32.to_radians()).abs() <= 1e-6, "{default}");
        assert_eq!(set, 0.3);
        assert_eq!(zero, 0.0);
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let rows = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.01),
            ("tiny negative", -f32::MIN_POSITIVE),
            ("a quarter turn", FRAC_PI_2),
            ("past a quarter turn", 2.0),
            ("huge", f32::MAX),
        ];

        for (name, slope) in rows {
            let mut world = PhysicsWorld::new();

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));

            assert!(result.is_err(), "{name} ({slope}) was accepted");
        }
    }

    #[test]
    fn set_rest_slope_accepts_the_ends_of_its_range() {
        let mut world = PhysicsWorld::new();
        let nearly_vertical = FRAC_PI_2 - 0.001;

        world.set_rest_slope(0.0);
        world.set_rest_slope(nearly_vertical);

        assert_eq!(world.rest_slope(), nearly_vertical);
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 0.5, 0.0),
            linear_velocity: Vec3::new(1.0, 0.0, 0.0),
            angular_velocity: Vec3::new(3.0, 2.0, 1.0),
            ..LooseSpawn::new(BOX, 0.2)
        };
        let mut braked = floor_world_with_rest_slope(5.0f32.to_radians());
        let mut free = floor_world_with_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);
        let mut compared = 0;

        while read(&free, free_id).position.y > FLAT_BOX.y + 0.03 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            assert_eq!(
                a.linear_velocity.y.to_bits(),
                b.linear_velocity.y.to_bits(),
                "tick {compared}: vertical velocity {} vs {}",
                a.linear_velocity.y,
                b.linear_velocity.y
            );
            compared += 1;
        }

        assert!(compared >= 8, "the drop lasted only {compared} ticks");
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        let (half_height, radius) = (0.08, 0.05);

        for degrees in 10u8..=45 {
            let tilt = f32::from(degrees).to_radians();
            let reach = half_height * tilt.cos() + radius * tilt.sin();
            let mut world = gravity_world(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, reach + 0.001, 0.0),
                rotation: Quat::from_rotation_z(tilt),
                ..LooseSpawn::new(CYLINDER, 0.2)
            });

            run(&mut world, 600);

            let state = read(&world, id);
            let from_vertical = angle_from_vertical_degrees(state.rotation * Vec3::Y);
            let lying = (from_vertical - 90.0).abs() <= 0.5;
            let standing = from_vertical <= 0.5 || from_vertical >= 179.5;
            assert!(
                lying || standing,
                "{degrees} deg: frozen {from_vertical} deg from vertical, {state:?}"
            );
            assert!(state.asleep, "{degrees} deg: not asleep, {state:?}");
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 0.04 + 0.019, 0.0),
            ..rolling_capsule(2.0)
        };
        let mut braked = floor_world_with_rest_slope(5.0f32.to_radians());
        let mut free = floor_world_with_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        run(&mut braked, 2);
        run(&mut free, 2);

        let (a, b) = (read(&braked, braked_id), read(&free, free_id));
        assert_eq!(bits(&a), bits(&b), "braked {a:?}, free {b:?}");
        assert!(a.linear_velocity.x > 1.99, "it still rolls at speed, {a:?}");
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = gravity_world(&floor_mesh(0.0));
        let spawn = |z: f32| LooseSpawn {
            position: Vec3::new(0.0, 0.04, z),
            rotation: Quat::from_rotation_z(FRAC_PI_2),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let first = world.spawn_loose(&spawn(-0.035));
        let second = world.spawn_loose(&spawn(0.035));

        run(&mut world, 300);

        let (a, b) = (read(&world, first), read(&world, second));
        let depth = 0.08 - (a.position.z - b.position.z).abs();
        assert!(depth <= 0.005, "still {depth} m deep, {a:?} and {b:?}");
        assert!(a.asleep && b.asleep, "both rest, {a:?} and {b:?}");
    }

    /// Twenty random objects of all three shapes dropped into a 1 m square from staggered
    /// heights, each with a random orientation.
    fn pile_spawns(seed: u64) -> Vec<LooseSpawn> {
        let mut rng = SplitMix(seed);
        (0u8..20)
            .map(|i| {
                let shape = match i % 3 {
                    0 => LooseShape::Box {
                        half_extents: rng.vec3(0.03, 0.06),
                    },
                    1 => LooseShape::Capsule {
                        half_height: rng.range(0.04, 0.08),
                        radius: rng.range(0.025, 0.04),
                    },
                    _ => LooseShape::Cylinder {
                        half_height: rng.range(0.04, 0.08),
                        radius: rng.range(0.03, 0.05),
                    },
                };
                let axis = rng.vec3(-1.0, 1.0).normalize_or(Vec3::Y);
                LooseSpawn {
                    position: Vec3::new(
                        rng.range(-0.5, 0.5),
                        0.3 + 0.3 * f32::from(i),
                        rng.range(-0.5, 0.5),
                    ),
                    rotation: Quat::from_axis_angle(axis, rng.range(0.0, PI)),
                    ..LooseSpawn::new(shape, 0.2)
                }
            })
            .collect()
    }

    fn spawn_pile(world: &mut PhysicsWorld, spawns: &[LooseSpawn]) -> Vec<LooseId> {
        spawns
            .iter()
            .map(|spawn| world.spawn_loose(spawn))
            .collect()
    }

    /// The deepest penetration in metres among contacts with the floor, and among contacts
    /// between two loose objects, as the last step left them.
    fn deepest_penetrations(world: &PhysicsWorld) -> (f32, f32) {
        let (mut floor, mut between) = (0.0f32, 0.0f32);
        for pair in world.narrow_phase.contact_pairs() {
            let touches_floor = [pair.collider1, pair.collider2]
                .iter()
                .any(|collider| world.collider_set[*collider].parent().is_none());
            for manifold in pair.manifolds() {
                for point in &manifold.points {
                    let deepest = if touches_floor {
                        &mut floor
                    } else {
                        &mut between
                    };
                    *deepest = deepest.max(-point.dist);
                }
            }
        }
        (floor, between)
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [0x804, 0x805, 0x806, 0x807] {
            let spawns = pile_spawns(seed);
            let mut world = gravity_world(&floor_mesh(0.0));
            let ids = spawn_pile(&mut world, &spawns);

            run(&mut world, 600);

            for (id, spawn) in ids.iter().zip(&spawns) {
                let state = read(&world, *id);
                assert!(
                    state.asleep,
                    "seed {seed:#x}: {:?} is awake, {state:?}",
                    spawn.shape
                );
                let bottom = lowest_point(&state, spawn.shape);
                assert!(
                    bottom >= -0.001,
                    "seed {seed:#x}: {:?} sank {bottom} m into the floor",
                    spawn.shape
                );
            }
            let (floor, between) = deepest_penetrations(&world);
            assert!(floor <= 0.001, "seed {seed:#x}: {floor} m into the floor");
            assert!(
                between <= 0.005,
                "seed {seed:#x}: {between} m into another object"
            );
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let spawns = pile_spawns(0x804);
        let mut first = gravity_world(&floor_mesh(0.0));
        let mut second = gravity_world(&floor_mesh(0.0));
        let first_ids = spawn_pile(&mut first, &spawns);
        let second_ids = spawn_pile(&mut second, &spawns);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (index, (a_id, b_id)) in first_ids.iter().zip(&second_ids).enumerate() {
                let (a, b) = (read(&first, *a_id), read(&second, *b_id));
                assert_eq!(
                    bits(&a),
                    bits(&b),
                    "tick {tick}, object {index}: state bits differ"
                );
                assert_eq!(
                    a.asleep, b.asleep,
                    "tick {tick}, object {index}: asleep differs"
                );
            }
        }
        assert!(
            read(&first, first_ids[19]).position.y < 1.0,
            "the pile fell"
        );
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        type Build = fn(f32, f32) -> LooseShape;
        let builds: [(&str, Build); 2] = [
            ("capsule", |half_height, radius| LooseShape::Capsule {
                half_height,
                radius,
            }),
            ("cylinder", |half_height, radius| LooseShape::Cylinder {
                half_height,
                radius,
            }),
        ];
        let mut rows = Vec::new();

        for (shape_name, build) in builds {
            for (name, bad) in BAD_POSITIVE {
                rows.push((
                    format!("{name} {shape_name} half height"),
                    LooseSpawn::new(build(bad, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} {shape_name} radius"),
                    LooseSpawn::new(build(0.1, bad), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }
}
