use super::PhysicsWorld;
use glam::{Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, NarrowPhase, Pose, RigidBodyBuilder, RigidBodyHandle,
};

/// Steepest ground, radians from level, that a new world brakes loose objects on: 5°.
pub(super) const DEFAULT_REST_SLOPE: f32 = 5.0_f32.to_radians();

/// Contact depth, metres, past which an overlap is left to the solver to push apart rather
/// than braked: braking would zero the velocity that separates the bodies and freeze the
/// overlap.
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

/// What the world keeps per loose object besides its body.
pub(super) struct Loose {
    body: RigidBodyHandle,
    /// Radius, metres, of the round cross-section that rolls about the body's local Y; `None`
    /// for a shape that does not roll.
    roll_radius: Option<f32>,
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
    /// Radius of the round cross-section that rolls about local Y; a box does not roll.
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
    /// Boxes, capsules and cylinders are supported. Resting on ground within the world's
    /// [rest slope](PhysicsWorld::set_rest_slope) of level, the object is braked until it stops.
    ///
    /// Characters do not ignore loose objects: a loose object blocks a character or is stepped
    /// onto like geometry, and any overlap shoves the object because the solver treats the
    /// character as infinitely heavy. Rays skip loose objects because their colliders carry no
    /// tag.
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

    /// Steepest ground, radians from level against gravity, on which loose objects are
    /// braked to rest. 0 turns the brake off. Default 5°.
    ///
    /// After each [`PhysicsWorld::step`], an awake loose object in contact with ground (a world
    /// mesh or another loose object) within this slope of level loses `g·tan(slope)` of
    /// speed per second, and its spin the matching amount for its rolling radius, until it
    /// stops. `g·tan(slope)` is the pull that holds a sliding object on that slope, so under
    /// the slope everything comes to rest and over it rolling shapes roll down. The speed
    /// along gravity is untouched, and so is an object being pushed out of a deep overlap.
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

    pub(super) fn brake_rolling(&mut self, dt: f32) {
        let cut = self.gravity.length() * self.rest_slope.tan() * dt;
        if cut == 0.0 {
            return;
        }
        let up = -self.gravity.normalize_or_zero();
        let cos_slope = self.rest_slope.cos();

        for loose in self.loose.values() {
            let Some(radius) = loose.roll_radius else {
                continue;
            };
            let body = &mut self.rigid_body_set[loose.body];
            if body.is_sleeping()
                || !rests_on_flat_ground(&self.narrow_phase, body.colliders()[0], up, cos_slope)
            {
                continue;
            }
            // Only the rolling motion is braked: spin about the shape's axis and travel across
            // it. Tipping happens in the plane of the axis and up, so it is left alone and a
            // shape tipping over an edge cannot be frozen part-way.
            let axis = *body.rotation() * Vec3::Y;
            let spin = body.angvel();
            let rolling_spin = axis * spin.dot(axis);
            body.set_angvel(
                spin - rolling_spin + shrink(rolling_spin, cut / radius),
                false,
            );
            let across_axis = axis.cross(up).normalize_or_zero();
            let velocity = body.linvel();
            let rolling_velocity = across_axis * velocity.dot(across_axis);
            body.set_linvel(
                velocity - rolling_velocity + shrink(rolling_velocity, cut),
                false,
            );
        }
    }
}

/// Whether `collider` touches a surface within the rest slope of `up` and no contact of it is
/// deeper than [`DEEP_CONTACT`]. A predicted contact, not yet touching, is not ground.
fn rests_on_flat_ground(
    narrow_phase: &NarrowPhase,
    collider: ColliderHandle,
    up: Vec3,
    cos_slope: f32,
) -> bool {
    let mut on_flat = false;
    for pair in narrow_phase.contact_pairs_with(collider) {
        if !pair.has_any_active_contact() {
            continue;
        }
        for manifold in pair.manifolds().iter().filter(|m| !m.points.is_empty()) {
            if manifold.points.iter().any(|p| p.dist < -DEEP_CONTACT) {
                return false;
            }
            let into_body = if pair.collider2 == collider {
                manifold.data.normal
            } else {
                -manifold.data.normal
            };
            on_flat |=
                into_body.dot(up) >= cos_slope && manifold.points.iter().any(|p| p.dist <= 0.0);
        }
    }
    on_flat
}

/// `v` shortened by `cut`, or zero if it is no longer than `cut`.
fn shrink(v: Vec3, cut: f32) -> Vec3 {
    v.clamp_length_max((v.length() - cut).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::test_support::{floor_mesh, slab_mesh, slope_mesh, TEST_TAG};
    use crate::world::RayQuery;
    use moho_render_api::{WorldMesh, WorldMeshId};
    use proptest::prelude::*;
    use rapier3d::prelude::{Collider, RigidBody};

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

    const CAPSULE: LooseShape = LooseShape::Capsule {
        half_height: 0.1,
        radius: 0.04,
    };

    const CYLINDER: LooseShape = LooseShape::Cylinder {
        half_height: 0.08,
        radius: 0.05,
    };

    fn every_shape() -> [(&'static str, LooseShape); 3] {
        [
            ("box", cuboid(FLAT_BOX)),
            ("capsule", CAPSULE),
            ("cylinder", CYLINDER),
        ]
    }

    /// The radius of the round cross-section that rolls on the ground when the shape lies on its side.
    fn cross_section(shape: LooseShape) -> f32 {
        match shape {
            LooseShape::Capsule { radius, .. } | LooseShape::Cylinder { radius, .. } => radius,
            LooseShape::Box { half_extents } => half_extents.min_element(),
        }
    }

    fn body_of(world: &PhysicsWorld, id: LooseId) -> &RigidBody {
        &world.rigid_body_set[world.loose[&id].body]
    }

    fn collider_of(world: &PhysicsWorld, id: LooseId) -> &Collider {
        &world.collider_set[body_of(world, id).colliders()[0]]
    }

    /// Height of the lowest point of the object's bounding box.
    fn bottom_of(world: &PhysicsWorld, id: LooseId) -> f32 {
        collider_of(world, id).compute_aabb().mins.y
    }

    /// Whether `point`, relative to the object's centre at the identity rotation, is inside it.
    fn probe(world: &PhysicsWorld, id: LooseId, point: Vec3) -> bool {
        let collider = collider_of(world, id);
        collider.shape().contains_point(collider.position(), point)
    }

    /// How deep two objects overlap, 0 if they do not touch.
    fn overlap(world: &PhysicsWorld, a: LooseId, b: LooseId) -> f32 {
        let (a, b) = (collider_of(world, a), collider_of(world, b));
        rapier3d::parry::query::contact(a.position(), a.shape(), b.position(), b.shape(), 0.0)
            .expect("both shapes are supported by contact queries")
            .map_or(0.0, |contact| (-contact.dist).max(0.0))
    }

    /// Xorshift32, so the piles are the same on every machine.
    struct Dice(u32);

    impl Dice {
        fn seeded(seed: u32) -> Self {
            Self(seed.wrapping_mul(2_654_435_761).wrapping_add(1))
        }

        /// In `[0, 1]`.
        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32
        }

        /// In `[-limit, limit]`.
        fn signed(&mut self, limit: f32) -> f32 {
            (self.unit() * 2.0 - 1.0) * limit
        }

        fn rotation(&mut self) -> Quat {
            Quat::from_euler(
                glam::EulerRot::XYZ,
                self.unit() * 6.0,
                self.unit() * 6.0,
                self.unit() * 6.0,
            )
        }
    }

    /// Twenty undamped objects of every shape in turn, dropped 0.3 m apart above a 1 m square.
    fn pile_spawns(seed: u32) -> Vec<LooseSpawn> {
        let shapes = every_shape();
        let mut dice = Dice::seeded(seed);
        (0..20)
            .map(|i| {
                let position =
                    Vec3::new(dice.unit() - 0.5, 0.2 + i as f32 * 0.3, dice.unit() - 0.5);
                LooseSpawn {
                    position,
                    rotation: dice.rotation(),
                    ..LooseSpawn::new(shapes[i % shapes.len()].1, 0.2)
                }
            })
            .collect()
    }

    fn pile_world(seed: u32) -> (PhysicsWorld, Vec<LooseId>) {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids = pile_spawns(seed)
            .iter()
            .map(|spawn| world.spawn_loose(spawn))
            .collect();
        (world, ids)
    }

    fn all_asleep(world: &PhysicsWorld, ids: &[LooseId]) -> bool {
        ids.iter().all(|&id| read(world, id).asleep)
    }

    /// Level ground when `degrees` is 0, otherwise ground through the origin falling toward +Z.
    fn tilted_world(degrees: f32) -> PhysicsWorld {
        let rise = 10.0 * degrees.to_radians().tan();
        world_with_floor(&slope_mesh(-10.0, rise, 10.0, -rise))
    }

    /// `shape` lying on its side, axis across the slope of `tilted_world(degrees)`, 2 mm above the
    /// ground at the origin.
    fn lying_on_slope(shape: LooseShape, degrees: f32) -> LooseSpawn {
        let tilt = degrees.to_radians();
        let normal = Vec3::new(0.0, tilt.cos(), tilt.sin());
        LooseSpawn {
            position: normal * (cross_section(shape) + 0.002),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(shape, 0.2)
        }
    }

    /// A capsule lying on its side on the floor, rolling along +Z without slipping.
    fn rolling_capsule(speed: f32) -> LooseSpawn {
        let radius = cross_section(CAPSULE);
        LooseSpawn {
            position: Vec3::new(0.0, radius, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / radius, 0.0, 0.0),
            ..LooseSpawn::new(CAPSULE, 0.2)
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

    /// Half the bounding box of a shape at the identity rotation.
    fn expected_half_box(shape: LooseShape) -> Vec3 {
        match shape {
            LooseShape::Box { half_extents } => half_extents,
            LooseShape::Capsule {
                half_height,
                radius,
            } => Vec3::new(radius, half_height + radius, radius),
            LooseShape::Cylinder {
                half_height,
                radius,
            } => Vec3::new(radius, half_height, radius),
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
                half_height: 0.3,
                radius: 0.15,
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
            let body = body_of(&world, id);
            assert!(
                (body.mass() - 3.5).abs() <= 1e-4,
                "{shape:?}: the body weighs the stated mass, mass={}",
                body.mass()
            );
            let collider = collider_of(&world, id);
            assert!(
                (collider.friction() - 0.3).abs() <= 1e-6
                    && (collider.restitution() - 0.2).abs() <= 1e-6,
                "{shape:?}: the collider carries the stated material"
            );
        }
    }

    #[test]
    fn each_shape_spawns_with_its_stated_size() {
        for (name, shape) in every_shape() {
            let mut world = PhysicsWorld::new();

            let id = world.spawn_loose(&LooseSpawn::new(shape, 1.0));

            let aabb = collider_of(&world, id).compute_aabb();
            let half = (aabb.maxs - aabb.mins) * 0.5;
            assert!(
                half.abs_diff_eq(expected_half_box(shape), 1e-5),
                "{name}: bounding box half {half}, expected {}",
                expected_half_box(shape)
            );
        }
    }

    #[test]
    fn a_capsule_has_round_caps_and_a_cylinder_has_flat_ends() {
        let corner = Vec3::new(0.035, 0.135, 0.0);
        let mut world = PhysicsWorld::new();
        let capsule = world.spawn_loose(&LooseSpawn::new(CAPSULE, 1.0));
        let flat = world.spawn_loose(&LooseSpawn::new(
            LooseShape::Cylinder {
                half_height: 0.14,
                radius: 0.04,
            },
            1.0,
        ));

        let in_capsule = probe(&world, capsule, corner);
        let in_cylinder = probe(&world, flat, corner);

        assert!(
            !in_capsule,
            "the capsule's cap is rounded off at the corner"
        );
        assert!(in_cylinder, "the cylinder reaches the corner");
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
                    format!("{name} half height of a {shape}"),
                    LooseSpawn::new(make(value, 0.05), 1.0),
                ));
                rows.push((
                    format!("{name} radius of a {shape}"),
                    LooseSpawn::new(make(0.1, value), 1.0),
                ));
            }
        }

        assert_rows_rejected(&rows, "half height and radius must be positive and finite");
    }

    #[test]
    fn each_shape_comes_to_rest_asleep() {
        for (name, shape) in every_shape() {
            for seed in 0..8 {
                let mut dice = Dice::seeded(seed);
                let mut world = world_with_floor(&floor_mesh(0.0));
                let id = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, 1.0, 0.0),
                    rotation: dice.rotation(),
                    angular_velocity: Vec3::new(
                        dice.signed(10.0),
                        dice.signed(10.0),
                        dice.signed(10.0),
                    ),
                    linear_velocity: Vec3::new(dice.signed(3.0), 0.0, dice.signed(3.0)),
                    ..LooseSpawn::new(shape, 0.2)
                });

                run(&mut world, 300);

                let state = read(&world, id);
                let bottom = bottom_of(&world, id);
                assert!(
                    bottom.abs() <= 0.001,
                    "{name} seed {seed}: bottom is {bottom} from the floor, state {state:?}"
                );
                assert!(state.asleep, "{name} seed {seed}: awake, state {state:?}");
            }
        }
    }

    #[test]
    fn a_rolling_capsule_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&rolling_capsule(2.0));
        let mut previous = read(&world, id).linear_velocity.length();

        for tick in 1..=300 {
            world.step(DT);
            let speed = read(&world, id).linear_velocity.length();
            assert!(
                speed <= previous + 1e-3,
                "tick {tick}: speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        let state = read(&world, id);
        assert!(
            state.position.z >= 1.0,
            "the capsule rolls about v^2/2a = 1.27 m before it stops, z={}",
            state.position.z
        );
        assert!(
            state.position.z <= 1.5,
            "the capsule stops within 1.5 m, z={}",
            state.position.z
        );
        assert!(state.asleep, "the capsule is at rest, state {state:?}");
    }

    #[test]
    fn a_capsule_spawned_before_its_floor_still_stops() {
        // The floor's collider is created second, so the contact pair lists the capsule first
        // and its normal points out of the capsule.
        let mut world = PhysicsWorld::new();
        let id = world.spawn_loose(&rolling_capsule(2.0));
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        run(&mut world, 300);
        let state = read(&world, id);
        assert!(
            state.asleep && state.position.z <= 1.5,
            "the capsule stops within 1.5 m, state {state:?}"
        );
    }

    #[test]
    fn the_default_rest_slope_is_five_degrees() {
        let world = PhysicsWorld::new();

        assert!(
            (world.rest_slope() - 5f32.to_radians()).abs() <= 1e-6,
            "rest slope {}",
            world.rest_slope()
        );
    }

    #[test]
    fn the_rest_slope_holds_a_shape_on_gentle_ground() {
        let rows = [
            ("capsule", CAPSULE, 2.0),
            ("capsule", CAPSULE, 4.9),
            ("cylinder", CYLINDER, 4.9),
        ];

        for (name, shape, degrees) in rows {
            let mut world = tilted_world(degrees);
            let spawn = lying_on_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let state = read(&world, id);
            let travelled = (state.position - spawn.position).length();
            assert!(
                travelled < 0.02,
                "{name} on {degrees} degrees should rest, travelled {travelled}"
            );
            assert!(state.asleep, "{name} on {degrees} degrees should sleep");
        }
    }

    #[test]
    fn the_rest_slope_lets_a_shape_roll_down_steep_ground() {
        let rows = [("capsule", CAPSULE, 5.5), ("cylinder", CYLINDER, 10.0)];

        for (name, shape, degrees) in rows {
            let mut world = tilted_world(degrees);
            let spawn = lying_on_slope(shape, degrees);
            let id = world.spawn_loose(&spawn);

            run(&mut world, 300);

            let down = read(&world, id).position.z - spawn.position.z;
            assert!(
                down > 1.0,
                "{name} on {degrees} degrees should roll down, rolled {down}"
            );
        }
    }

    #[test]
    fn the_rest_slope_is_game_data() {
        let mut steeper = tilted_world(8.0);
        steeper.set_rest_slope(10f32.to_radians());
        let lying = steeper.spawn_loose(&lying_on_slope(CAPSULE, 8.0));
        let mut off = world_with_floor(&floor_mesh(0.0));
        off.set_rest_slope(0.0);
        let rolling = off.spawn_loose(&rolling_capsule(2.0));

        run(&mut steeper, 300);
        run(&mut off, 120);

        assert!(
            (steeper.rest_slope() - 10f32.to_radians()).abs() <= 1e-6,
            "rest slope reads back, got {}",
            steeper.rest_slope()
        );
        assert_eq!(off.rest_slope(), 0.0);
        let held = read(&steeper, lying);
        assert!(held.asleep, "the capsule sleeps on 8 degrees, {held:?}");
        let speed = read(&off, rolling).linear_velocity.length();
        assert!(
            speed > 1.5,
            "without the brake it keeps rolling, speed={speed}"
        );
    }

    #[test]
    fn set_rest_slope_rejects_out_of_range() {
        let rows = [
            ("NaN", f32::NAN),
            ("+inf", f32::INFINITY),
            ("-inf", f32::NEG_INFINITY),
            ("negative", -0.01),
            ("a right angle", std::f32::consts::FRAC_PI_2),
            ("past a right angle", 2.0),
        ];

        for (name, slope) in rows {
            let mut world = PhysicsWorld::new();
            world.set_rest_slope(0.1);

            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                world.set_rest_slope(slope);
            }));

            assert!(outcome.is_err(), "{name} was accepted");
            assert_eq!(world.rest_slope(), 0.1, "{name} changed the rest slope");
        }
    }

    #[test]
    fn set_rest_slope_accepts_zero_and_just_under_a_right_angle() {
        let mut world = PhysicsWorld::new();

        world.set_rest_slope(0.0);
        let off = world.rest_slope();
        world.set_rest_slope(1.55);

        assert_eq!(off, 0.0);
        assert_eq!(world.rest_slope(), 1.55);
    }

    #[test]
    fn the_brake_scales_with_gravity() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_gravity(9.0);
        let id = world.spawn_loose(&rolling_capsule(2.0));

        run(&mut world, 600);

        let state = read(&world, id);
        assert!(state.asleep, "the capsule stops, state {state:?}");
        assert!(
            state.position.z > 2.0 && state.position.z < 3.0,
            "half the gravity doubles the roll to about 2.54 m, z={}",
            state.position.z
        );
    }

    #[test]
    fn a_shape_just_above_the_floor_is_not_braked() {
        let spawn = LooseSpawn {
            position: Vec3::new(0.0, cross_section(CAPSULE) + 0.019, 0.0),
            ..rolling_capsule(2.0)
        };
        let after_two_ticks = |rest_slope: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            world.set_rest_slope(rest_slope);
            let id = world.spawn_loose(&spawn);
            for _ in 0..2 {
                world.step(DT);
            }
            world.loose(id).expect("capsule is alive")
        };
        let braked = after_two_ticks(DEFAULT_REST_SLOPE);
        assert!(
            braked.position.y > cross_section(CAPSULE),
            "capsule reached the floor, so the test no longer probes the gap: {braked:?}"
        );
        assert_eq!(braked, after_two_ticks(0.0));
    }

    #[test]
    fn a_tilted_cylinder_tips_over_instead_of_freezing_on_its_rim() {
        for tilt_degrees in [10.0_f32, 20.0, 30.0, 45.0] {
            let tilt = tilt_degrees.to_radians();
            let LooseShape::Cylinder {
                half_height,
                radius,
            } = CYLINDER
            else {
                unreachable!("CYLINDER is a cylinder");
            };
            // Rim on the floor: the lowest point of a cylinder tilted about X sits at
            // half_height·cos + radius·sin below its centre.
            let height = half_height * tilt.cos() + radius * tilt.sin() + 0.001;
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, height, 0.0),
                rotation: Quat::from_rotation_x(tilt),
                ..LooseSpawn::new(CYLINDER, 0.2)
            });
            for _ in 0..300 {
                world.step(DT);
            }
            let state = world.loose(id).expect("cylinder is alive");
            let axis_from_vertical = angle_from_vertical_degrees(state.rotation * Vec3::Y);
            let lying = (axis_from_vertical - 90.0).abs() < 1.0;
            let standing = axis_from_vertical < 1.0 || axis_from_vertical > 179.0;
            assert!(
                state.asleep && (lying || standing),
                "tilt {tilt_degrees}: cylinder froze at {axis_from_vertical} degrees, {state:?}"
            );
        }
    }

    #[test]
    fn zero_gravity_means_no_brake() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_gravity(0.0);
        let id = world.spawn_loose(&rolling_capsule(2.0));

        run(&mut world, 60);

        let state = read(&world, id);
        assert!(
            state.linear_velocity.is_finite() && state.angular_velocity.is_finite(),
            "the velocities stay finite, state {state:?}"
        );
        assert!(
            state.linear_velocity.length() > 1.9,
            "nothing slows the capsule, state {state:?}"
        );
    }

    #[test]
    fn another_loose_object_is_ground() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let slab = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.05, 0.0),
            ..LooseSpawn::new(cuboid(Vec3::new(1.0, 0.05, 1.0)), 5.0)
        });
        run(&mut world, 60);
        let radius = cross_section(CAPSULE);
        let rolling = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.1 + radius, 0.0),
            ..rolling_capsule(1.0)
        });

        run(&mut world, 300);

        let state = read(&world, rolling);
        assert!(
            state.position.z < 1.0,
            "the capsule stops on the slab instead of rolling off, z={}",
            state.position.z
        );
        assert!(
            (bottom_of(&world, rolling) - 0.1).abs() <= 0.005,
            "the capsule still rests on the slab, bottom={}",
            bottom_of(&world, rolling)
        );
        assert!(all_asleep(&world, &[slab, rolling]), "both sleep");
    }

    #[test]
    fn non_loose_bodies_are_not_braked() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let handle = world.add_dynamic_sphere(Vec3::new(0.0, 0.05, 0.0), 0.05);
        let body = &mut world.rigid_body_set[handle];
        body.set_linvel(Vec3::new(0.0, 0.0, 2.0), true);
        body.set_angvel(Vec3::new(40.0, 0.0, 0.0), true);

        run(&mut world, 120);

        let speed = world.rigid_body_set[handle].linvel().length();
        assert!(speed > 1.5, "the sphere keeps rolling, speed={speed}");
    }

    #[test]
    fn up_follows_gravity() {
        let mut world = world_with_floor(&floor_mesh(2.0));
        world.set_gravity(-18.0);
        let radius = cross_section(CAPSULE);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 2.0 - radius, 0.0),
            angular_velocity: Vec3::new(-2.0 / radius, 0.0, 0.0),
            ..rolling_capsule(2.0)
        });

        run(&mut world, 300);

        let state = read(&world, id);
        assert!(
            state.position.z.abs() <= 1.5,
            "the capsule stops within 1.5 m on the ceiling, state {state:?}"
        );
        assert!(state.asleep, "the capsule is at rest, state {state:?}");
    }

    #[test]
    fn the_brake_never_touches_vertical_velocity() {
        let fall = |rest_slope: f32| {
            let mut world = world_with_floor(&floor_mesh(0.0));
            world.set_rest_slope(rest_slope);
            let id = world.spawn_loose(&LooseSpawn {
                restitution: 1.0,
                position: Vec3::new(0.0, 1.0, 0.0),
                ..LooseSpawn::new(cube(0.05), 0.2)
            });
            (0..30)
                .map(|_| {
                    world.step(DT);
                    read(&world, id).linear_velocity.y.to_bits()
                })
                .collect::<Vec<_>>()
        };

        let braked = fall(5f32.to_radians());
        let free = fall(0.0);

        assert_eq!(braked, free, "vertical velocity differs tick by tick");
        let bounced = braked.iter().any(|&bits| f32::from_bits(bits) > 1.0);
        assert!(bounced, "the box bounced back up");
    }

    #[test]
    fn the_brake_spares_overlapping_bodies() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let radius = cross_section(CAPSULE);
        let side_by_side = |z: f32| LooseSpawn {
            position: Vec3::new(0.0, radius, z),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..LooseSpawn::new(CAPSULE, 0.2)
        };
        let first = world.spawn_loose(&side_by_side(0.0));
        let second = world.spawn_loose(&side_by_side(2.0 * radius - 0.01));
        assert!(
            overlap(&world, first, second) > 0.009,
            "the capsules start 1 cm into each other"
        );

        run(&mut world, 300);

        let depth = overlap(&world, first, second);
        assert!(depth <= 0.005, "the capsules end {depth} m into each other");
        assert!(all_asleep(&world, &[first, second]), "both capsules sleep");
    }

    #[test]
    fn a_pile_of_every_shape_settles() {
        for seed in [0, 1, 2, 10] {
            let (mut world, ids) = pile_world(seed);

            run(&mut world, 600);

            for (i, &id) in ids.iter().enumerate() {
                let state = read(&world, id);
                assert!(state.asleep, "seed {seed} object {i} is awake, {state:?}");
                let depth = -bottom_of(&world, id);
                assert!(
                    depth <= 0.001,
                    "seed {seed} object {i} is {depth} m into the floor"
                );
                for (j, &other) in ids.iter().enumerate().skip(i + 1) {
                    let depth = overlap(&world, id, other);
                    assert!(
                        depth <= 0.005,
                        "seed {seed} objects {i} and {j} overlap by {depth} m"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_pile_twice_ends_bit_equal() {
        let (mut first, first_ids) = pile_world(3);
        let (mut second, second_ids) = pile_world(3);

        for tick in 1..=600 {
            first.step(DT);
            second.step(DT);
            for (i, (&a, &b)) in first_ids.iter().zip(&second_ids).enumerate() {
                let (a, b) = (read(&first, a), read(&second, b));
                assert_eq!(
                    bits(&a),
                    bits(&b),
                    "tick {tick} object {i}: state bits differ"
                );
                assert_eq!(a.asleep, b.asleep, "tick {tick} object {i}: asleep differs");
            }
        }
        assert!(
            all_asleep(&first, &first_ids),
            "the pile came to rest, so the comparison covered the whole settle"
        );
    }
}
