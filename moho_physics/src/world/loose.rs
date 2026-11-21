use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{ColliderBuilder, Pose, RigidBodyBuilder, RigidBodyHandle};
use std::f32::consts::FRAC_PI_2;

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

/// Steepest ground, radians from level, on which loose objects are braked to rest (5 degrees).
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0 * std::f32::consts::PI / 180.0;

/// A contact point deeper than this, m, means the solver is still pushing two bodies apart,
/// so the brake leaves the body alone.
const DEEP_CONTACT: f32 = 0.005;

/// A loose object's engine handle and the radius it rolls on.
#[derive(Clone, Copy, Debug)]
pub(super) struct Loose {
    body: RigidBodyHandle,
    /// `None` for a box, which does not roll.
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

    /// Steepest ground, radians from level against gravity, on which loose objects are
    /// braked to rest. 0 turns the brake off. Default 5 degrees.
    ///
    /// # Panics
    /// If `slope` is not finite or not in `[0, pi/2)`.
    pub fn set_rest_slope(&mut self, slope: f32) {
        assert!(
            slope.is_finite() && (0.0..FRAC_PI_2).contains(&slope),
            "rest slope must be finite and in [0, pi/2) radians, got {slope}"
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

    /// Brake the rolling shapes that rest on near-flat ground, on velocities only.
    ///
    /// Reads the contacts the step just computed. Awake bodies only, in id order; no body is
    /// woken.
    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let gravity = self.gravity.length();
        if self.rest_slope <= 0.0 || !(gravity.is_finite() && gravity > 0.0) {
            return;
        }
        let up = -self.gravity / gravity;
        let strength = gravity * self.rest_slope.tan();
        let flat = self.rest_slope.cos();

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
            let mut on_ground = false;
            let mut separating = false;
            for &collider in body.colliders() {
                for pair in self.narrow_phase.contact_pairs_with(collider) {
                    let into_body = if pair.collider1 == collider {
                        -1.0
                    } else {
                        1.0
                    };
                    for manifold in pair.manifolds() {
                        let normal = manifold.data.normal * into_body;
                        for point in &manifold.points {
                            separating |= point.dist < -DEEP_CONTACT;
                            on_ground |= point.dist <= 0.0 && normal.dot(up) >= flat;
                        }
                    }
                }
            }
            if !on_ground || separating {
                continue;
            }

            let axis = *body.rotation() * Vec3::Y;
            let mut linvel = body.linvel();
            let mut angvel = body.angvel();
            let spin = angvel.dot(axis);
            let spin_cut = strength * dt / radius;
            angvel += axis * (spin.signum() * (spin.abs() - spin_cut).max(0.0) - spin);
            let roll = axis.cross(up);
            if let Some(roll) = roll.try_normalize() {
                let along = linvel.dot(roll);
                let cut = strength * dt;
                linvel += roll * (along.signum() * (along.abs() - cut).max(0.0) - along);
            }
            if let Some(body) = self.rigid_body_set.get_mut(loose.body) {
                body.set_linvel(linvel, false);
                body.set_angvel(angvel, false);
            }
        }
    }
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

    /// The radius a shape rolls on; `None` for a box, which does not roll.
    fn roll_radius(self) -> Option<f32> {
        match self {
            Self::Box { .. } => None,
            Self::Capsule { radius, .. } | Self::Cylinder { radius, .. } => Some(radius),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::test_support::{floor_mesh, slab_mesh, slope_mesh, TEST_TAG};
    use crate::world::RayQuery;
    use glam::Mat3;
    use moho_render_api::{WorldMesh, WorldMeshId};
    use proptest::prelude::*;
    use std::f32::consts::FRAC_PI_2;

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
            let mass = world.rigid_body_set[world.loose[&id].body].mass();
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

    const CAPSULE_RADIUS: f32 = 0.04;
    const CAPSULE: LooseShape = LooseShape::Capsule {
        half_height: 0.1,
        radius: CAPSULE_RADIUS,
    };
    const CYLINDER_RADIUS: f32 = 0.05;
    const CYLINDER: LooseShape = LooseShape::Cylinder {
        half_height: 0.08,
        radius: CYLINDER_RADIUS,
    };

    fn all_shapes() -> [(&'static str, LooseShape); 3] {
        [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", CAPSULE),
            ("cylinder", CYLINDER),
        ]
    }

    /// A seeded linear congruential generator, so every run drops the same objects.
    struct Lcg(u64);

    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
        }

        /// Uniform in `[lo, hi)`.
        fn range(&mut self, lo: f32, hi: f32) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let top = u16::try_from(self.0 >> 48).expect("48 shifted bits fit 16");
            lo + (hi - lo) * f32::from(top) / 65_536.0
        }

        fn rotation(&mut self) -> Quat {
            let axis = Vec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            )
            .try_normalize()
            .unwrap_or(Vec3::Y);
            Quat::from_axis_angle(axis, self.range(0.0, std::f32::consts::PI))
        }
    }

    /// How far below the object's centre its lowest point reaches, negated: the height of the
    /// lowest point of the shape above y = 0.
    fn lowest_point(shape: LooseShape, state: &LooseState) -> f32 {
        let rotation = Mat3::from_quat(state.rotation);
        let axis_y = (rotation * Vec3::Y).y.abs();
        let reach = match shape {
            LooseShape::Box { half_extents } => (0..3)
                .map(|i| rotation.col(i).y.abs() * half_extents[i])
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

    fn seeded_drop(shape: LooseShape, seed: u64) -> LooseSpawn {
        let mut rng = Lcg::new(seed);
        LooseSpawn {
            position: Vec3::new(0.0, 1.0, 0.0),
            rotation: rng.rotation(),
            linear_velocity: Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0)),
            angular_velocity: Vec3::new(
                rng.range(-20.0, 20.0),
                rng.range(-20.0, 20.0),
                rng.range(-20.0, 20.0),
            ),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// A capsule lying along z on the floor at y = 0, rolling at `speed` along +x without
    /// slipping.
    fn rolling_capsule(speed: f32) -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_RADIUS, 0.0),
            rotation: Quat::from_rotation_x(FRAC_PI_2),
            linear_velocity: Vec3::new(speed, 0.0, 0.0),
            angular_velocity: Vec3::new(0.0, 0.0, -speed / CAPSULE_RADIUS),
            ..LooseSpawn::new(CAPSULE, 0.2)
        }
    }

    /// A world whose ground through the origin rises toward +z at `degrees`; downhill is -z.
    fn sloped_world(degrees: f32) -> PhysicsWorld {
        let rise = degrees.to_radians().tan() * 10.0;
        world_with_floor(&slope_mesh(-10.0, -rise, 10.0, rise))
    }

    /// A capsule or cylinder resting on that ground at the origin, its axis along x, across
    /// the slope.
    fn lying_across_slope(shape: LooseShape, radius: f32, degrees: f32) -> LooseSpawn {
        let angle = degrees.to_radians();
        let normal = Vec3::new(0.0, angle.cos(), -angle.sin());
        LooseSpawn {
            position: normal * radius,
            rotation: Quat::from_rotation_z(-FRAC_PI_2),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// How far the object ends from where it was spawned, and whether it sleeps.
    fn settle_on_slope(world: &mut PhysicsWorld, spawn: &LooseSpawn, ticks: u32) -> (f32, bool) {
        let id = world.spawn_loose(spawn);
        run(world, ticks);
        let state = read(world, id);
        ((state.position - spawn.position).length(), state.asleep)
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in all_shapes() {
            for seed in 0..8 {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&seeded_drop(shape, seed));

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = lowest_point(shape, &state);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} seed {seed}: lowest point {bottom} m above the floor, {state:?}"
                );
                assert!(state.asleep, "{name} seed {seed}: not asleep, {state:?}");
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawn = rolling_capsule(2.0);
        let id = world.spawn_loose(&spawn);
        let mut previous = spawn.linear_velocity.length();

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
        let travelled = (state.position - spawn.position).length();
        assert!(travelled <= 1.5, "rolled {travelled} m before stopping");
        assert!(travelled > 0.3, "it rolled before stopping, {travelled} m");
        assert!(state.asleep, "the capsule rests, {state:?}");
    }

    #[test]
    fn the_rest_slope_decides() {
        let rows = [
            ("capsule", CAPSULE, CAPSULE_RADIUS, 2.0, false),
            ("capsule", CAPSULE, CAPSULE_RADIUS, 4.9, false),
            ("cylinder", CYLINDER, CYLINDER_RADIUS, 4.9, false),
            ("capsule", CAPSULE, CAPSULE_RADIUS, 5.5, true),
            ("cylinder", CYLINDER, CYLINDER_RADIUS, 10.0, true),
        ];

        for (name, shape, radius, degrees, rolls) in rows {
            let mut world = sloped_world(degrees);
            let spawn = lying_across_slope(shape, radius, degrees);

            let (moved, asleep) = settle_on_slope(&mut world, &spawn, 300);

            if rolls {
                assert!(
                    moved > 1.0,
                    "{name} on {degrees} degrees rolled only {moved} m"
                );
            } else {
                assert!(moved < 0.02, "{name} on {degrees} degrees moved {moved} m");
                assert!(asleep, "{name} on {degrees} degrees is not asleep");
            }
        }
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let mut steeper = sloped_world(8.0);
        steeper.set_rest_slope(10.0_f32.to_radians());
        let (moved, asleep) = settle_on_slope(
            &mut steeper,
            &lying_across_slope(CAPSULE, CAPSULE_RADIUS, 8.0),
            300,
        );
        assert!(
            moved < 0.02,
            "moved {moved} m on 8 degrees at a 10 degree rest slope"
        );
        assert!(
            asleep,
            "the capsule sleeps on 8 degrees at a 10 degree rest slope"
        );

        let mut off = world_with_floor(&floor_mesh(0.0));
        off.set_rest_slope(0.0);
        let id = off.spawn_loose(&rolling_capsule(2.0));
        run(&mut off, 120);

        let state = read(&off, id);
        assert_eq!(off.rest_slope(), 0.0);
        assert!(
            state.linear_velocity.length() > 1.5,
            "with the brake off the capsule keeps rolling, {state:?}"
        );
        assert!(!state.asleep, "{state:?}");
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let rejected = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.01),
            ("a quarter turn", FRAC_PI_2),
            ("past a quarter turn", FRAC_PI_2 + 0.01),
            ("four radians", 4.0),
        ];

        for (name, slope) in rejected {
            let mut world = PhysicsWorld::new();
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }))
            .is_err();
            assert!(panicked, "{name} rest slope was accepted");
        }
        for slope in [0.0, 0.5, FRAC_PI_2 - 0.001] {
            let mut world = PhysicsWorld::new();
            world.set_rest_slope(slope);
            assert_eq!(world.rest_slope(), slope);
        }
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        for (name, shape) in all_shapes() {
            let spawn = LooseSpawn {
                position: Vec3::new(0.0, 0.3, 0.0),
                linear_velocity: Vec3::new(1.0, 0.0, -0.5),
                angular_velocity: Vec3::new(3.0, 5.0, -2.0),
                ..LooseSpawn::new(shape, 0.2)
            };
            let mut braked = world_with_floor(&floor_mesh(0.0));
            let mut free = world_with_floor(&floor_mesh(0.0));
            free.set_rest_slope(0.0);
            let braked_id = braked.spawn_loose(&spawn);
            let free_id = free.spawn_loose(&spawn);

            let mut tick = 0;
            let mut landed = false;
            while !landed && tick < 120 {
                braked.step(DT);
                free.step(DT);
                tick += 1;
                let (with, without) = (read(&braked, braked_id), read(&free, free_id));
                assert_eq!(
                    with.linear_velocity.y.to_bits(),
                    without.linear_velocity.y.to_bits(),
                    "{name} tick {tick}: vertical velocity differs"
                );
                landed = lowest_point(shape, &without) <= 0.0;
            }

            assert!(landed, "{name} reached the floor");
        }
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        let (half_height, radius) = (0.08, CYLINDER_RADIUS);
        let one_degree = 1.0_f32.to_radians();

        for degrees in [10.0_f32, 15.0, 20.0, 25.0, 27.0, 30.0, 35.0, 40.0, 45.0] {
            let tilt = degrees.to_radians();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(
                    0.0,
                    half_height * tilt.cos() + radius * tilt.sin() + 0.002,
                    0.0,
                ),
                rotation: Quat::from_rotation_z(tilt),
                ..LooseSpawn::new(CYLINDER, 0.2)
            });

            run(&mut world, 600);

            let state = read(&world, id);
            let axis_y = (state.rotation * Vec3::Y).y.abs();
            assert!(state.asleep, "{degrees} degrees: not asleep, {state:?}");
            assert!(
                axis_y >= one_degree.cos() || axis_y <= one_degree.sin(),
                "{degrees} degrees: neither standing nor lying, axis y {axis_y}, {state:?}"
            );
            let bottom = lowest_point(CYLINDER, &state);
            assert!(
                bottom.abs() <= 0.001,
                "{degrees} degrees: lowest point {bottom} m above the floor"
            );
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_RADIUS + 0.019, 0.0),
            ..rolling_capsule(2.0)
        };
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        run(&mut braked, 2);
        run(&mut free, 2);

        assert_eq!(
            bits(&read(&braked, braked_id)),
            bits(&read(&free, free_id)),
            "a predicted contact is not ground"
        );
    }

    /// The deepest any two loose objects overlap, from the contacts of the last step.
    fn deepest_loose_pair_overlap(world: &PhysicsWorld) -> f32 {
        let is_loose = |handle: rapier3d::prelude::ColliderHandle| {
            world.collider_set[handle].parent().is_some()
        };
        let mut deepest = 0.0_f32;
        for pair in world.narrow_phase.contact_pairs() {
            if is_loose(pair.collider1) && is_loose(pair.collider2) {
                for manifold in pair.manifolds() {
                    for point in &manifold.points {
                        deepest = deepest.max(-point.dist);
                    }
                }
            }
        }
        deepest
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let side_by_side = |x: f32| LooseSpawn {
            position: Vec3::new(x, CAPSULE_RADIUS, 0.0),
            rotation: Quat::from_rotation_x(FRAC_PI_2),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let left = world.spawn_loose(&side_by_side(0.0));
        let right = world.spawn_loose(&side_by_side(2.0 * CAPSULE_RADIUS - 0.01));

        run(&mut world, 300);

        let overlap = deepest_loose_pair_overlap(&world);
        assert!(overlap <= 0.005, "still {overlap} m overlapped");
        let gap = (read(&world, right).position - read(&world, left).position).length();
        assert!(
            gap >= 2.0 * CAPSULE_RADIUS - 0.005,
            "the capsules' centres are {gap} m apart"
        );
        assert!(read(&world, left).asleep && read(&world, right).asleep);
    }

    fn pile_spawns(seed: u64) -> Vec<LooseSpawn> {
        let mut rng = Lcg::new(seed);
        (0u8..20)
            .map(|index| {
                let shape = match index % 3 {
                    0 => cuboid(Vec3::new(
                        rng.range(0.03, 0.07),
                        rng.range(0.03, 0.07),
                        rng.range(0.03, 0.07),
                    )),
                    1 => LooseShape::Capsule {
                        half_height: rng.range(0.04, 0.08),
                        radius: rng.range(0.03, 0.05),
                    },
                    _ => LooseShape::Cylinder {
                        half_height: rng.range(0.04, 0.08),
                        radius: rng.range(0.03, 0.05),
                    },
                };
                LooseSpawn {
                    position: Vec3::new(
                        rng.range(-0.5, 0.5),
                        0.3 + 0.3 * f32::from(index),
                        rng.range(-0.5, 0.5),
                    ),
                    rotation: rng.rotation(),
                    linear_velocity: Vec3::new(rng.range(-1.0, 1.0), 0.0, rng.range(-1.0, 1.0)),
                    angular_velocity: Vec3::new(
                        rng.range(-5.0, 5.0),
                        rng.range(-5.0, 5.0),
                        rng.range(-5.0, 5.0),
                    ),
                    ..LooseSpawn::new(shape, 0.2)
                }
            })
            .collect()
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [1, 2, 3, 4] {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let spawns = pile_spawns(seed);
            let ids: Vec<_> = spawns.iter().map(|s| world.spawn_loose(s)).collect();

            run(&mut world, 600);

            for (id, spawn) in ids.iter().zip(&spawns) {
                let state = read(&world, *id);
                assert!(state.asleep, "seed {seed}: {id:?} awake, {state:?}");
                let bottom = lowest_point(spawn.shape, &state);
                assert!(
                    bottom >= -0.001,
                    "seed {seed}: {id:?} is {} m into the floor",
                    -bottom
                );
            }
            let overlap = deepest_loose_pair_overlap(&world);
            assert!(
                overlap <= 0.005,
                "seed {seed}: two objects overlap by {overlap} m"
            );
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let spawns = pile_spawns(7);
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

        let landed = first_ids
            .iter()
            .all(|id| read(&first, *id).position.y < 1.0);
        assert!(landed, "every object came down");
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        let mut rows = Vec::new();
        for (name, value) in BAD_POSITIVE {
            for (kind, build) in [
                ("capsule", capsule_of as fn(f32, f32) -> LooseShape),
                ("cylinder", cylinder_of),
            ] {
                rows.push((
                    format!("{name} {kind} half height"),
                    LooseSpawn::new(build(value, 0.04), 1.0),
                ));
                rows.push((
                    format!("{name} {kind} radius"),
                    LooseSpawn::new(build(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }

    #[test]
    fn the_brake_leaves_a_bouncing_capsules_vertical_motion_alone() {
        let spawn = LooseSpawn {
            restitution: 1.0,
            position: Vec3::new(0.0, 3.0, 0.0),
            rotation: Quat::from_rotation_x(FRAC_PI_2),
            linear_velocity: Vec3::new(1.0, 0.0, 0.0),
            angular_velocity: Vec3::new(0.0, 0.0, -1.0 / CAPSULE_RADIUS),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);
        let mut rebounds = 0;
        let mut ticks_after_second = 0;
        let mut previous_vy = 0.0_f32;

        for tick in 1..=180 {
            braked.step(DT);
            free.step(DT);
            let (with, without) = (read(&braked, braked_id), read(&free, free_id));
            assert!(
                (with.linear_velocity.y - without.linear_velocity.y).abs() <= 0.05,
                "tick {tick}: vertical velocity {} braked, {} free",
                with.linear_velocity.y,
                without.linear_velocity.y
            );
            assert!(
                (with.position.y - without.position.y).abs() <= 0.005,
                "tick {tick}: height {} braked, {} free",
                with.position.y,
                without.position.y
            );
            if without.linear_velocity.y - previous_vy > 0.5 {
                rebounds += 1;
            }
            previous_vy = without.linear_velocity.y;
            if rebounds >= 2 {
                ticks_after_second += 1;
                if ticks_after_second == 5 {
                    break;
                }
            }
        }

        assert!(rebounds >= 2, "the capsule bounced {rebounds} times");
    }

    #[test]
    fn the_rest_slope_scales_with_gravity() {
        let mut heavy = sloped_world(4.0);
        heavy.set_gravity(36.0);
        let (moved, asleep) = settle_on_slope(
            &mut heavy,
            &lying_across_slope(CAPSULE, CAPSULE_RADIUS, 4.0),
            300,
        );
        assert!(moved < 0.02, "moved {moved} m on 4 degrees at gravity 36");
        assert!(asleep, "the capsule sleeps on 4 degrees at gravity 36");

        let mut light = world_with_floor(&floor_mesh(0.0));
        light.set_gravity(9.0);
        let spawn = rolling_capsule(2.0);
        let id = light.spawn_loose(&spawn);
        run(&mut light, 600);

        let state = read(&light, id);
        let travelled = (state.position - spawn.position).length();
        assert!(
            travelled > 1.5,
            "at gravity 9 the capsule rolls on, travelled {travelled} m"
        );
        assert!(state.asleep, "it still comes to rest, {state:?}");
    }

    #[test]
    fn a_capsule_rolling_on_a_loose_box_is_braked() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let slab_half = Vec3::new(1.0, 0.05, 1.0);
        world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, slab_half.y, 0.0),
            ..LooseSpawn::new(cuboid(slab_half), 50.0)
        });
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 2.0 * slab_half.y + CAPSULE_RADIUS, 0.0),
            ..rolling_capsule(1.0)
        };
        let id = world.spawn_loose(&spawn);

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(state.asleep, "the capsule rests on the box, {state:?}");
        assert!(
            state.position.x > 0.0 && state.position.x < 0.6,
            "it stopped on the box after a short roll, {state:?}"
        );
        assert!(
            (state.position.y - (2.0 * slab_half.y + CAPSULE_RADIUS)).abs() <= 0.002,
            "it stays on top of the box, {state:?}"
        );
    }

    #[test]
    fn a_sliding_box_is_not_braked() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, FLAT_BOX.y, 0.0),
            linear_velocity: Vec3::new(1.5, 0.0, 0.0),
            ..LooseSpawn::new(cuboid(FLAT_BOX), 0.2)
        };
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);

        for tick in 1..=60 {
            braked.step(DT);
            free.step(DT);
            assert_eq!(
                bits(&read(&braked, braked_id)),
                bits(&read(&free, free_id)),
                "tick {tick}: the brake changed a box"
            );
        }

        assert!(read(&braked, braked_id).position.x > 0.05, "the box slid");
    }

    #[test]
    fn a_cylinder_standing_on_its_flat_end_rests_and_sleeps() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.08, 0.0),
            ..LooseSpawn::new(CYLINDER, 0.2)
        });

        run(&mut world, 120);

        let state = read(&world, id);
        assert!(state.asleep, "{state:?}");
        assert!(
            (state.position.y - 0.08).abs() <= 0.001,
            "it stands on its half height, y={}",
            state.position.y
        );
    }

    #[test]
    fn a_capsule_in_zero_gravity_is_not_braked() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_gravity(0.0);
        let id = world.spawn_loose(&rolling_capsule(1.0));

        run(&mut world, 60);

        let state = read(&world, id);
        assert!(
            state.position.is_finite()
                && state.linear_velocity.is_finite()
                && state.angular_velocity.is_finite(),
            "{state:?}"
        );
        assert!(
            state.linear_velocity.length() > 0.9,
            "with no gravity there is no up to brake against, {state:?}"
        );
    }

    #[test]
    fn raising_the_rest_slope_leaves_a_sleeper_asleep_and_unchanged() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_capsule(1.0));
        let mut ticks = 0;
        while ticks < 300 && !read(&world, id).asleep {
            world.step(DT);
            ticks += 1;
        }
        assert!(
            read(&world, id).asleep,
            "the capsule sleeps before the change"
        );
        let snapshot = bits(&read(&world, id));

        world.set_rest_slope(10.0_f32.to_radians());
        run(&mut world, 60);

        let state = read(&world, id);
        assert!(state.asleep, "the brake never wakes a sleeper, {state:?}");
        assert_eq!(bits(&state), snapshot, "the brake never changes a sleeper");
    }

    #[test]
    fn a_capsule_lying_down_the_slope_neither_rolls_nor_creeps() {
        let degrees = 4.0_f32;
        let angle = degrees.to_radians();
        let mut world = sloped_world(degrees);
        let normal = Vec3::new(0.0, angle.cos(), -angle.sin());
        let spawn = LooseSpawn {
            position: normal * CAPSULE_RADIUS,
            rotation: Quat::from_rotation_x(FRAC_PI_2 - angle),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };

        let (moved, asleep) = settle_on_slope(&mut world, &spawn, 300);

        assert!(moved < 0.02, "moved {moved} m lengthwise on 4 degrees");
        assert!(asleep, "the capsule sleeps lengthwise on 4 degrees");
    }

    #[test]
    fn a_standing_cylinder_spun_about_its_axis_comes_to_rest() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.08, 0.0),
            angular_velocity: Vec3::new(0.0, 6.0, 0.0),
            ..LooseSpawn::new(CYLINDER, 0.2)
        });

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(
            state.position.is_finite()
                && state.linear_velocity.is_finite()
                && state.angular_velocity.is_finite(),
            "{state:?}"
        );
        assert!(state.asleep, "the spin dies out, {state:?}");
        assert!(
            (state.position.y - 0.08).abs() <= 0.001,
            "it still stands, y={}",
            state.position.y
        );
    }

    fn capsule_of(half_height: f32, radius: f32) -> LooseShape {
        LooseShape::Capsule {
            half_height,
            radius,
        }
    }

    fn cylinder_of(half_height: f32, radius: f32) -> LooseShape {
        LooseShape::Cylinder {
            half_height,
            radius,
        }
    }
}
