use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, NarrowPhase, Pose, RigidBody, RigidBodyBuilder, RigidBodyHandle, RigidBodySet,
};

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

/// The rest slope of a world that never calls [`PhysicsWorld::set_rest_slope`], radians.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0 * std::f32::consts::PI / 180.0;

/// Contact depth, m, past which two bodies are being pushed apart and must not be braked.
const DEEP_CONTACT: f32 = 0.005;

/// A loose object in the world: its body and what the rolling brake needs of its shape.
pub(super) struct Loose {
    body: RigidBodyHandle,
    /// The radius of a shape that rolls about its local Y axis; `None` for a box.
    roll_radius: Option<f32>,
}

/// How a loose body sits on what it touches.
struct Footing {
    /// A contact at or inside the surface, on ground within the rest slope of level.
    on_ground: bool,
    /// A contact deeper than [`DEEP_CONTACT`]: the solver is pushing this body out of another.
    being_separated: bool,
}

impl LooseShape {
    fn roll_radius(self) -> Option<f32> {
        match self {
            Self::Box { .. } => None,
            Self::Capsule { radius, .. } | Self::Cylinder { radius, .. } => Some(radius),
        }
    }

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
    /// Capsules and cylinders touching ground within the world's rest slope of level are braked
    /// to rest; see [`PhysicsWorld::set_rest_slope`].
    ///
    /// Characters do not ignore loose objects: a loose object blocks a character or is stepped
    /// onto like geometry, and any overlap shoves the object because the solver treats the
    /// character as infinitely heavy. Rays skip loose objects because their colliders carry no
    /// tag.
    ///
    /// # Panics
    /// If the spawn would poison the simulation: a mass, half extent, half height or radius that
    /// is not positive and finite, a friction, restitution or damping that is negative or not finite, a position or
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
        self.loose.insert(
            id,
            Loose {
                body: handle,
                roll_radius: spawn.shape.roll_radius(),
            },
        );
        id
    }

    /// The object's current state; `None` for an id never spawned or removed.
    pub fn loose(&self, id: LooseId) -> Option<LooseState> {
        let body = self.rigid_body_set.get(self.loose.get(&id)?.body)?;
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
        let handle = self.loose.remove(&id).ok_or(UnknownLoose)?.body;
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

    /// What the body touches, from the contacts of the step just run. `up` is a unit vector and
    /// `min_cos` the cosine of the rest slope.
    fn footing(
        bodies: &RigidBodySet,
        narrow_phase: &NarrowPhase,
        handle: RigidBodyHandle,
        up: Vec3,
        min_cos: f32,
    ) -> Footing {
        let (mut on_ground, mut deep) = (false, false);
        for &collider in bodies[handle].colliders() {
            for pair in narrow_phase.contact_pairs_with(collider) {
                let body_is_first = pair.collider1 == collider;
                for manifold in pair.manifolds() {
                    let into_body = if body_is_first {
                        -manifold.data.normal
                    } else {
                        manifold.data.normal
                    };
                    for point in &manifold.points {
                        deep |= point.dist < -DEEP_CONTACT;
                        on_ground |= point.dist <= 0.0 && into_body.dot(up) >= min_cos;
                    }
                }
            }
        }
        Footing {
            on_ground,
            being_separated: deep,
        }
    }

    /// Cut the spin about the body's Y axis by `decel * dt / radius` and its linear velocity along
    /// `axis x up` by `decel * dt`, each to zero at most, without waking the body.
    fn cut_rolling(body: &mut RigidBody, up: Vec3, decel: f32, dt: f32, radius: f32) {
        let axis = body.rotation() * Vec3::Y;
        let spin = body.angvel().dot(axis);
        let spin_after = spin.signum() * (spin.abs() - decel * dt / radius).max(0.0);
        body.set_angvel(body.angvel() + axis * (spin_after - spin), false);
        if let Some(roll) = axis.cross(up).try_normalize() {
            let speed = body.linvel().dot(roll);
            let speed_after = speed.signum() * (speed.abs() - decel * dt).max(0.0);
            body.set_linvel(body.linvel() + roll * (speed_after - speed), false);
        }
    }

    /// Cut the rolling motion of awake capsules and cylinders that touch near-flat ground, so
    /// they come to rest. Reads the contacts the step just computed and changes velocities only.
    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let Some(up) = (-self.gravity).try_normalize() else {
            return;
        };
        let decel = self.gravity.length() * self.rest_slope.tan();
        if decel <= 0.0 {
            return;
        }
        let min_cos = self.rest_slope.cos();
        for loose in self.loose.values() {
            let Some(radius) = loose.roll_radius else {
                continue;
            };
            if self.rigid_body_set[loose.body].is_sleeping() {
                continue;
            }
            let footing = Self::footing(
                &self.rigid_body_set,
                &self.narrow_phase,
                loose.body,
                up,
                min_cos,
            );
            if footing.on_ground && !footing.being_separated {
                Self::cut_rolling(&mut self.rigid_body_set[loose.body], up, decel, dt, radius);
            }
        }
    }

    /// The steepest ground, radians from level, on which loose objects are braked to rest.
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
    use crate::world::test_support::{floor_mesh, slab_mesh, slope_mesh, TEST_TAG};
    use crate::world::RayQuery;
    use moho_render_api::{WorldMesh, WorldMeshId};
    use proptest::prelude::*;
    use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};

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
            ..LooseSpawn::new(cuboid(Vec3::new(0.1, 0.2, 0.3)), 3.5)
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
        let (_, body) = world.rigid_body_set.iter().next().expect("one body");
        let mass = body.mass();
        assert!(
            (mass - 3.5).abs() <= 1e-4,
            "the body weighs the stated mass, mass={mass}"
        );
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

    const CAPSULE: (f32, f32) = (0.1, 0.04);
    const CYLINDER: (f32, f32) = (0.08, 0.05);

    fn small_capsule() -> LooseShape {
        capsule(CAPSULE.0, CAPSULE.1)
    }

    fn small_cylinder() -> LooseShape {
        cylinder(CYLINDER.0, CYLINDER.1)
    }

    /// A tiny deterministic generator, so a test's "random" input is the same on every run.
    struct Lcg(u64);

    impl Lcg {
        /// Uniform in `[0, 1)`.
        fn unit(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }

        fn range(&mut self, low: f32, high: f32) -> f32 {
            low + (high - low) * self.unit()
        }

        fn vec3(&mut self, low: f32, high: f32) -> Vec3 {
            Vec3::new(
                self.range(low, high),
                self.range(low, high),
                self.range(low, high),
            )
        }

        fn rotation(&mut self) -> Quat {
            let parts = [
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(0.1, 1.0),
            ];
            Quat::from_array(parts).normalize()
        }
    }

    /// The lowest point of the shape at `state`, y in the world.
    fn lowest_point(shape: LooseShape, state: &LooseState) -> f32 {
        let axis_y = |local: Vec3| (state.rotation * local).y.abs();
        let reach = match shape {
            LooseShape::Box { half_extents } => {
                half_extents.x * axis_y(Vec3::X)
                    + half_extents.y * axis_y(Vec3::Y)
                    + half_extents.z * axis_y(Vec3::Z)
            }
            LooseShape::Capsule {
                half_height,
                radius,
            } => half_height * axis_y(Vec3::Y) + radius,
            LooseShape::Cylinder {
                half_height,
                radius,
            } => {
                let along = axis_y(Vec3::Y);
                half_height * along + radius * (1.0 - along * along).max(0.0).sqrt()
            }
        };
        state.position.y - reach
    }

    fn sideways_distance(from: Vec3, state: &LooseState) -> f32 {
        (state.position - from).length()
    }

    fn degrees(angle: f32) -> f32 {
        angle.to_radians()
    }

    /// A capsule or cylinder lying on level ground with its axis along x, at `rolling` m/s along z
    /// with the spin that matches.
    fn lying_spawn(shape: LooseShape, radius: f32, rolling: f32) -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, radius, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            linear_velocity: Vec3::new(0.0, 0.0, rolling),
            angular_velocity: Vec3::new(rolling / radius, 0.0, 0.0),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// A world whose ground falls away along +z at `angle` degrees, passing through the origin.
    fn slope_world(angle: f32) -> PhysicsWorld {
        let drop = 10.0 * degrees(angle).tan();
        world_with_floor(&slope_mesh(-10.0, drop, 10.0, -drop))
    }

    /// The shape lying across that slope at the origin, axis along x, at rest.
    fn lying_across_slope(
        world: &mut PhysicsWorld,
        shape: LooseShape,
        radius: f32,
        angle: f32,
    ) -> LooseId {
        let tilt = degrees(angle);
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, tilt.cos(), tilt.sin()) * radius,
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(shape, 0.2)
        })
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        let shapes = [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", small_capsule()),
            ("cylinder", small_cylinder()),
        ];
        let mut rng = Lcg(0x807);

        for (name, shape) in shapes {
            for drop in 0..8 {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    linear_velocity: Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0)),
                    angular_velocity: rng.vec3(-20.0, 20.0),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(shape, &state);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} drop {drop}: bottom {bottom} m from the floor, state {state:?}"
                );
                assert!(state.asleep, "{name} drop {drop}: awake, state {state:?}");
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawn = lying_spawn(small_capsule(), CAPSULE.1, 2.0);
        let id = world.spawn_loose(&spawn);
        let ground_speed = |world: &PhysicsWorld| {
            let velocity = read(world, id).linear_velocity;
            Vec3::new(velocity.x, 0.0, velocity.z).length()
        };
        let mut speed = ground_speed(&world);

        for tick in 1..=300 {
            world.step(DT);
            let now = ground_speed(&world);
            assert!(
                speed <= 0.01 || now <= speed + 1e-5,
                "tick {tick}: speed rose from {speed} to {now}"
            );
            speed = now;
        }

        let state = read(&world, id);
        let rolled = sideways_distance(spawn.position, &state);
        assert!(rolled <= 1.5, "rolled {rolled} m before stopping");
        assert!(rolled >= 1.0, "stopped after only {rolled} m");
        assert!(state.asleep, "the capsule is at rest, state {state:?}");
    }

    #[test]
    fn the_rest_slope_decides() {
        let rows = [
            ("capsule", small_capsule(), CAPSULE.1, 2.0, false),
            ("capsule", small_capsule(), CAPSULE.1, 4.9, false),
            ("cylinder", small_cylinder(), CYLINDER.1, 4.9, false),
            ("capsule", small_capsule(), CAPSULE.1, 5.5, true),
            ("cylinder", small_cylinder(), CYLINDER.1, 10.0, true),
        ];

        for (name, shape, radius, angle, rolls) in rows {
            let mut world = slope_world(angle);
            let id = lying_across_slope(&mut world, shape, radius, angle);
            let start = read(&world, id).position;

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = sideways_distance(start, &state);
            if rolls {
                assert!(
                    moved > 1.0,
                    "{name} on {angle} degrees moved only {moved} m"
                );
            } else {
                assert!(moved < 0.02, "{name} on {angle} degrees moved {moved} m");
                assert!(state.asleep, "{name} on {angle} degrees is awake");
            }
        }
    }

    #[test]
    fn a_raised_rest_slope_lets_a_capsule_sleep_on_a_steeper_slope() {
        let mut world = slope_world(8.0);
        let default_rest = world.rest_slope();
        world.set_rest_slope(degrees(10.0));
        let id = lying_across_slope(&mut world, small_capsule(), CAPSULE.1, 8.0);
        let start = read(&world, id).position;

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(
            (default_rest - degrees(5.0)).abs() <= 1e-6,
            "{default_rest}"
        );
        assert!((world.rest_slope() - degrees(10.0)).abs() <= 1e-6);
        assert!(sideways_distance(start, &state) < 0.02, "{state:?}");
        assert!(state.asleep, "the capsule sleeps on 8 degrees, {state:?}");
    }

    #[test]
    fn a_zero_rest_slope_leaves_a_rolling_capsule_rolling() {
        let mut off = world_with_floor(&floor_mesh(0.0));
        off.set_rest_slope(0.0);
        let rolling = off.spawn_loose(&lying_spawn(small_capsule(), CAPSULE.1, 2.0));

        run(&mut off, 120);

        let state = read(&off, rolling);
        assert_eq!(off.rest_slope(), 0.0);
        assert!(
            state.linear_velocity.length() > 1.5,
            "with the brake off the capsule rolls on, {state:?}"
        );
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let rows = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.1),
            ("a right angle", std::f32::consts::FRAC_PI_2),
            ("past a right angle", 2.0),
        ];

        for (name, slope) in rows {
            let mut world = PhysicsWorld::new();

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));

            assert!(outcome.is_err(), "{name}: the slope was accepted");
        }
    }

    #[test]
    fn set_rest_slope_accepts_zero_and_just_under_a_right_angle() {
        let mut world = PhysicsWorld::new();

        world.set_rest_slope(0.0);
        let off = world.rest_slope();
        world.set_rest_slope(1.5);
        let steep = world.rest_slope();

        assert_eq!(off, 0.0);
        assert_eq!(steep, 1.5);
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 0.5, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            restitution: 0.8,
            ..LooseSpawn::new(small_capsule(), 0.2)
        };
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);
        let mut peak_after_bounce = 0.0f32;
        let mut free_peak = 0.0f32;
        let mut landed = false;

        for tick in 1..=120 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            let gap = (a.linear_velocity.y - b.linear_velocity.y).abs();
            assert!(
                gap <= 0.05,
                "tick {tick}: vertical velocity differs by {gap}"
            );
            landed |= a.position.y <= CAPSULE.1 + 0.01;
            if landed {
                peak_after_bounce = peak_after_bounce.max(a.position.y);
                free_peak = free_peak.max(b.position.y);
            }
        }

        assert!(
            peak_after_bounce > CAPSULE.1 + 0.1,
            "the capsule bounced, peak {peak_after_bounce}"
        );
        assert!(
            (peak_after_bounce - free_peak).abs() <= 0.001,
            "peak {peak_after_bounce} against {free_peak} without the brake"
        );
    }

    #[test]
    fn the_brake_follows_the_gravity_input() {
        let mut holds = slope_world(4.9);
        holds.set_gravity(9.0);
        let held = lying_across_slope(&mut holds, small_capsule(), CAPSULE.1, 4.9);
        let held_start = read(&holds, held).position;
        let mut rolls = slope_world(5.5);
        rolls.set_gravity(9.0);
        let rolled = lying_across_slope(&mut rolls, small_capsule(), CAPSULE.1, 5.5);
        let rolled_start = read(&rolls, rolled).position;

        run(&mut holds, 300);
        run(&mut rolls, 300);

        let state = read(&holds, held);
        assert!(
            sideways_distance(held_start, &state) < 0.02,
            "4.9 degrees at gravity 9: {state:?}"
        );
        assert!(state.asleep, "the capsule sleeps at gravity 9, {state:?}");
        let state = read(&rolls, rolled);
        assert!(
            sideways_distance(rolled_start, &state) > 1.0,
            "5.5 degrees at gravity 9: {state:?}"
        );
    }

    #[test]
    fn a_spinning_upright_cylinder_stops_spinning_and_sleeps() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, CYLINDER.0, 0.0),
            angular_velocity: Vec3::new(0.0, 5.0, 0.0),
            ..LooseSpawn::new(small_cylinder(), 0.2)
        };
        let id = world.spawn_loose(&spawn);

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(state.position.is_finite(), "{state:?}");
        assert!(state.asleep, "the cylinder stops spinning, {state:?}");
        assert!(
            sideways_distance(spawn.position, &state) < 0.01,
            "{state:?}"
        );
    }

    #[test]
    fn a_capsule_lying_off_the_axes_rolls_to_rest() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let rotation = Quat::from_rotation_y(degrees(30.0))
            * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let axis = rotation * Vec3::Y;
        let roll = axis.cross(Vec3::Y);
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, CAPSULE.1, 0.0),
            rotation,
            linear_velocity: roll * 2.0,
            angular_velocity: axis * (2.0 / CAPSULE.1),
            ..LooseSpawn::new(small_capsule(), 0.2)
        };
        let id = world.spawn_loose(&spawn);

        run(&mut world, 300);

        let state = read(&world, id);
        let rolled = sideways_distance(spawn.position, &state);
        assert!(rolled <= 1.5, "rolled {rolled} m before stopping");
        assert!(rolled >= 1.0, "stopped after only {rolled} m");
        assert!(state.asleep, "the capsule is at rest, {state:?}");
    }

    #[test]
    fn removing_a_capsule_mid_run_leaves_the_rolling_one_to_stop() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let rolling = world.spawn_loose(&lying_spawn(small_capsule(), CAPSULE.1, 2.0));
        let removed = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(2.0, CAPSULE.1, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(small_capsule(), 0.2)
        });
        run(&mut world, 10);

        world.remove_loose(removed).expect("the capsule is alive");
        run(&mut world, 120);

        let state = read(&world, rolling);
        assert_eq!(world.loose_count(), 1);
        assert_eq!(world.loose(removed), None);
        assert!(state.position.z <= 1.5, "{state:?}");
        assert!(state.asleep, "the other capsule still stops, {state:?}");
    }

    #[test]
    fn the_brake_never_wakes_a_sleeping_capsule() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, CAPSULE.1, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(small_capsule(), 0.2)
        });
        run(&mut world, 300);
        let asleep = read(&world, id);
        assert!(asleep.asleep, "the capsule sleeps, {asleep:?}");

        for tick in 1..=60 {
            world.step(DT);
            let state = read(&world, id);
            assert!(state.asleep, "tick {tick}: woken, {state:?}");
            assert_eq!(bits(&state), bits(&asleep), "tick {tick}: state changed");
        }
    }

    #[test]
    fn without_gravity_nothing_is_braked() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_gravity(0.0);
        let id = world.spawn_loose(&lying_spawn(small_capsule(), CAPSULE.1, 2.0));

        run(&mut world, 10);

        let state = read(&world, id);
        assert!(state.position.is_finite(), "{state:?}");
        assert!(state.linear_velocity.is_finite(), "{state:?}");
        assert!(
            (state.linear_velocity.length() - 2.0).abs() <= 1e-3,
            "speed is unchanged, {state:?}"
        );
    }

    #[test]
    fn a_capsule_across_a_box_top_sleeps_there() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let box_half = Vec3::splat(1.0);
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, box_half.y, 0.0),
            ..LooseSpawn::new(cuboid(box_half), 500.0)
        });
        let top = 2.0 * box_half.y + CAPSULE.1;
        let rolling = lying_spawn(small_capsule(), CAPSULE.1, 1.5);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, top, -0.5),
            ..rolling
        });

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(state.asleep, "the capsule is at rest, {state:?}");
        assert!(
            (state.position.y - top).abs() <= 0.005,
            "the capsule rests on the box top, {state:?}"
        );
        assert!(
            state.position.x.abs() <= box_half.x && state.position.z.abs() <= box_half.z,
            "the capsule is over the box, {state:?}"
        );
    }

    #[test]
    fn a_capsule_spawned_before_the_floor_still_rolls_to_rest() {
        let mut world = PhysicsWorld::new();
        let spawn = lying_spawn(small_capsule(), CAPSULE.1, 2.0);
        let id = world.spawn_loose(&spawn);
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);

        run(&mut world, 300);

        let state = read(&world, id);
        let rolled = sideways_distance(spawn.position, &state);
        assert!(rolled <= 1.5, "rolled {rolled} m before stopping");
        assert!(rolled >= 1.0, "stopped after only {rolled} m");
        assert!(state.asleep, "the capsule is at rest, {state:?}");
    }

    #[test]
    fn a_capsule_spawned_before_the_box_under_it_sleeps_on_it() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let box_half = Vec3::splat(1.0);
        let top = 2.0 * box_half.y + CAPSULE.1;
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, top, -0.5),
            ..lying_spawn(small_capsule(), CAPSULE.1, 1.5)
        });
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, box_half.y, 0.0),
            ..LooseSpawn::new(cuboid(box_half), 500.0)
        });

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(state.asleep, "the capsule is at rest, {state:?}");
        assert!(
            state.position.x.abs() <= box_half.x && state.position.z.abs() <= box_half.z,
            "the capsule is over the box, {state:?}"
        );
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        for tilt_degrees in [10.0f32, 18.0, 27.0, 35.0, 45.0] {
            let tilt = degrees(tilt_degrees);
            let (half_height, radius) = CYLINDER;
            let rim = half_height * tilt.cos() + radius * tilt.sin();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, rim + 0.001, 0.0),
                rotation: Quat::from_rotation_z(tilt),
                ..LooseSpawn::new(small_cylinder(), 0.2)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            let axis = state.rotation * Vec3::Y;
            let upright = angle_from_vertical_degrees(axis).min(angle_from_vertical_degrees(-axis));
            let lying_or_standing = upright <= 0.5 || upright >= 89.5;
            assert!(
                lying_or_standing,
                "{tilt_degrees} degrees: froze at {upright} degrees from vertical"
            );
            assert!(state.asleep, "{tilt_degrees} degrees: awake, {state:?}");
            let bottom = lowest_point(small_cylinder(), &state);
            assert!(
                bottom.abs() <= 0.001,
                "{tilt_degrees} degrees: bottom {bottom}"
            );
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let mut spawn = lying_spawn(small_capsule(), CAPSULE.1, 2.0);
        spawn.position.y += 0.019;
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        run(&mut braked, 2);
        run(&mut free, 2);

        let (a, b) = (read(&braked, braked_id), read(&free, free_id));
        assert_eq!(bits(&a), bits(&b), "{a:?} against {b:?}");
        assert!(a.position.y > CAPSULE.1 + 0.001, "still in the air, {a:?}");
    }

    /// How deep the loose objects sit in the floor and in each other, m: `(floor, pair)`.
    fn deepest_penetrations(world: &PhysicsWorld) -> (f32, f32) {
        let (mut floor, mut pair) = (0.0f32, 0.0f32);
        for contact in world.narrow_phase.contact_pairs() {
            let parent = |collider: ColliderHandle| world.collider_set[collider].parent();
            let loose = |body: Option<RigidBodyHandle>| {
                body.filter(|handle| world.rigid_body_set[*handle].is_dynamic())
            };
            let (first, second) = (
                loose(parent(contact.collider1)),
                loose(parent(contact.collider2)),
            );
            let depth = contact
                .manifolds()
                .iter()
                .flat_map(|manifold| manifold.points.iter())
                .map(|point| -point.dist)
                .fold(0.0f32, f32::max);
            match (first, second) {
                (Some(_), Some(_)) => pair = pair.max(depth),
                (Some(_), None) | (None, Some(_)) => floor = floor.max(depth),
                (None, None) => {}
            }
        }
        (floor, pair)
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids: Vec<LooseId> = [0.0, 0.07]
            .into_iter()
            .map(|z| {
                world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, CAPSULE.1, z),
                    rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    ..LooseSpawn::new(small_capsule(), 0.2)
                })
            })
            .collect();

        run(&mut world, 300);

        let (_, pair) = deepest_penetrations(&world);
        assert!(pair <= 0.005, "the capsules still overlap by {pair} m");
        for id in ids {
            assert!(read(&world, id).asleep, "{:?} is awake", read(&world, id));
        }
    }

    const PILE_SEEDS: [u64; 4] = [2, 4, 8, 11];

    fn pile_spawns(seed: u64) -> Vec<LooseSpawn> {
        let mut rng = Lcg(seed);
        (0..20)
            .map(|index| {
                let shape = match index % 3 {
                    0 => cuboid(rng.vec3(0.03, 0.06)),
                    1 => capsule(rng.range(0.04, 0.08), rng.range(0.02, 0.04)),
                    _ => cylinder(rng.range(0.03, 0.06), rng.range(0.03, 0.05)),
                };
                LooseSpawn {
                    position: Vec3::new(
                        rng.range(-0.5, 0.5),
                        0.3 + 0.25 * index as f32,
                        rng.range(-0.5, 0.5),
                    ),
                    rotation: rng.rotation(),
                    angular_velocity: rng.vec3(-5.0, 5.0),
                    ..LooseSpawn::new(shape, 0.2)
                }
            })
            .collect()
    }

    fn pile_world(seed: u64) -> (PhysicsWorld, Vec<LooseId>) {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids = pile_spawns(seed)
            .iter()
            .map(|spawn| world.spawn_loose(spawn))
            .collect();
        (world, ids)
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in PILE_SEEDS {
            let (mut world, ids) = pile_world(seed);

            run(&mut world, 600);

            let awake = ids.iter().filter(|id| !read(&world, **id).asleep).count();
            let (floor, pair) = deepest_penetrations(&world);
            assert_eq!(awake, 0, "seed {seed}: {awake} objects still awake");
            assert!(floor <= 0.001, "seed {seed}: {floor} m into the floor");
            assert!(pair <= 0.005, "seed {seed}: {pair} m into each other");
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let seed = PILE_SEEDS[0];
        let (mut first, first_ids) = pile_world(seed);
        let (mut second, second_ids) = pile_world(seed);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (a, b) in first_ids.iter().zip(&second_ids) {
                let (a, b) = (read(&first, *a), read(&second, *b));
                assert_eq!(bits(&a), bits(&b), "tick {tick}: bits differ");
                assert_eq!(a.asleep, b.asleep, "tick {tick}: asleep differs");
            }
        }
    }

    #[test]
    fn a_capsule_and_cylinder_spawn_read_back_as_given_before_any_step() {
        for shape in [capsule(0.2, 0.1), cylinder(0.2, 0.1)] {
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
            let (_, body) = world.rigid_body_set.iter().next().expect("one body");
            assert!(
                (body.mass() - 3.5).abs() <= 1e-4,
                "the body weighs the stated mass, mass={}",
                body.mass()
            );
        }
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        type Make = fn(f32, f32) -> LooseShape;
        let makers: [(&str, Make); 2] = [("capsule", capsule), ("cylinder", cylinder)];
        let mut rows = Vec::new();
        for (kind, make) in makers {
            for (name, value) in BAD_POSITIVE {
                rows.push((
                    format!("{name} half height of a {kind}"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} radius of a {kind}"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
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
}
