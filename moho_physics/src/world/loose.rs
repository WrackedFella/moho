use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, ContactPair, Pose, RigidBody, RigidBodyBuilder,
    RigidBodyHandle,
};
use std::f32::consts::FRAC_PI_2;

/// The rest slope of a world that never calls [`PhysicsWorld::set_rest_slope`]: 5 degrees.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0_f32.to_radians();

/// A contact point deeper than this, m, means two bodies are still being pushed apart, so the
/// brake leaves both of them, and any body touching either, alone: braking would stop the
/// solver's separating velocity and freeze the overlap.
const DEEP_CONTACT: f32 = 0.005;

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

/// A loose object's engine side: its body and, for a shape that rolls, the radius it rolls on.
pub(super) struct Loose {
    pub(super) body: RigidBodyHandle,
    /// `None` for a box, which does not roll.
    roll_radius: Option<f32>,
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
    /// Capsules and cylinders touching ground within the rest slope of level are braked to rest;
    /// see [`PhysicsWorld::set_rest_slope`]. Boxes are not braked.
    ///
    /// Characters do not ignore loose objects: a loose object blocks a character or is
    /// stepped onto like geometry, and any overlap shoves the object because the solver treats
    /// the character as infinitely heavy. Rays skip loose objects because their colliders
    /// carry no tag.
    ///
    /// # Panics
    /// If the spawn would poison the simulation: a mass, half extent, half height or radius that
    /// is not positive and finite, a friction, restitution or damping that is negative or not
    /// finite, a position or velocity that is not finite, or a rotation that is not finite or
    /// whose squared length is more than 2e-4 from 1 (glam's `Quat::is_normalized`).
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

    /// Steepest ground, radians from level against gravity, on which loose objects are
    /// braked to rest. 0 turns the brake off. Default 5°.
    ///
    /// # Panics
    /// If `slope` is not finite or not in `[0, π/2)`.
    pub fn set_rest_slope(&mut self, slope: f32) {
        assert!(
            slope.is_finite() && (0.0..FRAC_PI_2).contains(&slope),
            "rest slope must be finite and in [0, pi/2), got {slope}"
        );
        self.rest_slope = slope;
    }

    /// The rest slope in radians; see [`PhysicsWorld::set_rest_slope`].
    pub fn rest_slope(&self) -> f32 {
        self.rest_slope
    }

    /// Slow rolling shapes that touch near-flat ground, so they come to rest instead of
    /// rolling for ever. Runs after the step and changes velocities only.
    ///
    /// With `a = g * tan(rest_slope)`, the spin about the shape's axis loses `a * dt / r` and
    /// the linear velocity across the axis and up loses `a * dt`, each stopping at zero. The
    /// velocity along up and tipping in the plane of the axis and up are untouched.
    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let Some(up) = (-self.gravity).try_normalize() else {
            return;
        };
        let cos_rest = self.rest_slope.cos();
        let decel = self.gravity.length() * self.rest_slope.tan() * dt;
        if decel <= 0.0 {
            return;
        }

        for loose in self.loose.values() {
            let Some(radius) = loose.roll_radius else {
                continue;
            };
            let Some(body) = self.rigid_body_set.get(loose.body) else {
                continue;
            };
            if body.is_sleeping() || !self.on_near_flat_ground(body, up, cos_rest) {
                continue;
            }

            let axis = *body.rotation() * Vec3::Y;
            let mut linvel = body.linvel();
            let mut angvel = body.angvel();

            let spin = angvel.dot(axis);
            angvel += axis * (shrink_toward_zero(spin, decel / radius) - spin);

            if let Some(across) = axis.cross(up).try_normalize() {
                let speed = linvel.dot(across);
                linvel += across * (shrink_toward_zero(speed, decel) - speed);
            }

            if let Some(body) = self.rigid_body_set.get_mut(loose.body) {
                body.set_linvel(linvel, false);
                body.set_angvel(angvel, false);
            }
        }
    }

    /// Whether `body` has a contact point at or inside a surface whose normal is within the
    /// rest slope of `up`, and neither it nor any awake body it touches has a contact point
    /// deeper than [`DEEP_CONTACT`].
    ///
    /// The neighbour clause matters because a braked body is a wall that cannot give way: a
    /// body pinned between two braked neighbours cannot be pushed out of an overlap, and
    /// sleeps in it.
    fn on_near_flat_ground(&self, body: &RigidBody, up: Vec3, cos_rest: f32) -> bool {
        let mut grounded = false;
        for &collider in body.colliders() {
            if self.is_pushed_apart(collider) {
                return false;
            }
            for pair in self.narrow_phase.contact_pairs_with(collider) {
                let (other, into_body) = if pair.collider1 == collider {
                    (pair.collider2, -1.0)
                } else {
                    (pair.collider1, 1.0)
                };
                let mut touching = false;
                for manifold in pair.manifolds() {
                    for point in &manifold.points {
                        if point.dist <= 0.0 {
                            touching = true;
                            if (manifold.data.normal * into_body).dot(up) >= cos_rest {
                                grounded = true;
                            }
                        }
                    }
                }
                if touching && self.is_awake_and_pushed_apart(other) {
                    return false;
                }
            }
        }
        grounded
    }

    /// Whether `collider` belongs to an awake body that [`Self::is_pushed_apart`]; a sleeping
    /// body's manifolds are stale, and a world mesh has no body.
    fn is_awake_and_pushed_apart(&self, collider: ColliderHandle) -> bool {
        self.collider_set[collider]
            .parent()
            .and_then(|handle| self.rigid_body_set.get(handle))
            .is_some_and(|body| !body.is_sleeping())
            && self.is_pushed_apart(collider)
    }

    /// Whether `collider` has a contact point deeper than [`DEEP_CONTACT`].
    fn is_pushed_apart(&self, collider: ColliderHandle) -> bool {
        self.narrow_phase
            .contact_pairs_with(collider)
            .flat_map(ContactPair::manifolds)
            .flat_map(|manifold| &manifold.points)
            .any(|point| point.dist < -DEEP_CONTACT)
    }
}

/// `value` moved `by` closer to zero, stopping at zero.
fn shrink_toward_zero(value: f32, by: f32) -> f32 {
    value.signum() * (value.abs() - by).max(0.0)
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
            assert_eq!(world.loose_count(), 1);
            let (_, body) = world
                .rigid_body_set
                .iter()
                .next()
                .expect("the spawn is the world's only body");
            let mass = body.mass();
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

    const CAPSULE: LooseShape = LooseShape::Capsule {
        half_height: 0.1,
        radius: 0.04,
    };
    const CYLINDER: LooseShape = LooseShape::Cylinder {
        half_height: 0.08,
        radius: 0.05,
    };

    fn all_shapes() -> [(&'static str, LooseShape); 3] {
        [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", CAPSULE),
            ("cylinder", CYLINDER),
        ]
    }

    fn roll_radius(shape: LooseShape) -> f32 {
        match shape {
            LooseShape::Capsule { radius, .. } | LooseShape::Cylinder { radius, .. } => radius,
            LooseShape::Box { .. } => panic!("a box does not roll"),
        }
    }

    /// Local Y turned onto world X, so the shape lies across a slope that falls along z.
    fn lying() -> Quat {
        Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)
    }

    /// Height of the shape's lowest point above the floor at y = 0.
    fn lowest_point(shape: LooseShape, state: &LooseState) -> f32 {
        let up_component = (state.rotation * Vec3::Y).y.abs();
        let reach = match shape {
            LooseShape::Box { half_extents } => [Vec3::X, Vec3::Y, Vec3::Z]
                .iter()
                .enumerate()
                .map(|(i, axis)| (state.rotation * *axis).y.abs() * half_extents[i])
                .sum::<f32>(),
            LooseShape::Capsule {
                half_height,
                radius,
            } => half_height * up_component + radius,
            LooseShape::Cylinder {
                half_height,
                radius,
            } => {
                half_height * up_component
                    + radius * (1.0 - up_component * up_component).max(0.0).sqrt()
            }
        };
        state.position.y - reach
    }

    /// A deterministic stream of values in `[0, 1)`.
    struct Seeded(u64);

    impl Seeded {
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 40) as f32 / 16_777_216.0
        }

        fn signed(&mut self, magnitude: f32) -> f32 {
            (self.next() * 2.0 - 1.0) * magnitude
        }
    }

    /// A world whose ground rises along +z at `degrees`, long enough to roll 25 m on.
    fn slope_world(degrees: f32) -> PhysicsWorld {
        let rise = degrees.to_radians().tan();
        world_with_floor(&slope_mesh(-30.0, 0.0, 30.0, 60.0 * rise))
    }

    /// The shape lying across the slope of [`slope_world`] at z = 0, 1 mm above the surface.
    fn lying_on_slope(shape: LooseShape, degrees: f32) -> LooseSpawn {
        let tilt = degrees.to_radians();
        let normal = Vec3::new(0.0, tilt.cos(), -tilt.sin());
        let surface = Vec3::new(0.0, 30.0 * tilt.tan(), 0.0);
        LooseSpawn {
            position: surface + normal * (roll_radius(shape) + 0.001),
            rotation: lying(),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// A capsule lying on level ground at y = 0, rolling along +z at `speed` m/s.
    fn rolling_capsule(speed: f32) -> LooseSpawn {
        let radius = roll_radius(CAPSULE);
        LooseSpawn {
            position: Vec3::new(0.0, radius, 0.0),
            rotation: lying(),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / radius, 0.0, 0.0),
            ..LooseSpawn::new(CAPSULE, 0.2)
        }
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in all_shapes() {
            for seed in 0..8 {
                let mut random = Seeded(0x807 + seed);
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    linear_velocity: Vec3::new(random.signed(3.0), 0.0, random.signed(3.0)),
                    angular_velocity: Vec3::new(
                        random.signed(15.0),
                        random.signed(15.0),
                        random.signed(15.0),
                    ),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(shape, &state);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} seed {seed}: lowest point {bottom} m from the floor, {state:?}"
                );
                assert!(state.asleep, "{name} seed {seed}: awake, {state:?}");
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_capsule(2.0));
        let mut previous = read(&world, id).linear_velocity.z;

        for tick in 1..=300 {
            world.step(DT);
            let speed = read(&world, id).linear_velocity.z;
            assert!(
                speed <= previous + 1e-5,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        assert!(state.asleep, "the capsule comes to rest, {state:?}");
        assert!(
            (0.8..=1.5).contains(&state.position.z),
            "it rolls about v^2/2a = 1.27 m before stopping, z={}",
            state.position.z
        );
    }

    #[test]
    fn the_default_rest_slope_is_five_degrees() {
        let world = PhysicsWorld::new();

        let slope = world.rest_slope();

        assert!(
            (slope - 5.0f32.to_radians()).abs() <= 1e-6,
            "default rest slope {slope} rad"
        );
    }

    #[test]
    fn the_rest_slope_decides() {
        let rows = [
            ("capsule", CAPSULE, 2.0, false),
            ("capsule", CAPSULE, 4.9, false),
            ("cylinder", CYLINDER, 4.9, false),
            ("capsule", CAPSULE, 5.5, true),
            ("cylinder", CYLINDER, 10.0, true),
        ];

        for (name, shape, degrees, rolls) in rows {
            let mut world = slope_world(degrees);
            let spawn = lying_on_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = (state.position - spawn.position).length();
            if rolls {
                assert!(
                    moved > 1.0,
                    "{name} on {degrees} degrees rolls more than 1 m, moved {moved}"
                );
            } else {
                assert!(
                    moved < 0.02,
                    "{name} on {degrees} degrees moves less than 2 cm, moved {moved}"
                );
                assert!(
                    state.asleep,
                    "{name} on {degrees} degrees sleeps, {state:?}"
                );
            }
        }
    }

    #[test]
    fn the_rest_slope_follows_the_worlds_gravity() {
        let rows = [
            ("capsule", CAPSULE, 4.9, false),
            ("capsule", CAPSULE, 5.5, true),
        ];

        for (name, shape, degrees, rolls) in rows {
            let mut world = slope_world(degrees);
            world.set_gravity(9.0);
            let spawn = lying_on_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let moved = (state.position - spawn.position).length();
            if rolls {
                assert!(
                    moved > 1.0,
                    "{name} on {degrees} degrees at g 9 rolls more than 1 m, moved {moved}"
                );
            } else {
                assert!(
                    moved < 0.02,
                    "{name} on {degrees} degrees at g 9 moves less than 2 cm, moved {moved}"
                );
                assert!(
                    state.asleep,
                    "{name} on {degrees} degrees at g 9 sleeps, {state:?}"
                );
            }
        }
    }

    #[test]
    fn upright_capsule_and_cylinder_rest_at_their_own_heights() {
        let rows = [
            ("capsule", CAPSULE, 0.1 + 0.04),
            ("cylinder", CYLINDER, 0.08),
        ];

        for (name, shape, centre) in rows {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, centre + 0.05, 0.0),
                ..LooseSpawn::new(shape, 0.2)
            });

            run(&mut world, 120);

            let state = read(&world, id);
            assert!(
                (state.position.y - centre).abs() <= 0.001,
                "{name} upright rests with its centre {centre} m up, y={}",
                state.position.y
            );
        }
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let mut steep_rest = slope_world(8.0);
        steep_rest.set_rest_slope(10.0f32.to_radians());
        let spawn = lying_on_slope(CAPSULE, 8.0);
        let held = steep_rest.spawn_loose(&spawn);
        let mut off = world_with_floor(&floor_mesh(0.0));
        off.set_rest_slope(0.0);
        let rolling = off.spawn_loose(&rolling_capsule(2.0));

        run(&mut steep_rest, 300);
        run(&mut off, 120);

        assert!(
            (steep_rest.rest_slope() - 10.0f32.to_radians()).abs() <= 1e-6,
            "the slope reads back, {}",
            steep_rest.rest_slope()
        );
        assert_eq!(off.rest_slope(), 0.0);
        let held = read(&steep_rest, held);
        assert!(
            held.asleep && (held.position - spawn.position).length() < 0.02,
            "10 degrees holds a capsule on 8 degrees, {held:?}"
        );
        let rolling = read(&off, rolling);
        assert!(
            rolling.linear_velocity.length() > 1.0 && rolling.position.z > 2.5,
            "with the brake off the capsule is still rolling after 2 s, {rolling:?}"
        );
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let bad = [
            ("NaN", f32::NAN),
            ("infinite", f32::INFINITY),
            ("negative", -0.1),
            ("a right angle", std::f32::consts::FRAC_PI_2),
            ("past a right angle", 2.0),
        ];

        for (name, slope) in bad {
            let mut world = PhysicsWorld::new();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));
            assert!(outcome.is_err(), "{name} rest slope was accepted");
        }
        let mut world = PhysicsWorld::new();
        world.set_rest_slope(0.0);
        assert_eq!(
            world.rest_slope(),
            0.0,
            "zero is valid and turns the brake off"
        );
        world.set_rest_slope(1.5);
        assert_eq!(world.rest_slope(), 1.5, "just under a right angle is valid");
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 0.5, 0.0),
            rotation: lying(),
            restitution: 0.5,
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);
        let mut bounce_speed = 0.0f32;

        for tick in 1..=45 {
            braked.step(DT);
            free.step(DT);
            let (a, b) = (read(&braked, braked_id), read(&free, free_id));
            bounce_speed = bounce_speed.max(b.linear_velocity.y);
            assert!(
                (a.linear_velocity.y - b.linear_velocity.y).abs() <= 1e-3,
                "tick {tick}: braked vertical velocity {} differs from free {}",
                a.linear_velocity.y,
                b.linear_velocity.y
            );
        }

        assert!(
            bounce_speed > 0.5,
            "the free capsule bounces, so contact and rebound were compared, peak {bounce_speed}"
        );
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        let (half_height, radius) = (0.08f32, 0.05f32);
        for degrees in [10.0f32, 15.0, 20.0, 27.0, 35.0, 45.0] {
            let tilt = degrees.to_radians();
            let rotation = Quat::from_rotation_z(tilt);
            let rim_depth = half_height * tilt.cos() + radius * tilt.sin();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, rim_depth + 0.001, 0.0),
                rotation,
                ..LooseSpawn::new(CYLINDER, 0.2)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            let axis_tilt = angle_from_vertical_degrees(state.rotation * Vec3::Y);
            let axis_tilt = axis_tilt.min(180.0 - axis_tilt);
            assert!(
                axis_tilt <= 1.0 || axis_tilt >= 89.0,
                "released at {degrees} degrees it ends {axis_tilt} degrees off vertical"
            );
            assert!(state.asleep, "released at {degrees} degrees it is awake");
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let mut spawn = rolling_capsule(2.0);
        spawn.position.y += 0.019;
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        run(&mut braked, 2);
        run(&mut free, 2);

        assert_eq!(
            bits(&read(&braked, braked_id)),
            bits(&read(&free, free_id)),
            "a predicted contact 1.9 cm away is not ground"
        );
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let radius = roll_radius(CAPSULE);
        let side_by_side = |z: f32| LooseSpawn {
            position: Vec3::new(0.0, radius, z),
            rotation: lying(),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let first = world.spawn_loose(&side_by_side(0.0));
        let second = world.spawn_loose(&side_by_side(2.0 * radius - 0.01));

        run(&mut world, 300);

        let (a, b) = (read(&world, first), read(&world, second));
        let apart = {
            let gap = a.position - b.position;
            gap.y.hypot(gap.z)
        };
        assert!(
            2.0 * radius - apart <= 0.005,
            "the capsules end {} m deep in each other",
            2.0 * radius - apart
        );
        assert!(a.asleep && b.asleep, "both sleep, {a:?} {b:?}");
    }

    #[test]
    fn a_capsule_spawned_before_the_floor_is_braked_like_one_spawned_after() {
        let mut after = world_with_floor(&floor_mesh(0.0));
        let after_id = after.spawn_loose(&rolling_capsule(2.0));
        let mut before = PhysicsWorld::new();
        let before_id = before.spawn_loose(&rolling_capsule(2.0));
        before.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let mut free = PhysicsWorld::new();
        free.set_rest_slope(0.0);
        let free_id = free.spawn_loose(&rolling_capsule(2.0));
        free.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);

        run(&mut after, 2);
        run(&mut before, 2);
        run(&mut free, 2);

        let braked = read(&before, before_id).linear_velocity.z;
        assert!(
            braked < read(&free, free_id).linear_velocity.z - 0.01,
            "braked {braked}"
        );
        let other_order = read(&after, after_id).linear_velocity.z;
        assert!(
            (braked - other_order).abs() < 2e-3,
            "spawn order does not change the brake: {braked} vs {other_order}"
        );
    }

    #[test]
    fn a_body_sunk_in_the_floor_elsewhere_does_not_stop_the_brake() {
        let mut alone = world_with_floor(&floor_mesh(0.0));
        let alone_id = alone.spawn_loose(&rolling_capsule(2.0));
        let mut crowded = world_with_floor(&floor_mesh(0.0));
        let crowded_id = crowded.spawn_loose(&rolling_capsule(2.0));
        crowded.spawn_loose(&LooseSpawn {
            position: Vec3::new(5.0, 0.04, 0.0),
            ..LooseSpawn::new(cube(0.05), 0.2)
        });

        run(&mut alone, 1);
        run(&mut crowded, 1);

        assert_eq!(
            read(&crowded, crowded_id).linear_velocity.z.to_bits(),
            read(&alone, alone_id).linear_velocity.z.to_bits(),
            "a cube 1 cm into the same floor is not a neighbour of the capsule"
        );
    }

    fn pile_spawns(seed: u64) -> Vec<LooseSpawn> {
        let mut random = Seeded(seed);
        let shapes = all_shapes();
        (0..20)
            .map(|i| {
                let shape = shapes[(random.next() * 3.0) as usize % 3].1;
                let axis = Vec3::new(random.signed(1.0), random.signed(1.0), random.signed(1.0))
                    .normalize_or(Vec3::Y);
                LooseSpawn {
                    position: Vec3::new(
                        random.signed(0.5),
                        0.15 + 0.12 * i as f32,
                        random.signed(0.5),
                    ),
                    rotation: Quat::from_axis_angle(axis, random.next() * std::f32::consts::TAU),
                    ..LooseSpawn::new(shape, 0.2)
                }
            })
            .collect()
    }

    /// Penetration depth recomputed from the colliders' current shapes and poses, so it
    /// cannot go stale the way a sleeping pair's cached manifold can.
    fn deepest_object_overlap(world: &PhysicsWorld) -> f32 {
        let loose: Vec<_> = world
            .collider_set
            .iter()
            .filter(|(_, collider)| collider.parent().is_some())
            .map(|(_, collider)| collider)
            .collect();
        let mut deepest = 0.0f32;
        for (i, a) in loose.iter().enumerate() {
            for b in &loose[i + 1..] {
                let contact = rapier3d::parry::query::contact(
                    a.position(),
                    a.shape(),
                    b.position(),
                    b.shape(),
                    0.0,
                );
                if let Ok(Some(contact)) = contact {
                    deepest = deepest.max(-contact.dist);
                }
            }
        }
        deepest
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [1u64, 2, 3, 4] {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let spawns = pile_spawns(seed);
            let ids: Vec<_> = spawns.iter().map(|s| world.spawn_loose(s)).collect();

            run(&mut world, 600);

            for (spawn, id) in spawns.iter().zip(&ids) {
                let state = read(&world, *id);
                assert!(state.asleep, "seed {seed}: {id:?} is awake, {state:?}");
                let bottom = lowest_point(spawn.shape, &state);
                assert!(
                    bottom >= -0.001,
                    "seed {seed}: {id:?} is {bottom} m into the floor"
                );
            }
            let overlap = deepest_object_overlap(&world);
            assert!(
                overlap <= 0.005,
                "seed {seed}: two objects overlap by {overlap} m"
            );
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let spawns = pile_spawns(1);
        let mut first = world_with_floor(&floor_mesh(0.0));
        let mut second = world_with_floor(&floor_mesh(0.0));
        let first_ids: Vec<_> = spawns.iter().map(|s| first.spawn_loose(s)).collect();
        let second_ids: Vec<_> = spawns.iter().map(|s| second.spawn_loose(s)).collect();

        for tick in 1..=300 {
            first.step(DT);
            second.step(DT);
            for (a, b) in first_ids.iter().zip(&second_ids) {
                let (a, b) = (read(&first, *a), read(&second, *b));
                assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
                assert_eq!(a.asleep, b.asleep, "tick {tick}: asleep differs");
            }
        }
        let fell = first_ids
            .iter()
            .all(|id| read(&first, *id).position.y < 2.0);
        assert!(fell, "the pile fell");
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
                    format!("{name} {shape} half height"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} {shape} radius"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
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
