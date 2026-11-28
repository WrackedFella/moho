use super::query::encode_loose;
use super::PhysicsWorld;
use glam::{Quat, Vec3};
use moho_core::sim_math;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, Group, InteractionGroups, InteractionTestMode, NarrowPhase,
    Pose, RigidBody, RigidBodyBuilder, RigidBodyHandle,
};
use serde::{Deserialize, Serialize};

/// The pose a body holds right now.
fn body_pose(body: &RigidBody) -> LoosePose {
    LoosePose {
        position: body.translation(),
        rotation: *body.rotation(),
    }
}

/// Membership bit that marks a loose collider. A collider whose only membership is this bit is
/// ignored by characters: their filter tests with `And`, so it hides only colliders whose every
/// membership bit is `LOOSE`. The character capsule, world meshes and props carry `Group::ALL`,
/// which includes this bit but also others, so they stay visible. Give another collider this
/// bit alone and characters stop seeing it.
pub(super) const LOOSE: Group = Group::GROUP_32;

/// Collision filter of a loose collider: it carries [`LOOSE`] and meets every collider.
const LOOSE_GROUPS: InteractionGroups =
    InteractionGroups::new(LOOSE, Group::ALL, InteractionTestMode::And);

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LooseId(u64);

impl LooseId {
    /// The issue number: how many ids the world had issued before this one.
    pub fn get(self) -> u64 {
        self.0
    }

    pub(super) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// What the world keeps per loose object besides its body.
pub(super) struct Loose {
    body: RigidBodyHandle,
    /// The spawn as given; its pose and velocity fields are those of the spawn, not the body's
    /// current ones.
    spawn: LooseSpawn,
    /// The body's pose before the last step, for drawing between ticks.
    previous: LoosePose,
}

/// How a shape rolls, which decides what the rest brake cuts.
#[derive(Clone, Copy)]
enum Roll {
    /// A shape that does not roll.
    None,
    /// A round cross-section of `radius` metres that rolls about the body's local Y.
    About { radius: f32 },
    /// A ball of `radius` metres that rolls about any axis.
    Ball { radius: f32 },
}

/// The collision shape of a loose object.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LooseShape {
    /// A box centred on the body, `half_extents` m from the centre to each face.
    Box { half_extents: Vec3 },
    /// A capsule along the body's local Y: a cylinder of `half_height` capped by hemispheres.
    Capsule { half_height: f32, radius: f32 },
    /// A cylinder along the body's local Y.
    Cylinder { half_height: f32, radius: f32 },
    /// A sphere centred on the body.
    Sphere { radius: f32 },
}

impl LooseShape {
    fn roll(self) -> Roll {
        match self {
            Self::Box { .. } => Roll::None,
            Self::Capsule { radius, .. } | Self::Cylinder { radius, .. } => Roll::About { radius },
            Self::Sphere { radius } => Roll::Ball { radius },
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
            Self::Sphere { radius } => ColliderBuilder::ball(radius),
        }
    }
}

/// Everything a game states when it drops a loose object.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

    /// Whether every field is fit for the simulation; the error names the first that is not.
    fn check(&self) -> Result<(), String> {
        if !(self.mass.is_finite() && self.mass > 0.0) {
            return Err(format!(
                "mass must be positive and finite, got {}",
                self.mass
            ));
        }
        match self.shape {
            LooseShape::Box { half_extents } => {
                if !(half_extents.is_finite() && half_extents.min_element() > 0.0) {
                    return Err(format!(
                        "half extents must be positive and finite, got {half_extents}"
                    ));
                }
            }
            LooseShape::Capsule {
                half_height,
                radius,
            }
            | LooseShape::Cylinder {
                half_height,
                radius,
            } => {
                if !(half_height.is_finite()
                    && half_height > 0.0
                    && radius.is_finite()
                    && radius > 0.0)
                {
                    return Err(format!(
                        "half height and radius must be positive and finite, got {half_height} and {radius}"
                    ));
                }
            }
            LooseShape::Sphere { radius } => {
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(format!("radius must be positive and finite, got {radius}"));
                }
            }
        }
        if !(self.linear_velocity.is_finite() && self.angular_velocity.is_finite()) {
            return Err(format!(
                "velocity must be finite, got linear {} and angular {}",
                self.linear_velocity, self.angular_velocity
            ));
        }
        for (name, value) in [
            ("friction", self.friction),
            ("restitution", self.restitution),
            ("linear damping", self.linear_damping),
            ("angular damping", self.angular_damping),
        ] {
            if !(value.is_finite() && value >= 0.0) {
                return Err(format!(
                    "{name} must be non-negative and finite, got {value}"
                ));
            }
        }
        if !self.position.is_finite() {
            return Err(format!("position must be finite, got {}", self.position));
        }
        if !(self.rotation.is_finite() && self.rotation.is_normalized()) {
            return Err(format!(
                "rotation must be finite and normalized, got {}",
                self.rotation
            ));
        }
        Ok(())
    }

    /// Panics on any field that would poison the simulation; see [`PhysicsWorld::spawn_loose`].
    fn assert_valid(&self) {
        if let Err(reason) = self.check() {
            panic!("{reason}");
        }
    }
}

/// A loose object as the game reads it back: plain data, no engine handle.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LooseState {
    pub shape: LooseShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub asleep: bool,
}

/// One loose object as a plain record: its id and the spawn that recreates it now.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LooseRecord {
    pub id: LooseId,
    /// The spawn that recreates the object now: its shape, mass, material and damping as
    /// spawned, with the pose and velocities it had when the record was taken.
    pub spawn: LooseSpawn,
}

/// Every loose object of a world, ascending id, and the next id the world will issue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LooseRecords {
    pub next_id: LooseId,
    pub objects: Vec<LooseRecord>,
}

/// Why a set of loose records cannot be restored.
#[derive(Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RestoreLooseError {
    #[error("loose ids must ascend strictly and stay below the next id; {0:?} does not")]
    BadId(LooseId),
    #[error("loose object {id:?}: {reason}")]
    BadSpawn { id: LooseId, reason: String },
    #[error("the next loose id {0:?} leaves no id to issue")]
    BadNextId(LooseId),
}

/// Where a loose object stood: the position and rotation a game draws it at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoosePose {
    pub position: Vec3,
    pub rotation: Quat,
}

/// A loose object's pose at the last two ticks, for drawing between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoosePoses {
    pub previous: LoosePose,
    pub current: LoosePose,
}

impl LoosePoses {
    /// `alpha` 0 is `previous`, 1 is `current`: position lerp, rotation shortest-arc slerp.
    /// `alpha` is clamped to [0, 1], and 0 and 1 return `previous` and `current` exactly.
    ///
    /// A NaN `alpha` is not mapped to anything: it passes through the clamp and the result has
    /// NaN components.
    pub fn blend(self, alpha: f32) -> LoosePose {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha == 0.0 {
            return self.previous;
        }
        if alpha == 1.0 {
            return self.current;
        }
        LoosePose {
            position: self.previous.position.lerp(self.current.position, alpha),
            // glam's slerp negates one end when the dot product is negative: the short way.
            rotation: self.previous.rotation.slerp(self.current.rotation, alpha),
        }
    }
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
    /// Boxes, capsules, cylinders and spheres are supported. Resting on ground within the
    /// world's [rest slope](PhysicsWorld::set_rest_slope) of level, the object is braked until
    /// it stops.
    ///
    /// Characters ignore loose objects: a loose object neither blocks a character nor is pushed
    /// by one. Rays skip loose objects because their colliders carry no tag; a
    /// [look](PhysicsWorld::pick_loose) finds them, unless a tagged surface is nearer.
    ///
    /// # Panics
    /// If the spawn would poison the simulation: a mass, half extent, half height or radius that
    /// is not positive and finite, a friction, restitution or damping that is negative or not
    /// finite, a position or velocity that is not finite, or a rotation that is not finite or
    /// whose squared length is more than 2e-4 from 1 (glam's `Quat::is_normalized`). Also if
    /// every id has been issued, so that no id is ever issued twice.
    pub fn spawn_loose(&mut self, spawn: &LooseSpawn) -> LooseId {
        spawn.assert_valid();
        let id = LooseId(self.next_loose);
        self.next_loose = self
            .next_loose
            .checked_add(1)
            .expect("every loose id has been issued");
        self.insert_loose(id, spawn);
        id
    }

    /// Insert the body and collider of an already validated spawn under `id`.
    fn insert_loose(&mut self, id: LooseId, spawn: &LooseSpawn) {
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
            .collision_groups(LOOSE_GROUPS)
            .user_data(encode_loose(id))
            .build();
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set);

        self.loose.insert(
            id,
            Loose {
                body: handle,
                spawn: *spawn,
                previous: LoosePose {
                    position: spawn.position,
                    rotation: spawn.rotation,
                },
            },
        );
    }

    /// The object's current state; `None` for an id never spawned or removed.
    pub fn loose(&self, id: LooseId) -> Option<LooseState> {
        Some(self.state_of(self.loose.get(&id)?))
    }

    /// Every live `Loose` owns a rigid body for its whole life, so the index cannot miss.
    fn state_of(&self, loose: &Loose) -> LooseState {
        let body = &self.rigid_body_set[loose.body];
        LooseState {
            shape: loose.spawn.shape,
            position: body.translation(),
            rotation: *body.rotation(),
            linear_velocity: body.linvel(),
            angular_velocity: body.angvel(),
            asleep: body.is_sleeping(),
        }
    }

    /// Every live loose object, ascending id; never depends on handle or hash order.
    ///
    /// Each entry equals what [`PhysicsWorld::loose`] reads for its id.
    pub fn loose_states(&self) -> impl Iterator<Item = (LooseId, LooseState)> + '_ {
        self.loose
            .iter()
            .map(|(&id, loose)| (id, self.state_of(loose)))
    }

    /// The object's pose at the last two ticks; `None` for an unknown id.
    pub fn loose_poses(&self, id: LooseId) -> Option<LoosePoses> {
        let loose = self.loose.get(&id)?;
        Some(LoosePoses {
            previous: loose.previous,
            current: body_pose(&self.rigid_body_set[loose.body]),
        })
    }

    /// Copy every loose body's pose, asleep or not, into `previous`: a sleeper skipped here
    /// would keep the pose from before it slept for ever.
    pub(super) fn remember_loose_poses(&mut self) {
        for loose in self.loose.values_mut() {
            loose.previous = body_pose(&self.rigid_body_set[loose.body]);
        }
    }

    /// Remove the object's body and collider.
    ///
    /// # Errors
    /// [`UnknownLoose`] if `id` was never spawned or was already removed.
    pub fn remove_loose(&mut self, id: LooseId) -> Result<(), UnknownLoose> {
        let handle = self.loose.remove(&id).ok_or(UnknownLoose)?.body;
        self.remove_body(handle);
        Ok(())
    }

    /// Every live loose object as a record, ascending id, with the next id the world will
    /// issue. Read at a tick boundary.
    ///
    /// Shape, mass, material and damping are those the object was spawned with; the pose and
    /// velocities are the body's now.
    pub fn loose_records(&self) -> LooseRecords {
        let objects = self
            .loose
            .iter()
            .map(|(&id, loose)| {
                let body = &self.rigid_body_set[loose.body];
                LooseRecord {
                    id,
                    spawn: LooseSpawn {
                        position: body.translation(),
                        rotation: *body.rotation(),
                        linear_velocity: body.linvel(),
                        angular_velocity: body.angvel(),
                        ..loose.spawn
                    },
                }
            })
            .collect();
        LooseRecords {
            next_id: LooseId(self.next_loose),
            objects,
        }
    }

    /// Rebuild loose objects from `records`, ready for the next tick; counts no tick.
    ///
    /// Every object comes back awake. The line supplies world meshes, gravity and rest slope
    /// again; they are not in the records.
    ///
    /// # Errors
    /// [`RestoreLooseError::BadNextId`] if the next id is `u64::MAX`, which no spawn could
    /// issue from without reusing an id;
    /// [`RestoreLooseError::BadId`] if ids do not ascend strictly or reach the next id;
    /// [`RestoreLooseError::BadSpawn`] if a record fails the checks of
    /// [`PhysicsWorld::spawn_loose`]. Every record is checked before any is inserted, so on
    /// error the world is unchanged.
    ///
    /// # Panics
    /// If the world already holds a loose object or has issued an id.
    pub fn restore_loose(&mut self, records: &LooseRecords) -> Result<(), RestoreLooseError> {
        assert!(
            self.next_loose == 0,
            "restore_loose needs a world that has issued no id, but this one already has"
        );
        if records.next_id.0 == u64::MAX {
            return Err(RestoreLooseError::BadNextId(records.next_id));
        }
        let mut previous: Option<LooseId> = None;
        for record in &records.objects {
            let ascends = previous.is_none_or(|last| record.id > last);
            if !ascends || record.id >= records.next_id {
                return Err(RestoreLooseError::BadId(record.id));
            }
            previous = Some(record.id);
        }
        for record in &records.objects {
            record
                .spawn
                .check()
                .map_err(|reason| RestoreLooseError::BadSpawn {
                    id: record.id,
                    reason,
                })?;
        }
        for record in &records.objects {
            self.insert_loose(record.id, &record.spawn);
        }
        self.next_loose = records.next_id.0;
        self.index_for_queries();
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
        let cut = self.gravity.length() * sim_math::tan(self.rest_slope) * dt;
        if cut == 0.0 {
            return;
        }
        let up = -self.gravity.normalize_or_zero();
        let cos_slope = sim_math::sin_cos(self.rest_slope).1;

        for loose in self.loose.values() {
            let roll = loose.spawn.shape.roll();
            let radius = match roll {
                Roll::None => continue,
                Roll::About { radius } | Roll::Ball { radius } => radius,
            };
            let body = &mut self.rigid_body_set[loose.body];
            if body.is_sleeping()
                || !rests_on_flat_ground(&self.narrow_phase, body.colliders()[0], up, cos_slope)
            {
                continue;
            }
            let (spin, velocity) = (body.angvel(), body.linvel());
            let (spin, velocity) = if let Roll::Ball { .. } = roll {
                // A ball rolls about any axis and cannot tip, so all its spin and all its
                // travel across up is braked.
                let vertical = up * velocity.dot(up);
                (
                    shrink(spin, cut / radius),
                    vertical + shrink(velocity - vertical, cut),
                )
            } else {
                // Only the rolling motion is braked: spin about the shape's axis and travel
                // across it. Tipping happens in the plane of the axis and up, so it is left
                // alone and a shape tipping over an edge cannot be frozen part-way.
                let axis = *body.rotation() * Vec3::Y;
                let rolling_spin = axis * spin.dot(axis);
                let across_axis = axis.cross(up).normalize_or_zero();
                let rolling_velocity = across_axis * velocity.dot(across_axis);
                (
                    spin - rolling_spin + shrink(rolling_spin, cut / radius),
                    velocity - rolling_velocity + shrink(rolling_velocity, cut),
                )
            };
            body.set_angvel(spin, false);
            body.set_linvel(velocity, false);
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
    use crate::world::test_support::{floor_mesh, slab_mesh, slope_mesh, x_wall_mesh, TEST_TAG};
    use crate::world::{HitTag, RayQuery};
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
            LooseShape::Sphere { radius } => radius,
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
            LooseShape::Sphere { radius } => Vec3::splat(radius),
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
            LooseShape::Sphere { radius: 0.1 },
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
        let radius = 0.12;
        let big_sphere = LooseShape::Sphere { radius };
        let shapes = every_shape().into_iter().chain([("sphere", big_sphere)]);
        for (name, shape) in shapes {
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

        let mut world = PhysicsWorld::new();
        let id = world.spawn_loose(&LooseSpawn::new(big_sphere, 1.0));
        let corner = Vec3::new(0.8 * radius, 0.8 * radius, 0.0);
        assert!(
            !probe(&world, id, corner),
            "a sphere is round: a point inside its bounding box, off the axes, is empty"
        );
        assert!(
            probe(&world, id, Vec3::new(0.9 * radius, 0.0, 0.0)),
            "a sphere fills its radius along an axis"
        );
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
        let shapes = every_shape().into_iter().chain([("sphere", SPHERE)]);
        for (name, shape) in shapes {
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
            ("sphere", SPHERE, 4.9),
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
        let rows = [
            ("capsule", CAPSULE, 5.5),
            ("cylinder", CYLINDER, 10.0),
            ("sphere", SPHERE, 10.0),
        ];

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
        let body = &mut world.rigid_body_set[handle.0];
        body.set_linvel(Vec3::new(0.0, 0.0, 2.0), true);
        body.set_angvel(Vec3::new(40.0, 0.0, 0.0), true);

        run(&mut world, 120);

        let speed = world.rigid_body_set[handle.0].linvel().length();
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
        for (name, shape) in [("cube", cube(0.05)), ("sphere", SPHERE)] {
            let fall = |rest_slope: f32| {
                let mut world = world_with_floor(&floor_mesh(0.0));
                world.set_rest_slope(rest_slope);
                let id = world.spawn_loose(&LooseSpawn {
                    restitution: 1.0,
                    position: Vec3::new(0.0, 1.0, 0.0),
                    ..LooseSpawn::new(shape, 0.2)
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

            assert_eq!(
                braked, free,
                "{name}: vertical velocity differs tick by tick"
            );
            let bounced = braked.iter().any(|&bits| f32::from_bits(bits) > 1.0);
            assert!(bounced, "the {name} bounced back up");
        }
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
        type Build = fn(u32) -> (PhysicsWorld, Vec<LooseId>);
        let builders: [(&str, Build); 2] = [
            ("every shape", pile_world),
            ("sphere and box", |seed| sphere_box_pile(seed, Vec3::ZERO)),
        ];

        for (name, build) in builders {
            let (mut first, first_ids) = build(3);
            let (mut second, second_ids) = build(3);

            for tick in 1..=600 {
                first.step(DT);
                second.step(DT);
                for (i, (&a, &b)) in first_ids.iter().zip(&second_ids).enumerate() {
                    let (a, b) = (read(&first, a), read(&second, b));
                    assert_eq!(
                        bits(&a),
                        bits(&b),
                        "{name} tick {tick} object {i}: state bits differ"
                    );
                    assert_eq!(
                        a.asleep, b.asleep,
                        "{name} tick {tick} object {i}: asleep differs"
                    );
                }
            }
            assert!(
                all_asleep(&first, &first_ids),
                "{name}: the pile came to rest, so the comparison covered the whole settle"
            );
        }
    }

    const SPHERE: LooseShape = LooseShape::Sphere { radius: 0.05 };

    /// Twenty undamped objects, a sphere and a box in turn, dropped 0.3 m apart above a 1 m
    /// square around `centre`.
    fn sphere_box_spawns(seed: u32, centre: Vec3) -> Vec<LooseSpawn> {
        let shapes = [SPHERE, cuboid(FLAT_BOX)];
        let mut dice = Dice::seeded(seed);
        (0..20)
            .map(|i| {
                let position =
                    centre + Vec3::new(dice.signed(0.5), 0.2 + i as f32 * 0.3, dice.signed(0.5));
                LooseSpawn {
                    position,
                    rotation: dice.rotation(),
                    ..LooseSpawn::new(shapes[i % 2], 0.2)
                }
            })
            .collect()
    }

    fn sphere_box_pile(seed: u32, centre: Vec3) -> (PhysicsWorld, Vec<LooseId>) {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids = sphere_box_spawns(seed, centre)
            .iter()
            .map(|spawn| world.spawn_loose(spawn))
            .collect();
        (world, ids)
    }

    /// A sphere on the floor rolling along +Z without slipping.
    fn rolling_sphere(speed: f32) -> LooseSpawn {
        let radius = cross_section(SPHERE);
        LooseSpawn {
            position: Vec3::new(0.0, radius, 0.0),
            linear_velocity: Vec3::new(0.0, 0.0, speed),
            angular_velocity: Vec3::new(speed / radius, 0.0, 0.0),
            ..LooseSpawn::new(SPHERE, 0.2)
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 16,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x777),
            ..ProptestConfig::default()
        })]

        #[test]
        fn a_spinning_sphere_comes_to_rest(
            spin in (-20.0f32..20.0, -20.0f32..20.0, -20.0f32..20.0),
        ) {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(0.0, 1.0, 0.0),
                angular_velocity: Vec3::new(spin.0, spin.1, spin.2),
                ..LooseSpawn::new(SPHERE, 0.2)
            });

            run(&mut world, 300);

            let state = world
                .loose(id)
                .ok_or_else(|| TestCaseError::fail("the spawned sphere reads back"))?;
            prop_assert!(state.asleep, "asleep after 5 s, state {state:?}");
            let bottom = bottom_of(&world, id);
            prop_assert!(bottom.abs() <= 0.001, "bottom is {bottom} from the floor");
        }
    }

    #[test]
    fn a_rolling_sphere_slows_every_tick_and_stops() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawn = LooseSpawn {
            position: Vec3::new(-4.0, cross_section(SPHERE), 0.0),
            ..rolling_sphere(2.0)
        };
        let id = world.spawn_loose(&spawn);
        let horizontal = |world: &PhysicsWorld| {
            let v = read(world, id).linear_velocity;
            Vec3::new(v.x, 0.0, v.z).length()
        };
        let mut previous = horizontal(&world);

        for tick in 1..=300 {
            world.step(DT);
            let speed = horizontal(&world);
            assert!(
                speed <= previous + 1e-3,
                "tick {tick}: horizontal speed rose from {previous} to {speed}"
            );
            previous = speed;
        }

        // The brake takes g*tan(5 deg) = 18 * 0.0875 = 1.575 m/s^2 off the speed, so a 2 m/s
        // roll covers v^2 / 2a = 1.27 m.
        let state = read(&world, id);
        let rolled = state.position.z;
        assert!(
            (1.15..=1.4).contains(&rolled),
            "the sphere rolls about 1.27 m before it stops, rolled {rolled}"
        );
        assert!(
            (state.position.x + 4.0).abs() <= 0.02,
            "the sphere rolls straight, x={}",
            state.position.x
        );
        assert!(state.asleep, "the sphere is at rest, state {state:?}");
    }

    #[test]
    fn spawn_rejects_a_bad_radius() {
        let rows: Vec<_> = BAD_POSITIVE
            .iter()
            .map(|&(name, radius)| {
                (
                    format!("{name} radius"),
                    LooseSpawn::new(LooseShape::Sphere { radius }, 1.0),
                )
            })
            .collect();

        assert_rows_rejected(&rows, "radius must be positive and finite");
    }

    #[test]
    fn a_pile_of_spheres_and_boxes_settles() {
        for seed in [1, 5, 8, 14] {
            let (mut world, ids) = sphere_box_pile(seed, Vec3::ZERO);

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
            assert!(
                ids.iter().any(|&id| bottom_of(&world, id) > 0.01),
                "seed {seed}: no object rests on another, the pile is flat"
            );
        }
    }

    #[test]
    fn three_stacked_boxes_stay_stacked() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids: Vec<_> = [0.05, 0.2, 0.35]
            .into_iter()
            .map(|y| {
                world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(0.0, y, 0.0),
                    ..LooseSpawn::new(cuboid(FLAT_BOX), 0.2)
                })
            })
            .collect();

        run(&mut world, 300);

        let heights: Vec<f32> = ids.iter().map(|&id| read(&world, id).position.y).collect();
        for (layer, pair) in heights.windows(2).enumerate() {
            let rise = pair[1] - pair[0];
            assert!(
                (rise - 2.0 * FLAT_BOX.y).abs() <= 0.004,
                "box {} rests a full height above box {layer}, centres {heights:?}",
                layer + 1
            );
        }
        assert!(
            (heights[2] - 5.0 * FLAT_BOX.y).abs() <= 0.008,
            "the top box centre is five half-heights up, centres {heights:?}"
        );
        assert!(all_asleep(&world, &ids), "all three boxes sleep");
    }

    mod walking_through_a_pile {
        use super::*;
        use crate::movement::{
            CharacterBody, CharacterCommand, Lean, LeanReach, MoveSettings, Stance,
        };
        use crate::world::test_support::{prone_body, test_settings};
        use crate::world::{CharacterHandle, CharacterSpawn, CharacterState};

        const WALK_TICKS: usize = 400;

        const PILE_CENTRE: Vec3 = Vec3::new(0.0, 0.0, 3.0);

        /// A body that crouches, goes prone and leans.
        fn body() -> CharacterBody {
            CharacterBody {
                lean: Lean {
                    upright: LeanReach {
                        distance: 0.3,
                        roll: 0.2,
                    },
                    prone: LeanReach {
                        distance: 0.2,
                        roll: 0.15,
                    },
                    head_radius: 0.1,
                },
                ..prone_body()
            }
        }

        fn settings() -> MoveSettings {
            MoveSettings {
                crouch_ticks: 12,
                prone_ticks: 12,
                lean_ticks: 8,
                ..test_settings()
            }
        }

        /// Along +Z: a quick approach, then a slow crawl through the pile while the walker
        /// crouches, goes prone with a lean, stands up and jumps twice, then a turn each way.
        fn command(t: usize) -> CharacterCommand {
            let (stance, lean) = match t {
                0..40 => (Stance::Stand, 0.0),
                40..100 => (Stance::Crouch, 0.0),
                100..180 => (Stance::Prone, 1.0),
                _ => (Stance::Stand, 0.0),
            };
            CharacterCommand {
                forward: match t {
                    0..30 => 1.0,
                    30..100 => 0.2,
                    100..180 => 0.05,
                    180..300 => 0.03,
                    _ => 0.5,
                },
                yaw_delta: match t {
                    300..340 => 0.02,
                    340..380 => -0.02,
                    _ => 0.0,
                },
                jump: t == 185 || t == 245,
                stance,
                lean,
                ..CharacterCommand::default()
            }
        }

        /// The world after 600 settling ticks, with the pile in it or not.
        fn settled(pile: bool) -> (PhysicsWorld, Vec<LooseId>) {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let ids = if pile {
                sphere_box_spawns(3, PILE_CENTRE)
                    .iter()
                    .map(|spawn| world.spawn_loose(spawn))
                    .collect()
            } else {
                Vec::new()
            };
            run(&mut world, 600);
            (world, ids)
        }

        fn add_walker(world: &mut PhysicsWorld) -> CharacterHandle {
            world.add_character(CharacterSpawn {
                position: Vec3::new(0.0, 1.0, 0.0),
                yaw: 0.0,
                pitch: 0.0,
                body: body(),
            })
        }

        fn walk_tick(
            world: &mut PhysicsWorld,
            walker: CharacterHandle,
            t: usize,
        ) -> CharacterState {
            world
                .move_character(walker, &command(t), &settings(), DT)
                .expect("the walker is alive");
            world.step(DT);
            world.character(walker).expect("the walker is alive")
        }

        /// Every field of the state, floats as bits.
        fn character_key(
            state: &CharacterState,
        ) -> (Vec<u32>, [bool; 2], [Stance; 2], u16, String) {
            let floats = [
                state.position.to_array().to_vec(),
                state.velocity.to_array().to_vec(),
                state.eye.to_array().to_vec(),
                vec![state.yaw, state.pitch, state.lean, state.roll],
            ]
            .concat()
            .into_iter()
            .map(f32::to_bits)
            .collect();
            (
                floats,
                [state.grounded, state.jump_held],
                [state.stance, state.from],
                state.progress,
                format!("{:?}", state.anchor),
            )
        }

        fn in_footprint(position: Vec3) -> bool {
            (position.x - PILE_CENTRE.x).abs() <= 0.5 && (position.z - PILE_CENTRE.z).abs() <= 0.5
        }

        /// Fewest ticks the walker's capsule must overlap a pile object: without that overlap
        /// the walk would pass the pile by and say nothing about the filter. The walk
        /// measures 201 ticks; half of that leaves room for a changed script.
        const MIN_TOUCHING: usize = 100;

        /// Whether the walker's capsule intersects any of `ids`, ignoring collision groups.
        fn touches_pile(world: &PhysicsWorld, walker: CharacterHandle, ids: &[LooseId]) -> bool {
            let capsule = &world.collider_set[world.rigid_body_set[walker.0].colliders()[0]];
            ids.iter().any(|&id| {
                let object = collider_of(world, id);
                rapier3d::parry::query::contact(
                    capsule.position(),
                    capsule.shape(),
                    object.position(),
                    object.shape(),
                    0.0,
                )
                .expect("both shapes support contact queries")
                .is_some()
            })
        }

        #[test]
        fn a_character_walking_through_a_pile_moves_as_without_it() {
            let (mut with_pile, pile_ids) = settled(true);
            let (mut without, _) = settled(false);
            let (walker, lone_walker) = (add_walker(&mut with_pile), add_walker(&mut without));
            let mut inside = 0;
            let mut prone_inside = false;
            let mut stood_up_inside = false;
            let mut touching = 0;

            for t in 0..WALK_TICKS {
                let among = walk_tick(&mut with_pile, walker, t);
                touching += usize::from(touches_pile(&with_pile, walker, &pile_ids));
                let alone = walk_tick(&mut without, lone_walker, t);

                assert_eq!(
                    character_key(&among),
                    character_key(&alone),
                    "tick {t}: the character differs, with {among:?}, without {alone:?}"
                );
                let within = in_footprint(among.position);
                inside += usize::from(within);
                prone_inside |= within && (100..180).contains(&t) && among.stance == Stance::Prone;
                stood_up_inside |= within && t == 180;
            }

            assert!(
                inside >= 15,
                "the walk covers the pile for only {inside} ticks"
            );
            assert!(
                touching >= MIN_TOUCHING,
                "the walker's capsule overlaps the pile for only {touching} ticks"
            );
            assert!(prone_inside, "the walker never lay prone inside the pile");
            assert!(
                stood_up_inside,
                "the walker did not stand up inside the pile"
            );
        }

        #[test]
        fn a_character_walking_through_a_pile_leaves_it_untouched() {
            let (mut walked_through, ids) = settled(true);
            let (mut left_alone, lone_ids) = settled(true);
            let walker = add_walker(&mut walked_through);
            assert!(
                all_asleep(&walked_through, &ids) && all_asleep(&left_alone, &lone_ids),
                "both piles are asleep before the walk starts"
            );
            let mut inside = 0;
            let mut touching = 0;

            for t in 0..WALK_TICKS {
                let walked = walk_tick(&mut walked_through, walker, t);
                inside += usize::from(in_footprint(walked.position));
                touching += usize::from(touches_pile(&walked_through, walker, &ids));
                left_alone.step(DT);

                for (i, (&a, &b)) in ids.iter().zip(&lone_ids).enumerate() {
                    let (touched, untouched) = (read(&walked_through, a), read(&left_alone, b));
                    assert_eq!(
                        bits(&touched),
                        bits(&untouched),
                        "tick {t} object {i}: the walker moved it"
                    );
                    assert!(touched.asleep, "tick {t} object {i}: the walker woke it");
                }
            }

            assert!(
                inside >= 15,
                "the walk covers the pile for only {inside} ticks"
            );
            assert!(
                touching >= MIN_TOUCHING,
                "the walker's capsule overlaps the pile for only {touching} ticks"
            );
        }

        #[test]
        fn a_box_dropped_over_a_standing_character_falls_as_without_it() {
            let drop = Vec3::new(0.0, 3.0, 0.0);
            let spawn = LooseSpawn {
                position: drop,
                ..LooseSpawn::new(cube(0.1), 0.5)
            };
            let mut both = world_with_floor(&floor_mesh(0.0));
            let mut box_only = world_with_floor(&floor_mesh(0.0));
            let mut walker_only = world_with_floor(&floor_mesh(0.0));
            let walker = add_walker(&mut both);
            let lone_walker = add_walker(&mut walker_only);
            let (dropped, lone_drop) = (both.spawn_loose(&spawn), box_only.spawn_loose(&spawn));
            let still = CharacterCommand::default();

            for tick in 0..300 {
                for (world, handle) in [(&mut both, walker), (&mut walker_only, lone_walker)] {
                    world
                        .move_character(handle, &still, &settings(), DT)
                        .expect("the walker is alive");
                }
                both.step(DT);
                box_only.step(DT);
                walker_only.step(DT);

                assert_eq!(
                    bits(&read(&both, dropped)),
                    bits(&read(&box_only, lone_drop)),
                    "tick {tick}: the character changed the box's fall"
                );
                assert_eq!(
                    character_key(&both.character(walker).expect("the walker is alive")),
                    character_key(&walker_only.character(lone_walker).expect("alive")),
                    "tick {tick}: the box changed the character"
                );
            }
            let landed = read(&both, dropped);
            assert!(
                landed.position.y < 0.2,
                "the box fell through to the floor, {landed:?}"
            );
        }

        #[test]
        fn a_restored_character_walking_through_a_pile_moves_as_without_it() {
            let restored_walker = |world: &mut PhysicsWorld| {
                let saved = add_walker(world);
                let record = world.character_record(saved).expect("the walker is alive");
                world.remove_character(saved).expect("the walker is alive");
                world.restore_character(&record, body())
            };
            let (mut with_pile, pile_ids) = settled(true);
            let (mut without, _) = settled(false);
            let (walker, lone_walker) = (
                restored_walker(&mut with_pile),
                restored_walker(&mut without),
            );
            let mut inside = 0;
            let mut touching = 0;

            for t in 0..WALK_TICKS {
                let among = walk_tick(&mut with_pile, walker, t);
                touching += usize::from(touches_pile(&with_pile, walker, &pile_ids));
                let alone = walk_tick(&mut without, lone_walker, t);

                assert_eq!(
                    character_key(&among),
                    character_key(&alone),
                    "tick {t}: the restored character differs, with {among:?}, without {alone:?}"
                );
                inside += usize::from(in_footprint(among.position));
            }

            assert!(
                inside >= 15,
                "the walk covers the pile for only {inside} ticks"
            );
            assert!(
                touching >= MIN_TOUCHING,
                "the walker's capsule overlaps the pile for only {touching} ticks"
            );
        }
    }

    #[test]
    fn a_spinning_sphere_comes_to_rest_on_a_resting_box() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let slab = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.1, 0.0),
            ..LooseSpawn::new(cuboid(Vec3::new(1.0, 0.1, 1.0)), 5.0)
        });
        run(&mut world, 120);
        assert!(read(&world, slab).asleep, "the slab sleeps before the drop");
        let ball = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.2, 1.0, 0.1),
            angular_velocity: Vec3::new(13.0, -17.0, 19.0),
            ..LooseSpawn::new(SPHERE, 0.2)
        });

        run(&mut world, 300);

        let state = read(&world, ball);
        let height = bottom_of(&world, ball) - 0.2;
        assert!(
            height.abs() <= 0.001,
            "the sphere's bottom is {height} from the slab top, state {state:?}"
        );
        assert!(state.asleep, "the sphere is awake, state {state:?}");
    }

    #[test]
    fn two_spheres_rolling_head_on_come_to_rest_asleep() {
        let radius = cross_section(SPHERE);
        let rolling = |x: f32, direction: f32| LooseSpawn {
            position: Vec3::new(x, radius, 1.0),
            linear_velocity: Vec3::new(2.0 * direction, 0.0, 0.0),
            angular_velocity: Vec3::new(0.0, 0.0, -2.0 * direction / radius),
            ..LooseSpawn::new(SPHERE, 0.2)
        };
        let mut world = world_with_floor(&floor_mesh(0.0));
        let ids = [
            world.spawn_loose(&rolling(-0.5, 1.0)),
            world.spawn_loose(&rolling(0.5, -1.0)),
        ];

        run(&mut world, 300);

        for id in ids {
            let state = read(&world, id);
            assert!(state.asleep, "a sphere is awake, state {state:?}");
        }
        let depth = overlap(&world, ids[0], ids[1]);
        assert!(depth <= 0.005, "the spheres end {depth} m into each other");
    }

    fn look(origin: Vec3, direction: Vec3) -> RayQuery {
        RayQuery {
            origin,
            direction,
            max_distance: 10.0,
            ignore: None,
        }
    }

    fn resting_box_world() -> (PhysicsWorld, LooseId) {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.2, 0.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        (world, id)
    }

    #[test]
    fn a_look_finds_a_resting_box() {
        let (mut world, id) = resting_box_world();
        run(&mut world, 300);
        let centre = read(&world, id).position;

        let found = world.pick_loose(&look(centre + Vec3::Y * 1.6, Vec3::NEG_Y));

        assert_eq!(found, Some(id));
    }

    #[test]
    fn a_wall_hides_a_box_from_a_look() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(3.0), WALL_TAG);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(4.0, 0.2, 0.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        run(&mut world, 60);
        let height = read(&world, id).position.y;
        let query = look(Vec3::new(2.0, height, 0.0), Vec3::X);
        let beyond_the_wall = world.pick_loose(&RayQuery {
            origin: Vec3::new(3.5, height, 0.0),
            ..query
        });

        let found = world.pick_loose(&query);
        let ray = world.cast_ray(&query).expect("the wall is in range");

        assert_eq!(beyond_the_wall, Some(id), "the box is there to be seen");
        assert_eq!(found, None);
        assert_eq!(ray.tag, WALL_TAG);
    }

    const WALL_TAG: HitTag = HitTag(7);

    #[test]
    fn looking_for_600_ticks_removes_nothing() {
        let (mut world, id) = resting_box_world();
        run(&mut world, 60);
        let query = look(Vec3::new(0.0, 1.8, 0.0), Vec3::NEG_Y);

        for tick in 1..=600 {
            world.step(DT);
            assert_eq!(world.pick_loose(&query), Some(id), "tick {tick}");
        }

        assert!(world.loose(id).is_some());
        assert_eq!(world.loose_count(), 1);
        assert_eq!(world.pick_loose(&query), Some(id));
    }

    fn three_apart() -> [LooseSpawn; 3] {
        let at = |z: f32, spin: Vec3| LooseSpawn {
            position: Vec3::new(0.0, 1.0, z),
            ..tumbling_spawn(spin, Vec3::new(1.0, 0.0, 0.0), 2.0)
        };
        [
            at(-3.0, Vec3::new(7.0, -3.0, 11.0)),
            at(0.0, Vec3::new(-5.0, 9.0, 2.0)),
            at(3.0, Vec3::new(3.0, 4.0, -8.0)),
        ]
    }

    #[test]
    fn removing_one_of_three_leaves_the_others_bit_equal() {
        let spawns = three_apart();
        for removed in 0..3 {
            let mut with = world_with_floor(&floor_mesh(0.0));
            let ids: Vec<LooseId> = spawns.iter().map(|s| with.spawn_loose(s)).collect();
            let mut without = world_with_floor(&floor_mesh(0.0));
            let kept: Vec<(LooseId, LooseId)> = (0..3)
                .filter(|&i| i != removed)
                .map(|i| (ids[i], without.spawn_loose(&spawns[i])))
                .collect();

            for tick in 1..=400 {
                with.step(DT);
                without.step(DT);
                if tick == 40 {
                    for &(kept_id, _) in &kept {
                        assert!(
                            !read(&with, kept_id).asleep,
                            "removed {removed}: a kept object is awake at tick 40"
                        );
                    }
                    with.remove_loose(ids[removed])
                        .expect("the object is alive");
                }
                for &(a, b) in &kept {
                    let (a, b) = (read(&with, a), read(&without, b));
                    assert_eq!(
                        bits(&a),
                        bits(&b),
                        "removed {removed}, tick {tick}: state bits differ"
                    );
                    assert_eq!(a.asleep, b.asleep, "removed {removed}, tick {tick}");
                }
            }
        }
    }

    #[test]
    fn a_look_at_a_removed_box_finds_the_floor() {
        let (mut world, id) = resting_box_world();
        run(&mut world, 60);
        let query = look(Vec3::new(0.0, 1.8, 0.0), Vec3::NEG_Y);
        let before = world.pick_loose(&query);

        world.remove_loose(id).expect("the box is alive");
        let at_once = world.pick_loose(&query);
        world.step(DT);
        let after_a_step = world.pick_loose(&query);
        let ray = world.cast_ray(&query).expect("the floor is in range");

        assert_eq!(before, Some(id), "the box is there to be seen");
        assert_eq!(at_once, None);
        assert_eq!(after_a_step, None);
        assert_eq!(ray.tag, TEST_TAG);
        assert!(ray.point.y.abs() <= 0.001, "hit the floor: {ray:?}");
    }

    #[test]
    fn removing_twice_changes_nothing() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawns = three_apart();
        let gone = world.spawn_loose(&spawns[0]);
        let other = world.spawn_loose(&spawns[1]);
        run(&mut world, 40);
        world.remove_loose(gone).expect("the object is alive");
        let mut ticks = 0;
        while ticks < 600 && !read(&world, other).asleep {
            world.step(DT);
            ticks += 1;
        }
        let state = read(&world, other);
        let count = world.loose_count();
        assert!(state.asleep, "the other object sleeps within 600 ticks");

        let second = world.remove_loose(gone);

        assert_eq!(second, Err(UnknownLoose));
        assert_eq!(read(&world, other), state);
        assert_eq!(world.loose_count(), count);
        assert_eq!(count, 1);
    }

    #[test]
    fn a_look_finds_every_shape_at_rest_by_its_own_id() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let shapes = every_shape().into_iter().chain([("sphere", SPHERE)]);
        let ids: Vec<(&str, LooseId)> = shapes
            .enumerate()
            .map(|(i, (name, shape))| {
                let spawn = LooseSpawn {
                    position: Vec3::new(i as f32 * 2.0, 0.5, 0.0),
                    ..LooseSpawn::new(shape, 0.2)
                };
                (name, world.spawn_loose(&spawn))
            })
            .collect();
        run(&mut world, 300);

        for (i, &(name, id)) in ids.iter().enumerate() {
            let centre = read(&world, id).position;
            let found = world.pick_loose(&look(centre + Vec3::Y, Vec3::NEG_Y));

            assert_eq!(found, Some(id), "{name} (object {i})");
        }
        let distinct: std::collections::HashSet<LooseId> = ids.iter().map(|&(_, id)| id).collect();
        assert_eq!(distinct.len(), ids.len(), "every object has its own id");
    }

    #[test]
    fn a_look_from_inside_a_box_finds_it() {
        let mut world = PhysicsWorld::new();
        world.set_gravity(0.0);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(1.0, 2.0, 3.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        world.step(DT);
        let centre = read(&world, id).position;
        let directions = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
            Vec3::new(1.0, 2.0, -3.0),
        ];

        for direction in directions {
            let found = world.pick_loose(&look(centre, direction));

            assert_eq!(found, Some(id), "direction {direction:?}");
        }
    }

    #[test]
    fn a_wall_orders_a_look_by_distance_to_the_box() {
        let box_at = |x: f32| {
            let mut world = PhysicsWorld::new();
            world.set_gravity(0.0);
            world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(3.0), WALL_TAG);
            let id = world.spawn_loose(&LooseSpawn {
                position: Vec3::new(x, 0.0, 0.0),
                ..LooseSpawn::new(cube(0.2), 1.0)
            });
            world.step(DT);
            (world, id)
        };
        let query = look(Vec3::ZERO, Vec3::X);
        let (in_front, front_id) = box_at(2.0);
        let (behind, _) = box_at(4.0);

        let seen = in_front.pick_loose(&query);
        let ray = in_front.cast_ray(&query).expect("the wall is in range");
        let hidden = behind.pick_loose(&query);

        assert_eq!(seen, Some(front_id));
        assert_eq!(ray.tag, WALL_TAG);
        assert!((ray.distance - 3.0).abs() <= 0.001, "{ray:?}");
        assert_eq!(hidden, None);
    }

    /// `loose_records`, postcard to bytes and back, `restore_loose` into a fresh world with the
    /// same floor.
    fn restored(world: &PhysicsWorld) -> PhysicsWorld {
        let bytes = postcard::to_allocvec(&world.loose_records()).expect("records serialise");
        let records: LooseRecords = postcard::from_bytes(&bytes).expect("records deserialise");
        let mut fresh = world_with_floor(&floor_mesh(0.0));
        fresh.restore_loose(&records).expect("the records restore");
        fresh
    }

    fn record(id: u64, spawn: LooseSpawn) -> LooseRecord {
        LooseRecord {
            id: LooseId(id),
            spawn,
        }
    }

    fn records(next_id: u64, objects: Vec<LooseRecord>) -> LooseRecords {
        LooseRecords {
            next_id: LooseId(next_id),
            objects,
        }
    }

    fn every_shape_with_sphere() -> [(&'static str, LooseShape); 4] {
        let [a, b, c] = every_shape();
        [a, b, c, ("sphere", SPHERE)]
    }

    #[test]
    fn a_restored_object_has_its_saved_state_at_the_load_tick() {
        for (name, shape) in every_shape_with_sphere() {
            for saved_at in [5, 40, 120] {
                let mut world = world_with_floor(&floor_mesh(0.0));
                let tumbling = world.spawn_loose(&LooseSpawn {
                    shape,
                    ..tumbling_spawn(Vec3::new(7.0, -3.0, 11.0), Vec3::new(2.0, 0.0, -1.5), 4.0)
                });
                let sliding = world.spawn_loose(&LooseSpawn {
                    position: Vec3::new(-1.0, 0.2, 1.0),
                    linear_velocity: Vec3::new(3.0, 0.0, 0.5),
                    ..LooseSpawn::new(shape, 0.2)
                });
                run(&mut world, saved_at);

                let back = restored(&world);

                assert_eq!(back.loose_count(), 2, "{name} at tick {saved_at}");
                for id in [tumbling, sliding] {
                    assert_eq!(
                        bits(&read(&back, id)),
                        bits(&read(&world, id)),
                        "{name} at tick {saved_at}: {id:?} differs"
                    );
                }
            }
        }
    }

    #[test]
    fn restoring_after_a_removal_continues_the_ids() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        for _ in 0..5 {
            world.spawn_loose(&flat_box_spawn());
        }
        world.remove_loose(LooseId(2)).expect("the object is alive");

        let mut back = restored(&world);
        let next = back.spawn_loose(&flat_box_spawn());

        let ids: Vec<u64> = back
            .loose_records()
            .objects
            .iter()
            .map(|object| object.id.get())
            .collect();
        assert_eq!(ids, [0, 1, 3, 4, 5]);
        assert_eq!(next, LooseId(5));
        assert!(back.loose(LooseId(2)).is_none(), "id 2 stays removed");
    }

    #[test]
    fn restoring_no_objects_still_sets_the_next_id() {
        let mut world = world_with_floor(&floor_mesh(0.0));

        world
            .restore_loose(&records(5, Vec::new()))
            .expect("empty records restore");

        assert_eq!(world.loose_count(), 0);
        assert_eq!(world.spawn_loose(&flat_box_spawn()), LooseId(5));
    }

    #[test]
    fn a_restored_pile_settles_again() {
        for seed in 0..16 {
            let (mut world, ids) = pile_world(seed);
            let mut ticks = 0;
            while ticks < 600 && !all_asleep(&world, &ids) {
                world.step(DT);
                ticks += 1;
            }
            assert!(all_asleep(&world, &ids), "seed {seed}: the pile sleeps");
            run(&mut world, 10);
            let saved: Vec<Vec3> = ids.iter().map(|&id| read(&world, id).position).collect();

            let mut back = restored(&world);
            run(&mut back, 120);

            assert!(
                all_asleep(&back, &ids),
                "seed {seed}: the restored pile is not asleep"
            );
            for (&id, saved_at) in ids.iter().zip(&saved) {
                let moved = read(&back, id).position.distance(*saved_at);
                assert!(
                    moved <= 0.05,
                    "seed {seed}: {id:?} moved {moved} m from its saved position"
                );
            }
        }
    }

    #[test]
    fn a_look_finds_a_restored_object_before_the_first_step() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        for _ in 0..2 {
            let spare = world.spawn_loose(&flat_box_spawn());
            world.remove_loose(spare).expect("the object is alive");
        }
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 0.2, 0.0),
            ..LooseSpawn::new(cube(0.2), 1.0)
        });
        assert_eq!(id, LooseId(2));
        run(&mut world, 300);
        let centre = read(&world, id).position;
        let query = look(centre + Vec3::Y * 1.6, Vec3::NEG_Y);

        let mut back = restored(&world);

        let before_a_step = back.pick_loose(&query);
        back.step(DT);
        let after_a_step = back.pick_loose(&query);
        assert_eq!(before_a_step, Some(id));
        assert_eq!(after_a_step, Some(id));
    }

    #[test]
    fn a_restored_pile_still_stands_on_its_floor_when_knocked() {
        for seed in 0..4 {
            let (mut world, ids) = pile_world(seed);
            run(&mut world, 300);
            let mut back = restored(&world);
            let mut ticks = 0;
            while ticks < 300 && !all_asleep(&back, &ids) {
                back.step(DT);
                ticks += 1;
            }
            assert!(all_asleep(&back, &ids), "seed {seed}: re-settles");
            let knocked = ids[ids.len() - 1];
            let handle = back.loose[&knocked].body;
            let velocity = read(&back, knocked).linear_velocity + Vec3::X;
            back.rigid_body_set[handle].set_linvel(velocity, true);

            run(&mut back, 300);

            for &id in &ids {
                let bottom = bottom_of(&back, id);
                assert!(bottom > -0.01, "seed {seed}: {id:?} fell, bottom {bottom}");
            }
            assert!(
                all_asleep(&back, &ids),
                "seed {seed}: the knocked pile is not asleep"
            );
        }
    }

    #[test]
    fn a_restored_collider_matches_a_spawned_one() {
        for (name, shape) in every_shape_with_sphere() {
            let mut world = world_with_floor(&floor_mesh(0.0));
            let spare = world.spawn_loose(&flat_box_spawn());
            world.remove_loose(spare).expect("the object is alive");
            let id = world.spawn_loose(&LooseSpawn {
                friction: 0.3,
                restitution: 0.2,
                linear_damping: 0.1,
                angular_damping: 0.4,
                position: Vec3::new(0.5, 1.0, -0.5),
                linear_velocity: Vec3::new(0.5, 0.0, 1.0),
                ..LooseSpawn::new(shape, 3.5)
            });
            run(&mut world, 10);

            let back = restored(&world);

            let (was, is) = (collider_of(&world, id), collider_of(&back, id));
            assert_eq!(is.mass(), was.mass(), "{name}: mass");
            assert_eq!(is.friction(), was.friction(), "{name}: friction");
            assert_eq!(is.restitution(), was.restitution(), "{name}: restitution");
            assert_eq!(
                is.collision_groups(),
                was.collision_groups(),
                "{name}: collision groups"
            );
            assert_eq!(is.user_data, was.user_data, "{name}: user data");
            assert_eq!(is.compute_aabb(), was.compute_aabb(), "{name}: bounds");
            let (was, is) = (body_of(&world, id), body_of(&back, id));
            assert_eq!(is.linear_damping(), was.linear_damping(), "{name}");
            assert_eq!(is.angular_damping(), was.angular_damping(), "{name}");
            assert_eq!(is.mass(), was.mass(), "{name}: body mass");
        }
    }

    #[test]
    fn loose_records_lists_live_objects_ascending_with_the_next_id() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawns = [
            LooseSpawn::new(cuboid(FLAT_BOX), 0.5),
            LooseSpawn {
                position: Vec3::new(1.0, 1.0, 0.0),
                ..LooseSpawn::new(CAPSULE, 0.7)
            },
            LooseSpawn {
                position: Vec3::new(-1.0, 1.0, 0.0),
                friction: 0.9,
                ..LooseSpawn::new(SPHERE, 0.9)
            },
        ];
        let ids: Vec<LooseId> = spawns.iter().map(|s| world.spawn_loose(s)).collect();
        world.remove_loose(ids[1]).expect("the object is alive");
        run(&mut world, 20);

        let records = world.loose_records();

        assert_eq!(records.next_id, LooseId(3));
        let listed: Vec<LooseId> = records.objects.iter().map(|o| o.id).collect();
        assert_eq!(listed, [ids[0], ids[2]]);
        for (object, original) in records.objects.iter().zip([spawns[0], spawns[2]]) {
            let state = read(&world, object.id);
            let spawn = object.spawn;
            assert_eq!(spawn.shape, original.shape);
            assert_eq!(spawn.mass, original.mass);
            assert_eq!(spawn.friction, original.friction);
            assert_eq!(spawn.restitution, original.restitution);
            assert_eq!(spawn.linear_damping, original.linear_damping);
            assert_eq!(spawn.angular_damping, original.angular_damping);
            assert_eq!(spawn.position, state.position);
            assert_eq!(spawn.rotation, state.rotation);
            assert_eq!(spawn.linear_velocity, state.linear_velocity);
            assert_eq!(spawn.angular_velocity, state.angular_velocity);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 16,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x891),
            ..ProptestConfig::default()
        })]

        #[test]
        fn records_round_trip_through_a_restore(
            ops in proptest::collection::vec((0u8..3, 0usize..8, 1u32..30), 0..20)
        ) {
            let shapes = every_shape_with_sphere();
            let mut world = world_with_floor(&floor_mesh(0.0));
            let mut live: Vec<LooseId> = Vec::new();
            for (kind, pick, ticks) in ops {
                match kind {
                    0 => {
                        let shape = shapes[pick % shapes.len()].1;
                        live.push(world.spawn_loose(&LooseSpawn {
                            shape,
                            position: Vec3::new(pick as f32 * 0.3, 1.0 + ticks as f32 * 0.05, 0.0),
                            angular_velocity: Vec3::new(ticks as f32 * 0.2, 1.0, -2.0),
                            ..LooseSpawn::new(shape, 0.2)
                        }));
                    }
                    1 if !live.is_empty() => {
                        let id = live.remove(pick % live.len());
                        prop_assert_eq!(world.remove_loose(id), Ok(()));
                    }
                    _ => run(&mut world, ticks),
                }
            }

            let back = restored(&world);

            prop_assert_eq!(back.loose_records(), world.loose_records());
        }
    }

    /// One record of each shape, ids 0, 3, 5 and 6, `next_id` 7. Every float is exact in binary.
    fn pinned_fixture() -> LooseRecords {
        records(
            7,
            vec![
                record(
                    0,
                    LooseSpawn {
                        shape: LooseShape::Box {
                            half_extents: Vec3::new(0.5, 0.25, 0.125),
                        },
                        mass: 2.0,
                        friction: 0.5,
                        restitution: 0.25,
                        linear_damping: 0.0,
                        angular_damping: 1.0,
                        position: Vec3::new(1.0, 2.0, 3.0),
                        rotation: Quat::from_xyzw(0.0, 0.0, 0.0, 1.0),
                        linear_velocity: Vec3::new(-1.0, 0.5, 0.0),
                        angular_velocity: Vec3::new(0.0, 4.0, -8.0),
                    },
                ),
                record(
                    3,
                    LooseSpawn {
                        shape: LooseShape::Capsule {
                            half_height: 0.5,
                            radius: 0.25,
                        },
                        mass: 1.0,
                        friction: 0.75,
                        restitution: 0.0,
                        linear_damping: 0.5,
                        angular_damping: 0.25,
                        position: Vec3::new(0.0, 0.5, 0.0),
                        rotation: Quat::from_xyzw(0.0, 1.0, 0.0, 0.0),
                        linear_velocity: Vec3::new(0.0, 0.0, 2.0),
                        angular_velocity: Vec3::new(1.0, 0.0, 0.0),
                    },
                ),
                record(
                    5,
                    LooseSpawn {
                        shape: LooseShape::Cylinder {
                            half_height: 0.125,
                            radius: 0.5,
                        },
                        mass: 4.0,
                        friction: 0.25,
                        restitution: 0.5,
                        linear_damping: 0.25,
                        angular_damping: 0.5,
                        position: Vec3::new(-2.0, 0.125, 1.0),
                        rotation: Quat::from_xyzw(1.0, 0.0, 0.0, 0.0),
                        linear_velocity: Vec3::new(0.0, -1.0, 0.0),
                        angular_velocity: Vec3::new(0.0, 0.0, 2.0),
                    },
                ),
                record(
                    6,
                    LooseSpawn {
                        shape: LooseShape::Sphere { radius: 0.75 },
                        mass: 8.0,
                        friction: 1.0,
                        restitution: 0.125,
                        linear_damping: 0.0,
                        angular_damping: 0.0,
                        position: Vec3::new(0.0, 0.75, -4.0),
                        rotation: Quat::from_xyzw(0.0, 0.0, 1.0, 0.0),
                        linear_velocity: Vec3::new(4.0, 0.0, 0.0),
                        angular_velocity: Vec3::new(0.0, -0.5, 0.0),
                    },
                ),
            ],
        )
    }

    /// Next id 7, four objects, one of each shape; each is its id, the shape's variant index and fields, then the
    /// spawn's f32s in little-endian order.
    const PINNED: [u8; 330] = [
        7, 4, 0, 0, 0, 0, 0, 63, 0, 0, 128, 62, 0, 0, 0, 62, 0, 0, 0, 64, 0, 0, 0, 63, 0, 0, 128,
        62, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 128, 63, 0, 0, 0, 64, 0, 0, 64, 64, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 128, 191, 0, 0, 0, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        128, 64, 0, 0, 0, 193, 3, 1, 0, 0, 0, 63, 0, 0, 128, 62, 0, 0, 128, 63, 0, 0, 64, 63, 0, 0,
        0, 0, 0, 0, 0, 63, 0, 0, 128, 62, 0, 0, 0, 0, 0, 0, 0, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        128, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 128, 63, 0, 0,
        0, 0, 0, 0, 0, 0, 5, 2, 0, 0, 0, 62, 0, 0, 0, 63, 0, 0, 128, 64, 0, 0, 128, 62, 0, 0, 0,
        63, 0, 0, 128, 62, 0, 0, 0, 63, 0, 0, 0, 192, 0, 0, 0, 62, 0, 0, 128, 63, 0, 0, 128, 63, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 191, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 64, 6, 3, 0, 0, 64, 63, 0, 0, 0, 65, 0, 0, 128, 63, 0, 0, 0, 62, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 63, 0, 0, 128, 192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128,
        63, 0, 0, 0, 0, 0, 0, 128, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 191, 0, 0, 0,
        0,
    ];

    #[test]
    fn loose_records_bytes_are_pinned() {
        let fixture = pinned_fixture();

        let bytes = postcard::to_allocvec(&fixture).expect("the records serialise");

        assert_eq!(bytes, PINNED);
        let back: LooseRecords = postcard::from_bytes(&PINNED).expect("the pinned bytes decode");
        assert_eq!(back, fixture);
    }

    #[test]
    fn restore_rejects_bad_ids() {
        let spawn = flat_box_spawn();
        let rows: [(&str, LooseRecords, u64); 5] = [
            (
                "duplicate",
                records(5, vec![record(1, spawn), record(1, spawn)]),
                1,
            ),
            (
                "descending",
                records(5, vec![record(2, spawn), record(1, spawn)]),
                1,
            ),
            ("equal to next", records(3, vec![record(3, spawn)]), 3),
            (
                "above next",
                records(3, vec![record(0, spawn), record(9, spawn)]),
                9,
            ),
            (
                "second equal to next",
                records(2, vec![record(0, spawn), record(2, spawn)]),
                2,
            ),
        ];

        for (name, bad, culprit) in rows {
            let mut world = world_with_floor(&floor_mesh(0.0));

            let outcome = world.restore_loose(&bad);

            assert_eq!(
                outcome,
                Err(RestoreLooseError::BadId(LooseId(culprit))),
                "{name}"
            );
            assert_eq!(world.loose_count(), 0, "{name}: the world is unchanged");
            assert_eq!(
                world.spawn_loose(&spawn),
                LooseId(0),
                "{name}: ids unchanged"
            );
        }
    }

    #[test]
    fn restore_rejects_a_bad_spawn_and_restores_nothing() {
        let valid = flat_box_spawn();
        let bad = LooseSpawn {
            position: Vec3::new(f32::NAN, 1.0, 0.0),
            ..valid
        };
        let expected_reason = spawn_panic_message(&bad).expect("a NaN position is rejected");
        let mut world = world_with_floor(&floor_mesh(0.0));

        let outcome = world.restore_loose(&records(
            7,
            vec![
                record(0, valid),
                record(3, valid),
                record(5, bad),
                record(6, valid),
            ],
        ));

        assert_eq!(
            outcome,
            Err(RestoreLooseError::BadSpawn {
                id: LooseId(5),
                reason: expected_reason,
            })
        );
        assert_eq!(world.loose_count(), 0);
        assert_eq!(world.spawn_loose(&valid), LooseId(0));
    }

    #[test]
    #[should_panic(expected = "already")]
    fn restoring_into_a_used_world_panics() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&flat_box_spawn());
        world.remove_loose(id).expect("the object is alive");

        let _ = world.restore_loose(&records(1, Vec::new()));
    }

    #[test]
    fn a_long_spin_still_restores_with_a_bit_equal_rotation() {
        let mut world = PhysicsWorld::new();
        world.set_gravity(0.0);
        let id = world.spawn_loose(&LooseSpawn {
            position: Vec3::new(0.0, 50.0, 0.0),
            angular_velocity: Vec3::new(40.0, 25.0, -33.0),
            ..LooseSpawn::new(cuboid(FLAT_BOX), 0.2)
        });
        run(&mut world, 3000);
        let bytes = postcard::to_allocvec(&world.loose_records()).expect("records serialise");
        let saved: LooseRecords = postcard::from_bytes(&bytes).expect("records deserialise");
        let mut fresh = world_with_floor(&floor_mesh(0.0));

        let outcome = fresh.restore_loose(&saved);

        assert_eq!(outcome, Ok(()));
        assert_eq!(
            read(&fresh, id).rotation.to_array().map(f32::to_bits),
            read(&world, id).rotation.to_array().map(f32::to_bits)
        );
    }

    #[test]
    fn the_highest_ids_restore_and_a_next_id_that_leaves_none_to_issue_is_bad() {
        let spawn = flat_box_spawn();
        let mut world = world_with_floor(&floor_mesh(0.0));

        let outcome =
            world.restore_loose(&records(u64::MAX - 1, vec![record(u64::MAX - 2, spawn)]));

        assert_eq!(outcome, Ok(()));
        assert!(world.loose(LooseId(u64::MAX - 2)).is_some());
        assert_eq!(world.loose_records().next_id, LooseId(u64::MAX - 1));
        let mut other = world_with_floor(&floor_mesh(0.0));
        assert_eq!(
            other.restore_loose(&records(u64::MAX - 1, vec![record(u64::MAX - 1, spawn)])),
            Err(RestoreLooseError::BadId(LooseId(u64::MAX - 1)))
        );
        assert_eq!(
            other.restore_loose(&records(u64::MAX, vec![record(u64::MAX - 1, spawn)])),
            Err(RestoreLooseError::BadNextId(LooseId(u64::MAX)))
        );
        assert_eq!(other.loose_count(), 0);
        assert_eq!(other.spawn_loose(&spawn), LooseId(0), "no id was issued");
    }

    #[test]
    fn a_restored_object_draws_at_its_restored_pose_until_the_next_step() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&tumbling_spawn(
            Vec3::new(7.0, -3.0, 11.0),
            Vec3::new(2.0, 0.0, -1.5),
            4.0,
        ));
        run(&mut world, 30);
        let saved = world.loose(id).expect("the object is alive");

        let back = restored(&world);

        let poses = back.loose_poses(id).expect("the restored object has poses");
        assert_eq!(
            poses.previous, poses.current,
            "no jump on the first drawn frame"
        );
        assert_eq!(poses.current.position, saved.position);
        assert_eq!(poses.current.rotation, saved.rotation);
    }

    #[test]
    fn a_world_restored_at_the_top_issues_the_last_id_without_reusing_one() {
        let spawn = flat_box_spawn();
        let mut world = world_with_floor(&floor_mesh(0.0));
        world
            .restore_loose(&records(u64::MAX - 1, vec![record(0, spawn)]))
            .expect("the records restore");

        let last = world.spawn_loose(&spawn);

        assert_eq!(last, LooseId(u64::MAX - 1));
        assert_eq!(world.loose_count(), 2);
        assert!(world.loose(LooseId(0)).is_some(), "id 0 is untouched");
    }

    #[test]
    #[should_panic(expected = "every loose id has been issued")]
    fn spawning_after_the_last_id_panics_rather_than_reuse_one() {
        let spawn = flat_box_spawn();
        let mut world = world_with_floor(&floor_mesh(0.0));
        world
            .restore_loose(&records(u64::MAX - 1, vec![record(0, spawn)]))
            .expect("the records restore");
        world.spawn_loose(&spawn);

        world.spawn_loose(&spawn);
    }

    #[test]
    fn a_restored_object_can_be_removed_and_the_next_id_stays() {
        let spawn = flat_box_spawn();
        let mut world = world_with_floor(&floor_mesh(0.0));
        world
            .restore_loose(&records(5, vec![record(1, spawn), record(3, spawn)]))
            .expect("the records restore");

        let removal = world.remove_loose(LooseId(3));

        assert_eq!(removal, Ok(()));
        let after = world.loose_records();
        assert_eq!(after.next_id, LooseId(5));
        let ids: Vec<LooseId> = after.objects.iter().map(|o| o.id).collect();
        assert_eq!(ids, [LooseId(1)]);
        assert_eq!(world.remove_loose(LooseId(3)), Err(UnknownLoose));
    }

    #[test]
    #[should_panic(expected = "already")]
    fn a_second_restore_into_the_same_world_panics() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world
            .restore_loose(&records(2, vec![record(0, flat_box_spawn())]))
            .expect("the records restore");

        let _ = world.restore_loose(&records(2, vec![record(1, flat_box_spawn())]));
    }

    #[test]
    #[should_panic(expected = "already")]
    fn restoring_empty_records_with_a_next_id_counts_as_issuing_ids() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        world
            .restore_loose(&records(3, Vec::new()))
            .expect("empty records restore");

        let _ = world.restore_loose(&records(0, Vec::new()));
    }

    fn poses_of(world: &PhysicsWorld, id: LooseId) -> LoosePoses {
        let poses = world.loose_poses(id);
        assert!(poses.is_some(), "a live object has poses");
        poses.unwrap_or_else(|| unreachable!("asserted just above"))
    }

    fn pose_bits(pose: &LoosePose) -> [u32; 7] {
        let [px, py, pz] = pose.position.to_array();
        let [rx, ry, rz, rw] = pose.rotation.to_array();
        [px, py, pz, rx, ry, rz, rw].map(f32::to_bits)
    }

    fn pose_of(state: &LooseState) -> LoosePose {
        LoosePose {
            position: state.position,
            rotation: state.rotation,
        }
    }

    /// The rotation angle, radians, between two unit quaternions, by the short way.
    fn angle_between(a: Quat, b: Quat) -> f32 {
        2.0 * a.dot(b).abs().clamp(0.0, 1.0).acos()
    }

    /// A tumbling box after two ticks in the air, with a turn between its poses.
    fn tumbling_poses() -> LoosePoses {
        let mut world = PhysicsWorld::new();
        let id = world.spawn_loose(&tumbling_spawn(
            Vec3::new(7.0, -3.0, 11.0),
            Vec3::new(2.0, 0.0, -1.5),
            4.0,
        ));
        run(&mut world, 2);
        let poses = poses_of(&world, id);
        assert!(
            angle_between(poses.previous.rotation, poses.current.rotation) > 0.1,
            "the box turned between ticks, {poses:?}"
        );
        poses
    }

    #[test]
    fn the_previous_pose_is_the_last_ticks_pose() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&tumbling_spawn(
            Vec3::new(7.0, -3.0, 11.0),
            Vec3::new(2.0, 0.0, -1.5),
            4.0,
        ));

        for tick in 1..=120 {
            let before = pose_of(&read(&world, id));
            world.step(DT);
            let after = pose_of(&read(&world, id));

            let poses = poses_of(&world, id);
            assert_eq!(
                pose_bits(&poses.previous),
                pose_bits(&before),
                "tick {tick}: previous is the pose before the step"
            );
            assert_eq!(
                pose_bits(&poses.current),
                pose_bits(&after),
                "tick {tick}: current is the pose after the step"
            );
        }
    }

    #[test]
    fn a_new_object_has_no_jump() {
        let mut world = PhysicsWorld::new();
        let spawn = LooseSpawn {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7),
            ..flat_box_spawn()
        };

        let id = world.spawn_loose(&spawn);

        let poses = poses_of(&world, id);
        assert_eq!(pose_bits(&poses.previous), pose_bits(&poses.current));
        assert!(
            poses.current.position.abs_diff_eq(spawn.position, 1e-6),
            "{poses:?}"
        );
        assert!(
            poses.current.rotation.abs_diff_eq(spawn.rotation, 1e-6),
            "{poses:?}"
        );
    }

    #[test]
    fn a_sleeping_object_stands_still() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let id = world.spawn_loose(&flat_box_spawn());
        let mut ticks = 0;
        while ticks < 600 && !read(&world, id).asleep {
            world.step(DT);
            ticks += 1;
        }
        assert!(read(&world, id).asleep, "the box sleeps within 600 ticks");

        world.step(DT);

        assert!(read(&world, id).asleep, "the box is still asleep");
        let poses = poses_of(&world, id);
        assert_eq!(pose_bits(&poses.previous), pose_bits(&poses.current));
    }

    #[test]
    fn blend_runs_from_previous_to_current() {
        let poses = tumbling_poses();
        let total = angle_between(poses.previous.rotation, poses.current.rotation);

        let start = poses.blend(0.0);
        let end = poses.blend(1.0);
        let middle = poses.blend(0.5);

        assert_eq!(pose_bits(&start), pose_bits(&poses.previous));
        assert_eq!(pose_bits(&end), pose_bits(&poses.current));
        let midpoint = (poses.previous.position + poses.current.position) * 0.5;
        assert!(
            middle.position.abs_diff_eq(midpoint, 1e-6),
            "position {} is not the midpoint {midpoint}",
            middle.position
        );
        let to_previous = angle_between(middle.rotation, poses.previous.rotation);
        let to_current = angle_between(middle.rotation, poses.current.rotation);
        assert!(
            (to_previous - total / 2.0).abs() <= 1e-4,
            "half the turn is {}, got {to_previous}",
            total / 2.0
        );
        assert!(
            (to_current - total / 2.0).abs() <= 1e-4,
            "half the turn is {}, got {to_current}",
            total / 2.0
        );
    }

    proptest! {
        #[test]
        fn blend_takes_the_short_way_round(
            from in (-3.1f32..3.1, -3.1f32..3.1, -3.1f32..3.1),
            to in (-3.1f32..3.1, -3.1f32..3.1, -3.1f32..3.1),
            alpha in 0.0f32..=1.0,
        ) {
            let euler = |(x, y, z): (f32, f32, f32)| {
                Quat::from_euler(glam::EulerRot::XYZ, x, y, z).normalize()
            };
            let previous = euler(from);
            let mut current = euler(to);
            if previous.dot(current) > 0.0 {
                current = -current;
            }
            prop_assume!(previous.dot(current) < -1e-3);
            let poses = LoosePoses {
                previous: LoosePose { position: Vec3::ZERO, rotation: previous },
                current: LoosePose { position: Vec3::ZERO, rotation: current },
            };
            let between = angle_between(previous, current);

            let blended = poses.blend(alpha).rotation;

            let to_previous = angle_between(blended, previous);
            let to_current = angle_between(blended, current);
            prop_assert!(to_previous <= between + 2e-3, "{to_previous} > {between}");
            prop_assert!(to_current <= between + 2e-3, "{to_current} > {between}");
            prop_assert!(
                (to_previous - alpha * between).abs() <= 2e-3,
                "alpha {alpha}: {to_previous} from previous, expected {}",
                alpha * between
            );
        }
    }

    #[test]
    fn blend_clamps_alpha() {
        let poses = tumbling_poses();

        let below = poses.blend(-1.0);
        let above = poses.blend(2.0);

        assert_eq!(pose_bits(&below), pose_bits(&poses.blend(0.0)));
        assert_eq!(pose_bits(&below), pose_bits(&poses.previous));
        assert_eq!(pose_bits(&above), pose_bits(&poses.blend(1.0)));
        assert_eq!(pose_bits(&above), pose_bits(&poses.current));
    }

    #[test]
    fn removed_objects_have_no_poses() {
        let mut world = PhysicsWorld::new();
        let id = world.spawn_loose(&flat_box_spawn());
        let before = world.loose_poses(id);
        world.remove_loose(id).expect("the object is alive");

        let after = world.loose_poses(id);

        assert!(before.is_some(), "a live object has poses");
        assert_eq!(after, None);
    }

    #[test]
    fn an_id_never_spawned_has_no_poses() {
        let mut larger = PhysicsWorld::new();
        let mut smaller = PhysicsWorld::new();
        larger.spawn_loose(&flat_box_spawn());
        larger.spawn_loose(&flat_box_spawn());
        let foreign = larger.spawn_loose(&flat_box_spawn());
        let own = smaller.spawn_loose(&flat_box_spawn());

        let from_larger_world = smaller.loose_poses(foreign);
        let beyond_issued = smaller.loose_poses(LooseId::from_raw(own.get() + 1));

        assert_eq!(from_larger_world, None);
        assert_eq!(beyond_issued, None);
    }

    fn listed(world: &PhysicsWorld) -> Vec<(LooseId, LooseState)> {
        world.loose_states().collect()
    }

    /// An entry as raw bits, so equality is bitwise and a NaN cannot hide a difference.
    fn entry_bits(entry: &(LooseId, LooseState)) -> (u64, LooseShape, [u32; 13], bool) {
        let (id, state) = entry;
        (id.get(), state.shape, bits(state), state.asleep)
    }

    fn varied_spawns() -> Vec<LooseSpawn> {
        let shapes = every_shape_with_sphere();
        (0..8)
            .map(|i| LooseSpawn {
                position: Vec3::new(i as f32 * 0.4 - 1.5, 0.3 + i as f32 * 0.2, 0.0),
                linear_velocity: Vec3::new(0.3 * i as f32, 0.0, 0.1),
                ..LooseSpawn::new(shapes[i % shapes.len()].1, 0.2)
            })
            .collect()
    }

    #[test]
    fn spawn_timing_within_a_tick_does_not_change_the_list() {
        use crate::movement::CharacterCommand;
        use crate::world::test_support::test_settings;
        let settings = test_settings();
        let command = CharacterCommand {
            forward: 1.0,
            ..CharacterCommand::default()
        };
        let mut before = world_with_floor(&floor_mesh(0.0));
        let mut after = world_with_floor(&floor_mesh(0.0));
        let far_before = before.add_character(Vec3::new(40.0, 1.2, 40.0));
        let far_after = after.add_character(Vec3::new(40.0, 1.2, 40.0));
        let spawns = varied_spawns();
        for spawn in &spawns {
            before.spawn_loose(spawn);
        }
        before
            .move_character(far_before, &command, &settings, DT)
            .expect("the character exists");
        after
            .move_character(far_after, &command, &settings, DT)
            .expect("the character exists");
        for spawn in &spawns {
            after.spawn_loose(spawn);
        }
        before.step(DT);
        after.step(DT);
        let mut saw_asleep = false;

        for tick in 1..=300 {
            let (a, b) = (listed(&before), listed(&after));
            assert_eq!(a.len(), spawns.len(), "tick {tick}: every object is listed");
            let a_bits: Vec<_> = a.iter().map(entry_bits).collect();
            let b_bits: Vec<_> = b.iter().map(entry_bits).collect();
            assert_eq!(a_bits, b_bits, "tick {tick}: the lists differ");
            saw_asleep |= a.iter().any(|(_, state)| state.asleep);
            before
                .move_character(far_before, &command, &settings, DT)
                .expect("the character exists");
            after
                .move_character(far_after, &command, &settings, DT)
                .expect("the character exists");
            before.step(DT);
            after.step(DT);
        }
        assert!(
            saw_asleep,
            "some object came to rest, so asleep was compared"
        );
    }

    proptest! {
        #[test]
        fn the_list_is_in_id_order_after_removals(
            ops in proptest::collection::vec((any::<bool>(), 0usize..8), 0..30)
        ) {
            let mut world = PhysicsWorld::new();
            let mut live: Vec<LooseId> = Vec::new();

            for (spawn, pick) in ops {
                if spawn || live.is_empty() {
                    live.push(world.spawn_loose(&flat_box_spawn()));
                } else {
                    let id = live.remove(pick % live.len());
                    prop_assert_eq!(world.remove_loose(id), Ok(()));
                }
            }

            let ids: Vec<LooseId> = world.loose_states().map(|(id, _)| id).collect();
            prop_assert!(ids.windows(2).all(|w| w[0] < w[1]), "not ascending: {ids:?}");
            prop_assert_eq!(&ids, &live);
            prop_assert_eq!(world.loose_states().count(), world.loose_count());
        }
    }

    #[test]
    fn the_list_reads_what_each_id_reads() {
        let mut world = world_with_floor(&floor_mesh(0.0));
        let spawns = varied_spawns();
        let ids: Vec<LooseId> = spawns.iter().map(|s| world.spawn_loose(s)).collect();
        run(&mut world, 40);
        world.remove_loose(ids[2]).expect("the object is alive");

        let list = listed(&world);

        assert_eq!(list.len(), spawns.len() - 1);
        for (id, state) in &list {
            assert_eq!(Some(*state), world.loose(*id), "{id:?}");
            assert_eq!(state.shape, spawns[id.get() as usize].shape, "{id:?}");
        }
    }

    #[test]
    fn a_spawn_round_trips_through_postcard() {
        for (name, shape) in every_shape_with_sphere() {
            let spawn = LooseSpawn {
                friction: 0.3,
                restitution: 0.2,
                linear_damping: 0.1,
                angular_damping: 0.4,
                position: Vec3::new(1.0, 2.0, -3.0),
                rotation: Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0).normalize(), 0.7),
                linear_velocity: Vec3::new(0.5, -1.0, 2.0),
                angular_velocity: Vec3::new(0.1, 0.2, 0.3),
                ..LooseSpawn::new(shape, 3.5)
            };

            let bytes = postcard::to_allocvec(&spawn).expect("the spawn serialises");
            let back: LooseSpawn = postcard::from_bytes(&bytes).expect("the spawn reads back");

            assert_eq!(back, spawn, "{name}");
        }
    }

    #[test]
    fn a_restored_list_keeps_the_saved_ids_and_a_new_spawn_follows_it() {
        let shapes = [
            cuboid(FLAT_BOX),
            LooseShape::Sphere { radius: 0.2 },
            CAPSULE,
        ];
        let spawns: Vec<LooseSpawn> = shapes
            .iter()
            .enumerate()
            .map(|(i, &shape)| LooseSpawn {
                position: Vec3::new(i as f32 + 0.5, 1.0 + i as f32, -2.0),
                rotation: Quat::from_axis_angle(
                    Vec3::new(1.0, 2.0, 3.0).normalize(),
                    0.3 + i as f32,
                ),
                linear_velocity: Vec3::new(0.5 * i as f32, -1.0, 2.0),
                angular_velocity: Vec3::new(0.1, 0.2 * i as f32, 0.3),
                ..LooseSpawn::new(shape, 1.0)
            })
            .collect();
        let saved = records(
            12,
            vec![
                record(2, spawns[0]),
                record(5, spawns[1]),
                record(9, spawns[2]),
            ],
        );
        let mut world = PhysicsWorld::new();
        world.restore_loose(&saved).expect("the records restore");

        let before = listed(&world);
        let issued = world.spawn_loose(&valid_spawn());
        let after = listed(&world);

        let ids: Vec<u64> = before.iter().map(|(id, _)| id.get()).collect();
        assert_eq!(ids, vec![2, 5, 9]);
        for ((_, state), spawn) in before.iter().zip(&spawns) {
            assert_eq!(state.shape, spawn.shape);
            assert_eq!(state.position, spawn.position);
            assert_eq!(state.rotation, spawn.rotation);
            assert_eq!(state.linear_velocity, spawn.linear_velocity);
            assert_eq!(state.angular_velocity, spawn.angular_velocity);
            assert!(!state.asleep);
        }
        assert_eq!(issued.get(), 12);
        let ids: Vec<u64> = after.iter().map(|(id, _)| id.get()).collect();
        assert_eq!(ids, vec![2, 5, 9, 12]);
    }

    #[test]
    fn a_removal_neither_shifts_the_list_nor_frees_an_id() {
        let sphere = LooseShape::Sphere { radius: 0.2 };
        let mut world = PhysicsWorld::new();
        let a = world.spawn_loose(&LooseSpawn::new(cuboid(FLAT_BOX), 1.0));
        let b = world.spawn_loose(&LooseSpawn::new(sphere, 1.0));
        world.remove_loose(a).expect("the object is alive");

        let c = world.spawn_loose(&LooseSpawn::new(CAPSULE, 1.0));

        assert_eq!(c.get(), 2);
        let list = listed(&world);
        let ids: Vec<LooseId> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![b, c]);
        assert_eq!(list[0].1.shape, sphere);
        assert_eq!(list[1].1.shape, CAPSULE);
    }
}
