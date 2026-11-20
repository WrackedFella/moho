use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{ColliderBuilder, Pose, RigidBodyBuilder, RigidBodyHandle};

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

/// Steepest ground, radians from level, on which loose objects are braked to rest by default.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0 * std::f32::consts::PI / 180.0;

/// Penetration, metres, past which a non-flat contact is being pushed apart and the brake
/// leaves the body alone, so the overlap is not frozen.
const DEEP_CONTACT: f32 = 0.005;

/// A loose body and what the brake needs to know about its shape.
pub(super) struct Loose {
    pub(super) body: RigidBodyHandle,
    /// The radius a shape rolls on, `None` for a shape that does not roll.
    roll_radius: Option<f32>,
}

/// `value` moved `amount` towards zero, stopping at zero.
fn toward_zero(value: f32, amount: f32) -> f32 {
    value.signum() * (value.abs() - amount).max(0.0)
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

    fn roll_radius(self) -> Option<f32> {
        match self {
            Self::Box { .. } => None,
            Self::Capsule { radius, .. } | Self::Cylinder { radius, .. } => Some(radius),
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
    /// Characters do not ignore loose objects: a loose box blocks a character or is stepped
    /// onto like geometry, and any overlap shoves the box because the solver treats the
    /// character as infinitely heavy; the same holds for a capsule or cylinder. Rays skip loose
    /// objects of every shape because their colliders carry no tag.
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

    /// Slow every awake loose rolling shape that touches near-flat ground, on velocities only.
    ///
    /// Cuts the spin about the shape's axis and the velocity along `axis x up` at matching
    /// rates, so a rolling body stays rolling while it slows and both reach zero together. Skips
    /// a body with a non-flat contact point deeper than [`DEEP_CONTACT`], so the solver can
    /// still separate overlapping bodies; a flat contact is exempt because the brake changes
    /// no velocity along the normal and soft contacts rest about 5 mm deep on a box top. Never
    /// wakes a sleeping body.
    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let Some(up) = (-self.gravity).try_normalize() else {
            return;
        };
        let rest_slope = self.rest_slope;
        if rest_slope == 0.0 {
            return;
        }
        let min_cos = rest_slope.cos();
        let decel = self.gravity.length() * rest_slope.tan();

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
            let mut too_deep = false;
            for &collider in body.colliders() {
                for pair in self.narrow_phase.contact_pairs_with(collider) {
                    for manifold in pair.manifolds() {
                        let into_body = if pair.collider2 == collider {
                            manifold.data.normal
                        } else {
                            -manifold.data.normal
                        };
                        let flat = into_body.dot(up) >= min_cos;
                        for point in &manifold.points {
                            too_deep |= !flat && -point.dist > DEEP_CONTACT;
                            on_ground |= flat && point.dist <= 0.0;
                        }
                    }
                }
            }
            if !on_ground || too_deep {
                continue;
            }

            let axis = *body.rotation() * Vec3::Y;
            let along = axis.cross(up).normalize_or_zero();
            let (mut linvel, mut angvel) = (body.linvel(), body.angvel());
            let spin = angvel.dot(axis);
            angvel += axis * (toward_zero(spin, decel * dt / radius) - spin);
            let travel = linvel.dot(along);
            linvel += along * (toward_zero(travel, decel * dt) - travel);

            if let Some(body) = self.rigid_body_set.get_mut(loose.body) {
                body.set_linvel(linvel, false);
                body.set_angvel(angvel, false);
            }
        }
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

    /// The steepest ground, radians from level, on which loose objects are braked to rest.
    pub fn rest_slope(&self) -> f32 {
        self.rest_slope
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
    use rapier3d::prelude::ColliderHandle;

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

    const CAPSULE: LooseShape = LooseShape::Capsule {
        half_height: 0.1,
        radius: 0.04,
    };
    const CAPSULE_RADIUS: f32 = 0.04;
    const CYLINDER: LooseShape = LooseShape::Cylinder {
        half_height: 0.08,
        radius: 0.05,
    };
    const CYLINDER_RADIUS: f32 = 0.05;
    const FLAT_BOX_SHAPE: LooseShape = LooseShape::Box {
        half_extents: FLAT_BOX,
    };

    /// A deterministic generator so every run drops the same objects.
    struct Seeded(u32);

    impl Seeded {
        fn new(seed: u32) -> Self {
            Self(seed.wrapping_mul(2_654_435_761).wrapping_add(0x9E37_79B9) | 1)
        }

        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 >> 8) as f32 / (1u32 << 24) as f32
        }

        fn range(&mut self, low: f32, high: f32) -> f32 {
            low + (high - low) * self.unit()
        }
    }

    fn bottom(world: &PhysicsWorld, id: LooseId) -> f32 {
        let body = &world.rigid_body_set[world.loose[&id].body];
        let collider = body.colliders()[0];
        world.collider_set[collider].compute_aabb().mins.y
    }

    fn lying() -> Quat {
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
    }

    /// A shape of `radius` lying along X on level ground at y = 0, rolling along +Z at `speed`
    /// without slipping.
    fn rolling_spawn(shape: LooseShape, radius: f32, speed: f32) -> LooseSpawn {
        LooseSpawn {
            position: Vec3::new(0.0, radius, 0.0),
            rotation: lying(),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / radius, 0.0, 0.0),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    fn degrees(angle: f32) -> f32 {
        angle.to_radians()
    }

    /// A world whose ground rises toward +Z at `angle` degrees, and a spawn of `shape` lying
    /// across it (axis along X) at rest, touching the surface above z = 0.
    fn slope_setup(shape: LooseShape, radius: f32, angle: f32) -> (PhysicsWorld, LooseSpawn) {
        let rise = degrees(angle).tan();
        let ground = slope_mesh(-10.0, 0.0, 10.0, 20.0 * rise);
        let world = world_with_floor(&ground);
        let normal = Vec3::new(0.0, degrees(angle).cos(), -degrees(angle).sin());
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, 10.0 * rise, 0.0) + normal * radius,
            rotation: lying(),
            ..LooseSpawn::new(shape, 0.2)
        };
        (world, spawn)
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in [
            ("box", FLAT_BOX_SHAPE),
            ("capsule", CAPSULE),
            ("cylinder", CYLINDER),
        ] {
            for seed in 0..8 {
                let mut rng = Seeded::new(seed);
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    angular_velocity: Vec3::new(
                        rng.range(-20.0, 20.0),
                        rng.range(-20.0, 20.0),
                        rng.range(-20.0, 20.0),
                    ),
                    linear_velocity: Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0)),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                assert!(state.asleep, "{name} seed {seed}: not asleep, {state:?}");
                let low = bottom(&world, id);
                assert!(
                    low.abs() <= 0.001,
                    "{name} seed {seed}: bottom is {low} from the floor"
                );
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0));
        let along_ground = |world: &PhysicsWorld| {
            let velocity = read(world, id).linear_velocity;
            Vec3::new(velocity.x, 0.0, velocity.z).length()
        };
        let mut previous = along_ground(&world);

        // The brake removes a*dt = g*tan(5 deg)*dt = 0.026 m/s a tick; 2e-3 is solver noise
        // only, an order of magnitude below that.
        for tick in 1..=300 {
            world.step(DT);
            let speed = along_ground(&world);
            assert!(
                speed <= previous + 2e-3,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        assert!(state.asleep, "the capsule stops and sleeps, {state:?}");
        let travelled = state.position.x.hypot(state.position.z);
        assert!(
            travelled <= 1.5,
            "it rests within 1.5 m, travelled {travelled}"
        );
        assert!(
            travelled > 0.5,
            "it rolled before stopping, travelled {travelled}"
        );
    }

    /// Runs 5 s on a slope of `angle` degrees at `gravity`, returning the spawn and end state.
    fn settle_on_slope(
        shape: LooseShape,
        radius: f32,
        angle: f32,
        gravity: f32,
    ) -> (LooseSpawn, LooseState) {
        let (mut world, spawn) = slope_setup(shape, radius, angle);
        world.set_gravity(gravity);
        let id = world.spawn_loose(&spawn);

        run(&mut world, 300);

        (spawn, read(&world, id))
    }

    fn assert_holds(rows: &[(&str, LooseShape, f32, f32)], gravity: f32) {
        for &(name, shape, radius, angle) in rows {
            let (spawn, state) = settle_on_slope(shape, radius, angle, gravity);

            let moved = (state.position - spawn.position).length();
            assert!(
                moved < 0.02,
                "{name} on {angle} degrees moved {moved}, expected under 2 cm"
            );
            assert!(state.asleep, "{name} on {angle} degrees is awake");
        }
    }

    fn assert_rolls_down(rows: &[(&str, LooseShape, f32, f32)], gravity: f32) {
        for &(name, shape, radius, angle) in rows {
            let (spawn, state) = settle_on_slope(shape, radius, angle, gravity);

            assert!(
                state.position.z < spawn.position.z - 1.0,
                "{name} on {angle} degrees ended at z={}, expected over 1 m downhill of {}",
                state.position.z,
                spawn.position.z
            );
        }
    }

    #[test]
    fn the_rest_slope_holds_a_shape_below_it() {
        assert_holds(
            &[
                ("capsule", CAPSULE, CAPSULE_RADIUS, 2.0),
                ("capsule", CAPSULE, CAPSULE_RADIUS, 4.9),
                ("cylinder", CYLINDER, CYLINDER_RADIUS, 4.9),
            ],
            18.0,
        );
    }

    #[test]
    fn a_shape_rolls_down_ground_above_the_rest_slope() {
        assert_rolls_down(
            &[
                ("capsule", CAPSULE, CAPSULE_RADIUS, 5.5),
                ("cylinder", CYLINDER, CYLINDER_RADIUS, 10.0),
            ],
            18.0,
        );
    }

    #[test]
    fn the_rest_slope_follows_the_gravity_input() {
        assert_holds(&[("capsule", CAPSULE, CAPSULE_RADIUS, 4.9)], 9.0);
        assert_rolls_down(&[("capsule", CAPSULE, CAPSULE_RADIUS, 5.5)], 9.0);
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let (mut steep, spawn) = slope_setup(CAPSULE, CAPSULE_RADIUS, 8.0);
        steep.set_rest_slope(degrees(10.0));
        let on_slope = steep.spawn_loose(&spawn);
        let mut flat = world_with_floor(&floor_mesh(0.0));
        flat.set_rest_slope(0.0);
        let rolling = flat.spawn_loose(&rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0));

        run(&mut steep, 300);
        run(&mut flat, 120);

        assert_eq!(steep.rest_slope(), degrees(10.0));
        assert_eq!(flat.rest_slope(), 0.0);
        let held = read(&steep, on_slope);
        assert!(
            (held.position - spawn.position).length() < 0.02 && held.asleep,
            "a 10 degree rest slope holds the capsule on 8 degrees, {held:?}"
        );
        let state = read(&flat, rolling);
        assert!(
            state.linear_velocity.z > 1.5 && !state.asleep,
            "with no brake the capsule is still rolling after 2 s, {state:?}"
        );
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let bad = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.01),
            ("pi/2", std::f32::consts::FRAC_PI_2),
            ("above pi/2", 2.0),
        ];

        for (name, slope) in bad {
            let mut world = PhysicsWorld::new();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));
            assert!(outcome.is_err(), "{name} was accepted");
        }
        let mut world = PhysicsWorld::new();
        world.set_rest_slope(0.0);
        assert_eq!(world.rest_slope(), 0.0, "zero turns the brake off");
        world.set_rest_slope(1.5);
        assert_eq!(world.rest_slope(), 1.5, "just under pi/2 is accepted");
    }

    #[test]
    fn upright_shapes_rest_at_their_collider_height() {
        let rows = [
            ("capsule", CAPSULE, 0.1 + CAPSULE_RADIUS),
            ("cylinder", CYLINDER, 0.08),
        ];

        for (name, shape, rest_height) in rows {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, rest_height + 0.01, 0.0),
                ..LooseSpawn::new(shape, 0.2)
            });

            run(&mut world, 120);

            let y = read(&world, id).position.y;
            assert!(
                (y - rest_height).abs() <= 0.001,
                "{name} rests at y={y}, expected {rest_height}"
            );
        }
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let spawn = LooseSpawn {
            restitution: 0.3,
            position: Vec3::new(0.0, 0.5, 0.0),
            ..rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0)
        };
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(&spawn);
        let free_id = free.spawn_loose(&spawn);
        let mut first_braking_tick = None;

        // The worlds only diverge once the brake acts, so up to and including that tick the
        // vertical velocities must agree exactly; after it the hops legitimately differ.
        for tick in 1..=120 {
            braked.step(DT);
            free.step(DT);
            let (b, f) = (read(&braked, braked_id), read(&free, free_id));
            assert_eq!(
                b.linear_velocity.y.to_bits(),
                f.linear_velocity.y.to_bits(),
                "tick {tick}: vertical velocity {} with the brake, {} without",
                b.linear_velocity.y,
                f.linear_velocity.y
            );
            if b.linear_velocity.x != f.linear_velocity.x
                || b.linear_velocity.z != f.linear_velocity.z
            {
                first_braking_tick = Some(tick);
                break;
            }
        }

        assert!(
            first_braking_tick.is_some(),
            "the brake never acted on the falling capsule"
        );
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        for angle in [10.0f32, 20.0, 27.0, 35.0, 45.0] {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let (half_height, radius) = (0.08, CYLINDER_RADIUS);
            let tilt = degrees(angle);
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(
                    0.0,
                    half_height * tilt.cos() + radius * tilt.sin() + 0.001,
                    0.0,
                ),
                rotation: Quat::from_rotation_x(tilt),
                ..LooseSpawn::new(CYLINDER, 0.2)
            });

            run(&mut world, 300);

            let state = read(&world, id);
            assert!(state.asleep, "{angle} degrees: still moving, {state:?}");
            let axis_tilt = angle_from_vertical_degrees(state.rotation * Vec3::Y);
            let standing = axis_tilt.min(180.0 - axis_tilt) <= 0.5;
            let lying_down = (axis_tilt - 90.0).abs() <= 0.5;
            assert!(
                standing || lying_down,
                "{angle} degrees: froze at {axis_tilt} degrees from vertical"
            );
            let low = bottom(&world, id);
            assert!(low.abs() <= 0.001, "{angle} degrees: bottom at {low}");
        }
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let after_two_ticks = |height: f32, rest_slope: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            world.set_rest_slope(rest_slope);
            let mut spawn = rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0);
            spawn.position.y += height;
            let id = world.spawn_loose(&spawn);
            run(&mut world, 2);
            read(&world, id)
        };

        let airborne_braked = after_two_ticks(0.019, degrees(5.0));
        let airborne_free = after_two_ticks(0.019, 0.0);
        let touching_braked = after_two_ticks(0.0, degrees(5.0));
        let touching_free = after_two_ticks(0.0, 0.0);

        assert_eq!(bits(&airborne_braked), bits(&airborne_free));
        assert!(
            touching_braked.linear_velocity.z < touching_free.linear_velocity.z - 0.01,
            "control: a touching capsule is braked, {} against {}",
            touching_braked.linear_velocity.z,
            touching_free.linear_velocity.z
        );
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let first = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_RADIUS, 0.0),
            rotation: lying(),
            ..LooseSpawn::new(CAPSULE, 0.2)
        });
        let second = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, CAPSULE_RADIUS, 2.0 * CAPSULE_RADIUS - 0.01),
            rotation: lying(),
            ..LooseSpawn::new(CAPSULE, 0.2)
        });

        run(&mut world, 300);

        let (a, b) = (read(&world, first), read(&world, second));
        assert!(a.asleep && b.asleep, "both sleep, {a:?} {b:?}");
        let overlap = 2.0 * CAPSULE_RADIUS - (b.position.z - a.position.z);
        assert!(overlap <= 0.005, "they end {overlap} m overlapped");
    }

    /// Twenty objects of all shapes dropped over a 1 m square, each 0.3 m above the last.
    fn pile_world(seed: u32) -> (PhysicsWorld, Vec<LooseId>) {
        let mut rng = Seeded::new(seed);
        let mut world = world_with_floor(&floor_mesh(0.0));
        let shapes = [FLAT_BOX_SHAPE, CAPSULE, CYLINDER];
        let ids = (0..20)
            .map(|index| {
                let axis = Vec3::new(
                    rng.range(-1.0, 1.0),
                    rng.range(-1.0, 1.0),
                    rng.range(-1.0, 1.0),
                );
                let axis = if axis.length() < 0.1 {
                    Vec3::Y
                } else {
                    axis.normalize()
                };
                // One object per cell of a 5 by 4 grid over the square, scattered so grid
                // neighbours differ in height; the highest release is 0.53 m, so no impact
                // exceeds about 4.4 m/s, under the 7 cm a 60 Hz tick moves it against the
                // 8 cm width of the thinnest shape.
                let cell = (index * 3) % 20;
                let centre = |slot: usize, count: usize| (slot as f32 + 0.5) / count as f32 - 0.5;
                world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(
                        centre(cell % 5, 5) + rng.range(-0.03, 0.03),
                        0.15 + 0.02 * index as f32,
                        centre(cell / 5, 4) + rng.range(-0.03, 0.03),
                    ),
                    rotation: Quat::from_axis_angle(axis, rng.range(0.0, std::f32::consts::TAU)),
                    ..LooseSpawn::new(shapes[index % 3], 0.2)
                })
            })
            .collect();
        (world, ids)
    }

    fn deepest_pair_overlap(world: &PhysicsWorld) -> f32 {
        let mut deepest = 0.0f32;
        for pair in world.narrow_phase.contact_pairs() {
            let parented = |handle: ColliderHandle| world.collider_set[handle].parent().is_some();
            if !(parented(pair.collider1) && parented(pair.collider2)) {
                continue;
            }
            for manifold in pair.manifolds() {
                for point in &manifold.points {
                    deepest = deepest.max(-point.dist);
                }
            }
        }
        deepest
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [1, 2, 3, 4] {
            let (mut world, ids) = pile_world(seed);

            run(&mut world, 600);

            for id in &ids {
                let state = read(&world, *id);
                assert!(state.asleep, "seed {seed}: {id:?} is awake, {state:?}");
                let low = bottom(&world, *id);
                assert!(low >= -0.001, "seed {seed}: {id:?} is {low} into the floor");
            }
            let overlap = deepest_pair_overlap(&world);
            assert!(overlap <= 0.005, "seed {seed}: pair overlap {overlap}");
        }
    }

    /// Two floor worlds, one with the default rest slope and one with the brake off, each
    /// holding `spawn`.
    fn braked_and_free(spawn: &LooseSpawn) -> ((PhysicsWorld, LooseId), (PhysicsWorld, LooseId)) {
        let mut braked = world_with_floor(&floor_mesh(0.0));
        let mut free = world_with_floor(&floor_mesh(0.0));
        free.set_rest_slope(0.0);
        let braked_id = braked.spawn_loose(spawn);
        let free_id = free.spawn_loose(spawn);
        ((braked, braked_id), (free, free_id))
    }

    fn assert_bit_equal_every_tick(
        braked: &mut (PhysicsWorld, LooseId),
        free: &mut (PhysicsWorld, LooseId),
        ticks: u32,
    ) {
        for tick in 1..=ticks {
            braked.0.step(DT);
            free.0.step(DT);
            let (b, f) = (read(&braked.0, braked.1), read(&free.0, free.1));
            assert_eq!(bits(&b), bits(&f), "tick {tick}: state bits differ");
            assert_eq!(b.asleep, f.asleep, "tick {tick}: asleep differs");
        }
    }

    #[test]
    fn a_box_is_not_braked() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, FLAT_BOX.y, 0.0),
            linear_velocity: Vec3::new(2.0, 0.0, 1.0),
            angular_velocity: Vec3::new(0.0, 3.0, 0.0),
            ..LooseSpawn::new(FLAT_BOX_SHAPE, 0.2)
        };
        let (mut braked, mut free) = braked_and_free(&spawn);

        assert_bit_equal_every_tick(&mut braked, &mut free, 120);

        let slid = read(&braked.0, braked.1).position.x;
        assert!(slid > 0.1, "the box slid, x={slid}");
    }

    #[test]
    fn a_capsule_on_a_loose_box_is_braked() {
        let platform = LooseSpawn {
            position: Vec3::new(0.0, 0.1, 0.0),
            ..LooseSpawn::new(cuboid(Vec3::new(1.5, 0.1, 1.5)), 200.0)
        };
        let capsule = LooseSpawn {
            position: Vec3::new(0.0, 0.2 + CAPSULE_RADIUS, -0.8),
            ..rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0)
        };
        let ground_speed = |world: &PhysicsWorld, id: LooseId| {
            let velocity = read(world, id).linear_velocity;
            Vec3::new(velocity.x, 0.0, velocity.z).length()
        };
        let setup = |rest_slope: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            world.set_rest_slope(rest_slope);
            world.spawn_loose(&platform);
            let id = world.spawn_loose(&capsule);
            (world, id)
        };
        let (mut braked, braked_id) = setup(degrees(5.0));
        let (mut free, free_id) = setup(0.0);

        for tick in 1..=30 {
            braked.step(DT);
            free.step(DT);
            let (b, f) = (
                ground_speed(&braked, braked_id),
                ground_speed(&free, free_id),
            );
            assert!(
                b <= f + 1e-3,
                "tick {tick}: braked speed {b} above free speed {f}"
            );
        }

        let (b, f) = (
            ground_speed(&braked, braked_id),
            ground_speed(&free, free_id),
        );
        assert!(
            b < f - 0.3,
            "after 30 ticks on the box: braked {b}, free {f}"
        );
    }

    /// Spawns a rolling `shape` 3 cm into the top of a settled loose box, runs 300 ticks and
    /// checks the two end within 5 mm of touching, both asleep.
    fn assert_pressed_shape_is_pushed_out(name: &str, shape: LooseShape, radius: f32) {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let platform = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.1, 0.0),
            ..LooseSpawn::new(cuboid(Vec3::new(1.0, 0.1, 1.0)), 20.0)
        });
        run(&mut world, 120);
        assert!(
            read(&world, platform).asleep,
            "{name}: the box settles first"
        );
        let rider = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.2 + radius - 0.03, -0.8),
            ..rolling_spawn(shape, radius, 2.0)
        });

        run(&mut world, 300);

        let (box_state, rider_state) = (read(&world, platform), read(&world, rider));
        assert!(
            box_state.asleep && rider_state.asleep,
            "{name}: both sleep, {box_state:?} {rider_state:?}"
        );
        let overlap = deepest_pair_overlap(&world);
        assert!(overlap <= 0.005, "{name}: ends {overlap} m into the box");
    }

    #[test]
    fn a_capsule_pressed_into_a_box_top_is_pushed_out() {
        assert_pressed_shape_is_pushed_out("capsule", CAPSULE, CAPSULE_RADIUS);
    }

    #[test]
    fn a_cylinder_pressed_into_a_box_top_is_pushed_out() {
        assert_pressed_shape_is_pushed_out("cylinder", CYLINDER, CYLINDER_RADIUS);
    }

    #[test]
    fn the_brake_cuts_spin_and_travel_at_matching_rates() {
        for (name, shape, radius) in [
            ("capsule", CAPSULE, CAPSULE_RADIUS),
            ("cylinder", CYLINDER, CYLINDER_RADIUS),
        ] {
            let spawn = rolling_spawn(shape, radius, 2.0);
            let (mut braked, mut free) = braked_and_free(&spawn);
            // The worlds are identical until the brake first acts, which may take a tick or
            // two to find ground contact.
            let (mut b, mut f) = (read(&braked.0, braked.1), read(&free.0, free.1));
            for _ in 0..5 {
                braked.0.step(DT);
                free.0.step(DT);
                (b, f) = (read(&braked.0, braked.1), read(&free.0, free.1));
                if bits(&b) != bits(&f) {
                    break;
                }
            }
            assert_ne!(bits(&b), bits(&f), "{name}: the brake never acted");

            let axis = b.rotation * Vec3::Y;
            let along = axis.cross(Vec3::Y).normalize();
            let decel = braked.0.gravity() * braked.0.rest_slope().tan();
            let spin_cut = f.angular_velocity.dot(axis).abs() - b.angular_velocity.dot(axis).abs();
            let travel_cut =
                f.linear_velocity.dot(along).abs() - b.linear_velocity.dot(along).abs();

            let (want_spin, want_travel) = (decel * DT / radius, decel * DT);
            assert!(
                (spin_cut - want_spin).abs() <= 0.02 * want_spin,
                "{name}: spin cut {spin_cut}, expected {want_spin}"
            );
            assert!(
                (travel_cut - want_travel).abs() <= 0.02 * want_travel,
                "{name}: travel cut {travel_cut}, expected {want_travel}"
            );
        }
    }

    #[test]
    fn a_braked_body_is_braked_whichever_collider_is_first() {
        let ground_speed = |world: &PhysicsWorld, id: LooseId| {
            let velocity = read(world, id).linear_velocity;
            Vec3::new(velocity.x, 0.0, velocity.z).length()
        };
        let after_ten_ticks = |rest_slope: f32| {
            let mut world = PhysicsWorld::new();
            world.set_rest_slope(rest_slope);
            let id = world.spawn_loose(&rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0));
            world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
            world.step(DT);
            let own = world.rigid_body_set[world.loose[&id].body].colliders()[0];
            let first = world
                .narrow_phase
                .contact_pairs_with(own)
                .all(|pair| pair.collider1 == own);
            assert!(
                first,
                "the capsule's collider is first in its contact pairs"
            );
            run(&mut world, 9);
            ground_speed(&world, id)
        };

        let braked = after_ten_ticks(degrees(5.0));
        let free = after_ten_ticks(0.0);

        assert!(
            braked < free - 0.1,
            "braked {braked} against free {free} after 10 ticks"
        );
    }

    #[test]
    fn zero_gravity_means_no_brake() {
        let spawn = rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0);
        let (mut braked, mut free) = braked_and_free(&spawn);
        braked.0.set_gravity(0.0);
        free.0.set_gravity(0.0);

        assert_bit_equal_every_tick(&mut braked, &mut free, 60);

        let state = read(&braked.0, braked.1);
        assert!(
            state.linear_velocity.is_finite() && state.angular_velocity.is_finite(),
            "{state:?}"
        );
        assert!(
            state.linear_velocity.z > 1.9,
            "the capsule keeps rolling, {state:?}"
        );
    }

    #[test]
    fn a_sleeping_capsule_stays_asleep() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_spawn(CAPSULE, CAPSULE_RADIUS, 2.0));
        run(&mut world, 300);
        let mut previous = read(&world, id);
        assert!(previous.asleep, "the capsule sleeps first, {previous:?}");

        for tick in 1..=120 {
            world.step(DT);
            let state = read(&world, id);
            assert!(state.asleep, "tick {tick}: woke, {state:?}");
            assert_eq!(bits(&state), bits(&previous), "tick {tick}: state moved");
            previous = state;
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let (mut first, first_ids) = pile_world(7);
        let (mut second, second_ids) = pile_world(7);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (a, b) in first_ids.iter().zip(&second_ids) {
                let (a, b) = (read(&first, *a), read(&second, *b));
                assert_eq!(bits(&a), bits(&b), "tick {tick}: state bits differ");
                assert_eq!(a.asleep, b.asleep, "tick {tick}: asleep differs");
            }
        }
    }

    #[test]
    fn spawn_rejects_a_bad_capsule_or_cylinder() {
        let mut rows = Vec::new();
        for (name, value) in BAD_POSITIVE {
            let shapes = [
                (
                    format!("{name} capsule half height"),
                    LooseShape::Capsule {
                        half_height: value,
                        radius: 0.04,
                    },
                ),
                (
                    format!("{name} capsule radius"),
                    LooseShape::Capsule {
                        half_height: 0.1,
                        radius: value,
                    },
                ),
                (
                    format!("{name} cylinder half height"),
                    LooseShape::Cylinder {
                        half_height: value,
                        radius: 0.05,
                    },
                ),
                (
                    format!("{name} cylinder radius"),
                    LooseShape::Cylinder {
                        half_height: 0.08,
                        radius: value,
                    },
                ),
            ];
            for (row, shape) in shapes {
                rows.push((row, LooseSpawn::new(shape, 1.0)));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
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
