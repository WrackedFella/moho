use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{ColliderBuilder, ColliderHandle, Pose, RigidBodyBuilder, RigidBodyHandle};

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

    /// The radius a shape rolls on, `None` for a box.
    fn roll_radius(self) -> Option<f32> {
        match self {
            Self::Box { .. } => None,
            Self::Capsule { radius, .. } | Self::Cylinder { radius, .. } => Some(radius),
        }
    }
}

/// Steepest ground, radians, a world brakes loose objects on until it is told otherwise.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0_f32.to_radians();

/// Contact depth, m, past which a loose body counts as being pushed out of an overlap.
const DEEP_CONTACT: f32 = 0.005;

/// A loose object's body, its one collider and the radius it rolls on.
pub(super) struct Loose {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    roll_radius: Option<f32>,
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

/// The change that moves `speed` toward zero by at most `amount`.
fn cut(speed: f32, amount: f32) -> f32 {
    -speed.signum() * speed.abs().min(amount)
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
    /// Characters do not ignore loose objects: a loose object blocks a character or is stepped
    /// onto like geometry, and any overlap shoves the object because the solver treats the
    /// character as infinitely heavy. Rays skip loose objects because their colliders carry no
    /// tag.
    ///
    /// # Panics
    /// If the spawn would poison the simulation: a mass, box half extent, or capsule or cylinder
    /// half height or radius that is not positive and finite, a friction, restitution or damping
    /// that is negative or not finite, a position or velocity that is not finite, or a rotation
    /// that is not finite or whose squared length is more than 2e-4 from 1 (glam's
    /// `Quat::is_normalized`).
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
        let collider =
            self.collider_set
                .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        let id = LooseId(self.next_loose);
        self.next_loose += 1;
        self.loose.insert(
            id,
            Loose {
                body: handle,
                collider,
                roll_radius: spawn.shape.roll_radius(),
            },
        );
        id
    }

    /// Steepest ground, radians from level against gravity, on which loose objects are
    /// braked to rest. 0 turns the brake off. Default 5°.
    ///
    /// # Panics
    /// If `slope` is not finite or not in `[0, π/2)`.
    pub fn set_rest_slope(&mut self, slope: f32) {
        assert!(
            slope.is_finite() && (0.0..std::f32::consts::FRAC_PI_2).contains(&slope),
            "rest slope must be finite and in [0, pi/2), got {slope}"
        );
        self.rest_slope = slope;
    }

    /// The steepest ground, radians, on which loose objects are braked to rest.
    pub fn rest_slope(&self) -> f32 {
        self.rest_slope
    }

    /// Cuts the rolling motion of loose capsules and cylinders lying on near-flat ground so
    /// they come to rest instead of creeping.
    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let Some(up) = (-self.gravity).try_normalize() else {
            return;
        };
        let decel = self.gravity.length() * self.rest_slope.tan();
        if decel == 0.0 {
            return;
        }
        let min_up_dot = self.rest_slope.cos();

        for loose in self.loose.values() {
            let Some(radius) = loose.roll_radius else {
                continue;
            };
            let Some(body) = self.rigid_body_set.get(loose.body) else {
                continue;
            };
            if body.is_sleeping() {
                continue;
            }
            if !self.rests_on_flat_ground(loose.collider, up, min_up_dot) {
                continue;
            }

            let axis = *body.rotation() * Vec3::Y;
            let angvel = body.angvel();
            let linvel = body.linvel();
            let spin_cut = cut(angvel.dot(axis), decel * dt / radius);
            let new_angvel = (spin_cut != 0.0).then(|| angvel + axis * spin_cut);
            let new_linvel = axis.cross(up).try_normalize().and_then(|dir| {
                let roll_cut = cut(linvel.dot(dir), decel * dt);
                (roll_cut != 0.0).then(|| linvel + dir * roll_cut)
            });

            if new_angvel.is_none() && new_linvel.is_none() {
                continue;
            }
            // `get_mut` marks the body modified, so it is taken only when a velocity changes.
            let Some(body) = self.rigid_body_set.get_mut(loose.body) else {
                continue;
            };
            if let Some(angvel) = new_angvel {
                body.set_angvel(angvel, false);
            }
            if let Some(linvel) = new_linvel {
                body.set_linvel(linvel, false);
            }
        }
    }

    /// Whether a touching point (`dist <= 0`; a predicted contact is not ground) has a normal
    /// within the rest slope of `up`, and no point is deeper than [`DEEP_CONTACT`] (braking
    /// would freeze the velocity the solver uses to push an overlap apart).
    fn rests_on_flat_ground(&self, collider: ColliderHandle, up: Vec3, min_up_dot: f32) -> bool {
        let mut on_ground = false;
        for pair in self.narrow_phase.contact_pairs_with(collider) {
            let into_body = if pair.collider1 == collider {
                -1.0
            } else {
                1.0
            };
            for manifold in pair.manifolds() {
                let flat = (manifold.data.normal * into_body).dot(up) >= min_up_dot;
                for point in &manifold.points {
                    if point.dist < -DEEP_CONTACT {
                        return false;
                    }
                    on_ground |= flat && point.dist <= 0.0;
                }
            }
        }
        on_ground
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
        let loose = self.loose.remove(&id).ok_or(UnknownLoose)?;
        self.rigid_body_set.remove(
            loose.body,
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
        for shape in [cuboid(Vec3::new(0.1, 0.2, 0.3)), capsule(), cylinder()] {
            assert_a_spawn_reads_back_as_given(shape);
        }
    }

    fn assert_a_spawn_reads_back_as_given(shape: LooseShape) {
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
        assert_eq!(world.rigid_body_set.len(), 1, "the world holds one body");
        let (_, body) = world.rigid_body_set.iter().next().expect("one body");
        let mass = body.mass();
        assert!(
            (mass - 3.5).abs() <= 1e-4,
            "the body weighs the stated mass, mass={mass}"
        );
        assert_eq!(world.collider_set.len(), 1, "the world holds one collider");
        let (_, collider) = world.collider_set.iter().next().expect("one collider");
        match shape {
            LooseShape::Box { half_extents } => {
                let cuboid = collider.shape().as_cuboid().expect("a box is a cuboid");
                assert!(
                    cuboid.half_extents.abs_diff_eq(half_extents, 1e-6),
                    "{:?}",
                    cuboid.half_extents
                );
            }
            LooseShape::Capsule {
                half_height,
                radius,
            } => {
                let capsule = collider
                    .shape()
                    .as_capsule()
                    .expect("a capsule is a capsule");
                assert!(
                    capsule
                        .segment
                        .a
                        .abs_diff_eq(Vec3::new(0.0, -half_height, 0.0), 1e-6)
                        && capsule
                            .segment
                            .b
                            .abs_diff_eq(Vec3::new(0.0, half_height, 0.0), 1e-6),
                    "the capsule runs along local Y, {:?}",
                    capsule.segment
                );
                assert!((capsule.radius - radius).abs() <= 1e-6);
            }
            LooseShape::Cylinder {
                half_height,
                radius,
            } => {
                let cylinder = collider
                    .shape()
                    .as_cylinder()
                    .expect("a cylinder is a cylinder");
                assert!((cylinder.half_height - half_height).abs() <= 1e-6);
                assert!((cylinder.radius - radius).abs() <= 1e-6);
            }
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

    const CAPSULE_HALF_HEIGHT: f32 = 0.1;
    const CAPSULE_RADIUS: f32 = 0.04;
    const CYLINDER_HALF_HEIGHT: f32 = 0.08;
    const CYLINDER_RADIUS: f32 = 0.05;

    fn capsule() -> LooseShape {
        LooseShape::Capsule {
            half_height: CAPSULE_HALF_HEIGHT,
            radius: CAPSULE_RADIUS,
        }
    }

    fn cylinder() -> LooseShape {
        LooseShape::Cylinder {
            half_height: CYLINDER_HALF_HEIGHT,
            radius: CYLINDER_RADIUS,
        }
    }

    /// The radius of a capsule's or cylinder's round cross-section.
    fn round_radius(shape: LooseShape) -> f32 {
        match shape {
            LooseShape::Capsule { radius, .. } | LooseShape::Cylinder { radius, .. } => radius,
            LooseShape::Box { .. } => panic!("a box has no round cross-section"),
        }
    }

    /// The shape's local Y turned onto world X, so it lies across a slope that runs along z.
    fn lying_across() -> Quat {
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    }

    /// Height of the lowest point of `shape` in pose `state`.
    fn lowest_point(shape: LooseShape, state: &LooseState) -> f32 {
        let axis_y = (state.rotation * Vec3::Y).y.abs();
        let reach = match shape {
            LooseShape::Box { half_extents } => (0..3)
                .map(|i| half_extents[i] * (state.rotation * Vec3::AXES[i]).y.abs())
                .sum::<f32>(),
            LooseShape::Capsule {
                half_height,
                radius,
            } => half_height * axis_y + radius,
            LooseShape::Cylinder {
                half_height,
                radius,
            } => half_height * axis_y + radius * (1.0 - axis_y * axis_y).max(0.0).sqrt(),
        };
        state.position.y - reach
    }

    /// A small deterministic generator (splitmix64), so a seed always yields the same scene.
    struct Rng(u64);

    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// A value in `[low, high)`.
        fn range(&mut self, low: f32, high: f32) -> f32 {
            let unit = f32::from((self.next_u64() >> 48) as u16) / 65536.0;
            low + unit * (high - low)
        }
    }

    fn drop_shapes() -> [(&'static str, LooseShape); 3] {
        [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", capsule()),
            ("cylinder", cylinder()),
        ]
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in drop_shapes() {
            let mut rng = Rng(0xF48);
            for drop in 0..8 {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    linear_velocity: Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0)),
                    angular_velocity: Vec3::new(
                        rng.range(-20.0, 20.0),
                        rng.range(-20.0, 20.0),
                        rng.range(-20.0, 20.0),
                    ),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(shape, &state);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} drop {drop}: lowest point {bottom} m from the floor, state {state:?}"
                );
                assert!(state.asleep, "{name} drop {drop}: not asleep, {state:?}");
            }
        }
    }

    /// A capsule lying across level ground at `position`, rolling along +z at `speed` without
    /// slipping.
    fn rolling_capsule(position: Vec3, speed: f32) -> LooseSpawn {
        LooseSpawn {
            position,
            rotation: lying_across(),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / CAPSULE_RADIUS, 0.0, 0.0),
            ..LooseSpawn::new(capsule(), 0.2)
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        // The brake decelerates by gravity times tan 5 degrees: v^2 / 2a is 1.27 m at 18 and
        // 2.54 m at 9, so a hard-coded gravity falls outside one of the bands.
        for (gravity, stops_within) in [(18.0, 1.0..=1.5), (9.0, 2.0..=3.0)] {
            let mut world = world_with_floor(&floor_mesh(0.0));
            world.set_gravity(gravity);
            let start = Vec3::new(0.0, CAPSULE_RADIUS, -5.0);
            let id = world.spawn_loose(&rolling_capsule(start, 2.0));
            let mut previous = read(&world, id).linear_velocity.length();

            for tick in 1..=600 {
                world.step(DT);
                let speed = read(&world, id).linear_velocity.length();
                assert!(
                    speed <= previous + 1e-5,
                    "gravity {gravity}, tick {tick}: speed rose from {previous} to {speed}"
                );
                previous = speed;
            }

            let end = read(&world, id);
            let rolled = end.position.distance(start);
            assert!(
                stops_within.contains(&rolled),
                "gravity {gravity}: rolled {rolled} m before stopping, wanted {stops_within:?}"
            );
            assert!(
                end.linear_velocity.length() < 0.01,
                "gravity {gravity}: still moving: {end:?}"
            );
            assert!(end.asleep, "gravity {gravity}: not asleep, {end:?}");
        }
    }

    fn slope_world(degrees: f32) -> PhysicsWorld {
        let rise = degrees.to_radians().tan();
        world_with_floor(&slope_mesh(-10.0, 20.0 * rise, 10.0, 0.0))
    }

    /// A shape lying across the slope of `degrees` built by [`slope_world`], 0.5 m from its high
    /// edge, touching it and at rest. Downhill is +z.
    fn lying_on_slope(shape: LooseShape, degrees: f32) -> LooseSpawn {
        let angle = degrees.to_radians();
        let z = -9.5;
        let surface = Vec3::new(0.0, angle.tan() * (10.0 - z), z);
        let normal = Vec3::new(0.0, angle.cos(), angle.sin());
        LooseSpawn {
            position: surface + normal * round_radius(shape),
            rotation: lying_across(),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    #[test]
    fn the_rest_slope_decides() {
        let rows = [
            ("capsule", capsule(), 2.0, false),
            ("capsule", capsule(), 4.9, false),
            ("cylinder", cylinder(), 4.9, false),
            ("capsule", capsule(), 5.5, true),
            ("cylinder", cylinder(), 10.0, true),
        ];

        for (name, shape, degrees, rolls) in rows {
            let mut world = slope_world(degrees);
            let spawn = lying_on_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = state.position.distance(spawn.position);
            if rolls {
                assert!(
                    moved > 1.0,
                    "{name} on {degrees} degrees rolled only {moved} m, {state:?}"
                );
            } else {
                assert!(
                    moved < 0.02,
                    "{name} on {degrees} degrees moved {moved} m, {state:?}"
                );
                assert!(
                    state.asleep,
                    "{name} on {degrees} degrees is awake, {state:?}"
                );
            }
        }
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let fresh = PhysicsWorld::new();
        assert!(
            (fresh.rest_slope() - 5.0f32.to_radians()).abs() <= 1e-6,
            "the default rest slope is 5 degrees, got {} rad",
            fresh.rest_slope()
        );

        let mut steep = slope_world(8.0);
        steep.set_rest_slope(10.0f32.to_radians());
        let id = steep.spawn_loose(&lying_on_slope(capsule(), 8.0));
        run(&mut steep, 300);
        let mut flat = world_with_floor(&floor_mesh(0.0));
        flat.set_rest_slope(0.0);
        let rolling = flat.spawn_loose(&rolling_capsule(Vec3::new(0.0, CAPSULE_RADIUS, -5.0), 2.0));
        run(&mut flat, 120);

        assert!((steep.rest_slope() - 10.0f32.to_radians()).abs() <= 1e-6);
        assert!(
            read(&steep, id).asleep,
            "a capsule on 8 degrees sleeps under a 10 degree rest slope"
        );
        assert_eq!(flat.rest_slope(), 0.0);
        let state = read(&flat, rolling);
        assert!(
            state.linear_velocity.length() > 1.5 && !state.asleep,
            "with the brake off the capsule keeps rolling, {state:?}"
        );
    }

    fn set_rest_slope_panics(world: &mut PhysicsWorld, slope: f32) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.set_rest_slope(slope)))
            .is_err()
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let half_pi = std::f32::consts::FRAC_PI_2;
        let just_under = f32::from_bits(half_pi.to_bits() - 1);
        let rejected = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.1),
            ("pi/2", half_pi),
            ("above pi/2", 2.0),
        ];
        let mut world = PhysicsWorld::new();
        let default = world.rest_slope();

        for (name, slope) in rejected {
            assert!(
                set_rest_slope_panics(&mut world, slope),
                "{name} was accepted"
            );
            assert_eq!(
                world.rest_slope(),
                default,
                "{name} changed the rest slope before panicking"
            );
        }

        for (name, slope) in [("zero", 0.0), ("just under pi/2", just_under)] {
            assert!(
                !set_rest_slope_panics(&mut world, slope),
                "{name} was rejected"
            );
            assert_eq!(world.rest_slope(), slope, "{name} reads back as set");
        }
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        for down in [0.5, 2.0, 5.0] {
            let mut braked = world_with_floor(&floor_mesh(0.0));
            let mut free = world_with_floor(&floor_mesh(0.0));
            free.set_rest_slope(0.0);
            let spawn = LooseSpawn {
                restitution: 0.5,
                linear_velocity: Vec3::new(0.0, -down, 2.0),
                ..rolling_capsule(Vec3::new(0.0, CAPSULE_RADIUS - 0.0005, -5.0), 2.0)
            };
            let braked_id = braked.spawn_loose(&spawn);
            let free_id = free.spawn_loose(&spawn);

            braked.step(DT);
            free.step(DT);

            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            assert_eq!(
                a.linear_velocity.y.to_bits(),
                b.linear_velocity.y.to_bits(),
                "falling at {down} m/s: vertical velocity differs, braked {a:?}, free {b:?}"
            );
            assert_ne!(
                (a.linear_velocity.x.to_bits(), a.linear_velocity.z.to_bits()),
                (b.linear_velocity.x.to_bits(), b.linear_velocity.z.to_bits()),
                "falling at {down} m/s: the brake did not act, braked {a:?}, free {b:?}"
            );
        }
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        for degrees in [10.0f32, 20.0, 27.0, 35.0, 45.0] {
            let tilt = degrees.to_radians();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let rim_height = CYLINDER_HALF_HEIGHT * tilt.cos() + CYLINDER_RADIUS * tilt.sin();
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, rim_height + 0.001, 0.0),
                rotation: Quat::from_rotation_z(tilt),
                ..LooseSpawn::new(cylinder(), 0.2)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            let axis = state.rotation * Vec3::Y;
            let from_vertical = angle_from_vertical_degrees(axis);
            let from_vertical = from_vertical.min(180.0 - from_vertical);
            let standing = from_vertical <= 2.0;
            let lying = from_vertical >= 88.0;
            assert!(
                standing || lying,
                "released at {degrees} degrees it froze {from_vertical} degrees from vertical, {state:?}"
            );
            // It tips over past atan(radius / half height), about 32 degrees.
            if degrees <= 20.0 {
                assert!(
                    standing,
                    "released at {degrees} degrees it should stand, {state:?}"
                );
                assert!(
                    (state.position.y - CYLINDER_HALF_HEIGHT).abs() <= 0.001,
                    "standing centre height {}",
                    state.position.y
                );
            } else if degrees >= 45.0 {
                assert!(
                    lying,
                    "released at {degrees} degrees it should lie, {state:?}"
                );
                assert!(
                    (state.position.y - CYLINDER_RADIUS).abs() <= 0.001,
                    "lying centre height {}",
                    state.position.y
                );
            }
            assert!(
                state.asleep,
                "released at {degrees} degrees it is awake, {state:?}"
            );
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let spawn = rolling_capsule(Vec3::new(0.0, CAPSULE_RADIUS + 0.019, -5.0), 2.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        for tick in 1..=2 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
            assert!(
                lowest_point(capsule(), &a) > 0.0,
                "tick {tick}: the capsule touched the floor, {a:?}"
            );
            let gaps = floor_contact_gaps(&braked);
            assert!(
                gaps.iter().any(|&gap| gap > 0.0),
                "tick {tick}: no predicted floor contact point, gaps {gaps:?}"
            );
        }
    }

    /// The `dist` of every contact point between a loose body and the fixed floor; positive
    /// means predicted but not yet touching.
    fn floor_contact_gaps(world: &PhysicsWorld) -> Vec<f32> {
        let mut gaps = Vec::new();
        for contact in world.narrow_phase.contact_pairs() {
            let loose_sides = [contact.collider1, contact.collider2]
                .iter()
                .filter(|&&handle| world.collider_set[handle].parent().is_some())
                .count();
            if loose_sides == 1 {
                for manifold in contact.manifolds() {
                    gaps.extend(manifold.points.iter().map(|point| point.dist));
                }
            }
        }
        gaps
    }

    /// The deepest penetration, in metres, between any two loose bodies and between a loose body
    /// and the floor, from exact geometry at the bodies' current poses. The narrow phase's
    /// manifolds are not used: they predate the solver's last move and go stale once a body
    /// sleeps.
    struct Depths {
        pair: f32,
        floor: f32,
    }

    fn deepest_contacts(world: &PhysicsWorld) -> Depths {
        let loose: Vec<_> = world
            .collider_set
            .iter()
            .map(|(_, collider)| collider)
            .filter(|collider| collider.parent().is_some())
            .collect();
        let mut depths = Depths {
            pair: 0.0,
            floor: 0.0,
        };
        for (index, a) in loose.iter().enumerate() {
            // The floor is the plane y = 0 and the shapes are convex, so the lowest point of the
            // bounding box is the lowest point of the shape.
            depths.floor = depths.floor.max(-a.compute_aabb().mins.y);
            for b in &loose[index + 1..] {
                let contact = rapier3d::parry::query::contact(
                    a.position(),
                    a.shape(),
                    b.position(),
                    b.shape(),
                    0.0,
                )
                .expect("convex shapes have a contact query");
                if let Some(contact) = contact {
                    depths.pair = depths.pair.max(-contact.dist);
                }
            }
        }
        depths
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids: Vec<_> = [-0.035, 0.035]
            .into_iter()
            .map(|z| {
                world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, CAPSULE_RADIUS, z),
                    rotation: lying_across(),
                    ..LooseSpawn::new(capsule(), 0.2)
                })
            })
            .collect();

        run(&mut world, 300);

        let depth = deepest_contacts(&world).pair;
        assert!(
            depth <= 0.005,
            "the capsules still overlap by {} mm",
            depth * 1000.0
        );
        for id in ids {
            assert!(read(&world, id).asleep, "a capsule is awake");
        }
    }

    fn is_finite(state: &LooseState) -> bool {
        state.position.is_finite()
            && state.rotation.is_finite()
            && state.linear_velocity.is_finite()
            && state.angular_velocity.is_finite()
    }

    #[test]
    fn a_box_is_never_braked() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, FLAT_BOX.y, 0.0),
            linear_velocity: Vec3::new(1.5, 0.0, 0.0),
            ..LooseSpawn::new(cuboid(FLAT_BOX), 0.2)
        };
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        for tick in 1..=60 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
        }
    }

    #[test]
    fn a_capsule_sliding_along_its_axis_is_not_braked() {
        let travel = |rest_slope: Option<f32>| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            if let Some(slope) = rest_slope {
                world.set_rest_slope(slope);
            }
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(-5.0, CAPSULE_RADIUS, 0.0),
                rotation: lying_across(),
                linear_velocity: Vec3::new(2.0, 0.0, 0.0),
                ..LooseSpawn::new(capsule(), 0.2)
            });
            run(&mut world, 60);
            read(&world, id).position.x + 5.0
        };

        let braked = travel(None);
        let free = travel(Some(0.0));

        assert!(free > 0.1, "the capsule slid {free} m");
        assert!(
            (braked - free).abs() <= 0.001,
            "braked slid {braked} m, free slid {free} m"
        );
    }

    #[test]
    fn a_capsule_rolling_on_a_box_top_stops_on_it() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.1, 0.0),
            ..LooseSpawn::new(cuboid(Vec3::new(1.0, 0.1, 1.0)), 50.0)
        });
        run(&mut world, 120);
        let start = Vec3::new(0.0, 0.2 + CAPSULE_RADIUS, -0.5);
        let id = world.spawn_loose(&rolling_capsule(start, 0.5));

        run(&mut world, 300);

        let state = read(&world, id);
        let rolled = state.position.distance(start);
        assert!(rolled < 0.3, "rolled {rolled} m on the box top, {state:?}");
        assert!(
            state.position.y > 0.2 + CAPSULE_RADIUS - 0.005,
            "the capsule left the box top, {state:?}"
        );
        assert!(state.asleep, "the capsule is awake, {state:?}");
    }

    #[test]
    fn zero_gravity_turns_the_brake_off() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        braked.set_gravity(0.0);
        free.set_gravity(0.0);
        free.set_rest_slope(0.0);
        let spawn = rolling_capsule(Vec3::new(0.0, CAPSULE_RADIUS, -5.0), 2.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        for tick in 1..=60 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            assert!(
                is_finite(&a),
                "tick {tick}: braked state is not finite, {a:?}"
            );
            assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
        }
    }

    #[test]
    fn an_upright_capsule_spinning_on_its_end_comes_to_rest() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_HALF_HEIGHT + CAPSULE_RADIUS, 0.0),
            angular_velocity: Vec3::new(0.0, 10.0, 0.0),
            ..LooseSpawn::new(capsule(), 0.2)
        });
        let mut previous = read(&world, id).angular_velocity.length();
        let mut asleep_at = None;

        for tick in 1..=300 {
            world.step(DT);
            let state = read(&world, id);
            assert!(is_finite(&state), "tick {tick}: not finite, {state:?}");
            let spin = state.angular_velocity.length();
            assert!(
                spin <= previous + 1e-5,
                "tick {tick}: spin rose from {previous} to {spin}, {state:?}"
            );
            previous = spin;
            if state.asleep && asleep_at.is_none() {
                asleep_at = Some(tick);
            }
        }

        let state = read(&world, id);
        assert!(asleep_at.is_some(), "not asleep after 300 ticks, {state:?}");
    }

    #[test]
    fn a_steep_rest_slope_stops_a_roll_without_reversing_it() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_rest_slope(1.5);
        let id = world.spawn_loose(&rolling_capsule(Vec3::new(0.0, CAPSULE_RADIUS, -5.0), 2.0));
        let mut asleep_at = None;

        for tick in 1..=120 {
            world.step(DT);
            let state = read(&world, id);
            assert!(
                state.linear_velocity.z >= -1e-4,
                "tick {tick}: the roll reversed, {state:?}"
            );
            if state.asleep && asleep_at.is_none() {
                asleep_at = Some(tick);
            }
        }

        assert!(
            asleep_at.is_some(),
            "not asleep after 120 ticks, {:?}",
            read(&world, id)
        );
    }

    #[test]
    fn a_standing_cylinder_sliding_is_not_braked() {
        let slid = |rest_slope: Option<f32>| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            if let Some(slope) = rest_slope {
                world.set_rest_slope(slope);
            }
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(-5.0, CYLINDER_HALF_HEIGHT, 0.0),
                linear_velocity: Vec3::new(1.5, 0.0, 0.0),
                ..LooseSpawn::new(cylinder(), 0.2)
            });
            run(&mut world, 60);
            read(&world, id).position.x + 5.0
        };

        let braked = slid(None);
        let free = slid(Some(0.0));

        assert!(free > 0.05, "the cylinder slid {free} m");
        assert!(
            (braked - free).abs() <= 0.001,
            "braked slid {braked} m, free slid {free} m"
        );
    }

    #[test]
    fn spawn_rejects_a_bad_mass_on_a_capsule_or_cylinder() {
        let mut rows = Vec::new();

        for (shape_name, shape) in [("capsule", capsule()), ("cylinder", cylinder())] {
            for (name, mass) in BAD_POSITIVE {
                rows.push((
                    format!("{name} mass on a {shape_name}"),
                    LooseSpawn::new(shape, mass),
                ));
            }
        }

        assert_rows_rejected(&rows, "mass must be positive and finite");
    }

    const PILE_SEEDS: [u64; 4] = [1, 7, 42, 0x776];

    /// Twenty objects of all three shapes at random poses in a 1 m square, in a column spread
    /// from 0.3 m up so they land one after another.
    fn pile_spawns(seed: u64) -> Vec<LooseSpawn> {
        let mut rng = Rng(seed);
        (0..20)
            .map(|index| {
                let shape = match index % 3 {
                    0 => cuboid(FLAT_BOX),
                    1 => capsule(),
                    _ => cylinder(),
                };
                let position = Vec3::new(
                    rng.range(-0.5, 0.5),
                    0.3 + 0.15 * index as f32 + rng.range(-0.03, 0.03),
                    rng.range(-0.5, 0.5),
                );
                let axis = Vec3::new(
                    rng.range(-1.0, 1.0),
                    rng.range(-1.0, 1.0),
                    rng.range(-1.0, 1.0),
                );
                let axis = axis.try_normalize().unwrap_or(Vec3::Y);
                LooseSpawn {
                    position,
                    rotation: Quat::from_axis_angle(axis, rng.range(0.0, std::f32::consts::TAU)),
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

            let awake = ids.iter().filter(|&&id| !read(&world, id).asleep).count();
            assert_eq!(awake, 0, "seed {seed}: {awake} of 20 objects are awake");
            let depths = deepest_contacts(&world);
            assert!(
                depths.floor <= 0.001,
                "seed {seed}: an object is {} mm into the floor",
                depths.floor * 1000.0
            );
            assert!(
                depths.pair <= 0.005,
                "seed {seed}: two objects overlap by {} mm",
                depths.pair * 1000.0
            );
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let (mut first, first_ids) = pile_world(0x776);
        let (mut second, second_ids) = pile_world(0x776);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (index, (&a, &b)) in first_ids.iter().zip(&second_ids).enumerate() {
                let (a, b) = (read(&first, a), read(&second, b));
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

        let highest = first_ids
            .iter()
            .map(|&id| read(&first, id).position.y)
            .fold(f32::MIN, f32::max);
        assert!(highest < 1.0, "the pile fell, highest centre {highest}");
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        type Make = fn(f32, f32) -> LooseShape;
        let makers: [(&str, Make); 2] = [
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

        for (shape, make) in makers {
            for (name, value) in BAD_POSITIVE {
                rows.push((
                    format!("{name} half height on a {shape}"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} radius on a {shape}"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }
}
