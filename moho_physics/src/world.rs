use glam::Vec3;
use moho_render_api::{WorldMesh, WorldMeshId, WorldMeshRange};
use rapier3d::prelude::*;
use std::collections::{BTreeMap, HashMap};

/// Downward acceleration of a world that never calls [`PhysicsWorld::set_gravity`], m/s².
const DEFAULT_GRAVITY: f32 = 18.0;

mod character;
mod loose;
mod query;
mod slide;
#[cfg(test)]
pub(crate) mod test_support;

use character::Character;
pub use character::{CharacterHandle, CharacterSpawn, CharacterState, HitVolume, UnknownCharacter};
pub use loose::{LooseId, LooseShape, LooseSpawn, LooseState, UnknownLoose};
pub use query::{HitTag, RayHit, RayQuery};

pub struct PhysicsWorld {
    gravity: Vector,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    soft_body_set: SoftBodySet,
    ccd_solver: CCDSolver,
    world_mesh_colliders: HashMap<WorldMeshId, ColliderHandle>,
    characters: HashMap<CharacterHandle, Character>,
    loose: BTreeMap<LooseId, loose::Loose>,
    next_loose: u64,
}

impl std::fmt::Debug for PhysicsWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld")
            .field("characters", &self.characters.len())
            .finish_non_exhaustive()
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        let gravity = Vec3::new(0.0, -DEFAULT_GRAVITY, 0.0);
        let integration_parameters = IntegrationParameters::default();
        let physics_pipeline = PhysicsPipeline::new();
        let island_manager = IslandManager::new();
        let broad_phase = BroadPhaseBvh::new();
        let narrow_phase = NarrowPhase::new();
        let rigid_body_set = RigidBodySet::new();
        let collider_set = ColliderSet::new();
        let impulse_joint_set = ImpulseJointSet::new();
        let multibody_joint_set = MultibodyJointSet::new();
        let soft_body_set = SoftBodySet::new();
        let ccd_solver = CCDSolver::new();

        Self {
            gravity,
            integration_parameters,
            physics_pipeline,
            island_manager,
            broad_phase,
            narrow_phase,
            rigid_body_set,
            collider_set,
            impulse_joint_set,
            multibody_joint_set,
            soft_body_set,
            ccd_solver,
            world_mesh_colliders: HashMap::new(),
            characters: HashMap::new(),
            loose: BTreeMap::new(),
            next_loose: 0,
        }
    }

    /// Step the physics simulation (advances dynamic rigid bodies).
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            &mut self.ccd_solver,
            &(),
            &(),
        );
    }

    /// Set the downward acceleration, m/s², positive down, that pulls rigid bodies and
    /// characters whose `MoveSettings::gravity` is `None`.
    pub fn set_gravity(&mut self, gravity: f32) {
        self.gravity = Vec3::new(0.0, -gravity, 0.0);
    }

    /// The downward acceleration, m/s², positive down.
    pub fn gravity(&self) -> f32 {
        -self.gravity.y
    }

    /// Register `mesh` as the static collider for `id`, replacing any previous one.
    ///
    /// Rays that hit the mesh report `tag`; an untagged collider never stops a ray.
    ///
    /// An empty mesh, or one rapier rejects (warned), leaves `id` without a collider.
    pub fn set_world_mesh(&mut self, id: WorldMeshId, mesh: &WorldMesh, tag: HitTag) {
        self.remove_world_mesh(id);
        if mesh.is_empty() {
            return;
        }
        let points: Vec<Vector> = mesh
            .positions()
            .iter()
            .map(|v| Vec3::new(v[0], v[1], v[2]))
            .collect();
        let tris: Vec<[u32; 3]> = mesh.indices().as_chunks::<3>().0.to_vec();
        match ColliderBuilder::trimesh(points, tris) {
            Ok(builder) => {
                let handle = self.collider_set.insert(
                    builder
                        .friction(0.6)
                        .user_data(query::encode_tag(tag))
                        .build(),
                );
                self.world_mesh_colliders.insert(id, handle);
            }
            Err(error) => {
                tracing::warn!(?id, ?error, "world mesh rejected by physics, no collider");
            }
        }
    }

    /// Remove the collider registered for `id`, if any.
    pub fn remove_world_mesh(&mut self, id: WorldMeshId) {
        if let Some(handle) = self.world_mesh_colliders.remove(&id) {
            self.remove_collider(handle);
        }
    }

    /// Remove the collider of every id in `range`; ids without one are skipped.
    pub fn remove_world_meshes(&mut self, range: WorldMeshRange) {
        for id in range.iter() {
            self.remove_world_mesh(id);
        }
    }

    /// The collider registered for `id`; `None` for an unknown, empty or rejected mesh.
    pub fn world_mesh_collider(&self, id: WorldMeshId) -> Option<ColliderHandle> {
        self.world_mesh_colliders.get(&id).copied()
    }

    /// Remove a collider from the simulation.
    pub fn remove_collider(&mut self, handle: ColliderHandle) {
        self.collider_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.rigid_body_set,
            &mut self.soft_body_set,
            false,
        );
    }

    /// Spawn a dynamic sphere rigid body. Returns the body handle.
    pub fn add_dynamic_sphere(&mut self, position: Vec3, radius: f32) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic().translation(position).build();
        let body_handle = self.rigid_body_set.insert(body);

        let collider = ColliderBuilder::ball(radius)
            .restitution(0.4)
            .friction(0.6)
            .build();
        self.collider_set
            .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        body_handle
    }

    /// Add a dynamic box rigid body at `position` with given half-extents.
    pub fn add_dynamic_cuboid(
        &mut self,
        position: Vec3,
        half_x: f32,
        half_y: f32,
        half_z: f32,
    ) -> RigidBodyHandle {
        let body = RigidBodyBuilder::dynamic().translation(position).build();
        let body_handle = self.rigid_body_set.insert(body);

        let collider = ColliderBuilder::cuboid(half_x, half_y, half_z)
            .restitution(0.3)
            .friction(0.7)
            .build();
        self.collider_set
            .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        body_handle
    }

    /// Get a dynamic rigid body's current translation.
    pub fn body_position(&self, handle: RigidBodyHandle) -> Option<Vec3> {
        let body = self.rigid_body_set.get(handle)?;
        Some(body.position().translation)
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TEST_TAG;
    use super::test_support::{floor_mesh, quad_mesh, test_settings};
    use super::*;
    use crate::movement::{CharacterCommand, MoveSettings};
    use proptest::prelude::*;

    #[test]
    fn test_gravity_drops_rigid_body() {
        let mut world = PhysicsWorld::new();
        let sphere = world.add_dynamic_sphere(Vec3::new(0.0, 10.0, 0.0), 0.5);

        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }

        let pos = world.body_position(sphere).unwrap();
        assert!(
            (0.7..1.2).contains(&pos.y),
            "Sphere should fall about 9m in 1s, y={}",
            pos.y
        );
    }

    #[test]
    fn test_floor_stops_rigid_body() {
        let mut world = PhysicsWorld::new();

        // Build a flat floor at y=0
        let verts = vec![
            [-10.0f32, 0.0, -10.0],
            [10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, 10.0],
        ];
        let idxs: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        world.set_world_mesh(WorldMeshId(0), &quad_mesh(&verts, &idxs), TEST_TAG);

        let sphere = world.add_dynamic_sphere(Vec3::new(0.0, 5.0, 0.0), 0.5);

        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        let pos = world.body_position(sphere).unwrap();
        assert!(
            pos.y > 0.4 && pos.y < 0.7,
            "Sphere should rest on floor at its radius, y={}",
            pos.y
        );
    }

    #[test]
    fn test_removed_floor_no_longer_stops_rigid_body() {
        let mut world = PhysicsWorld::new();
        let verts = vec![
            [-10.0f32, 0.0, -10.0],
            [10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, 10.0],
        ];
        let idxs: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        let floor_id = WorldMeshId(0);
        world.set_world_mesh(floor_id, &quad_mesh(&verts, &idxs), TEST_TAG);
        let floor = world.world_mesh_collider(floor_id).expect("floor collider");
        world.remove_world_mesh(floor_id);

        let sphere = world.add_dynamic_sphere(Vec3::new(0.0, 5.0, 0.0), 0.5);
        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        assert!(world.collider_set.get(floor).is_none());
        let pos = world.body_position(sphere).unwrap();
        assert!(pos.y < -1.0, "Sphere should fall through, y={}", pos.y);
    }

    #[test]
    fn dynamic_cuboid_rests_on_its_half_height() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let body = world.add_dynamic_cuboid(Vec3::new(0.0, 3.0, 0.0), 0.5, 0.25, 0.5);

        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        let y = world.body_position(body).unwrap().y;
        assert!(
            (0.2..0.3).contains(&y),
            "cuboid should rest at half height 0.25, y={y}"
        );
    }

    fn empty_mesh() -> WorldMesh {
        quad_mesh(&[], &[])
    }

    fn fall_for_one_second(world: &mut PhysicsWorld, from_y: f32) -> f32 {
        let ball = world.add_dynamic_sphere(Vec3::new(0.0, from_y, 0.0), 0.5);
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        world.body_position(ball).unwrap().y
    }

    #[test]
    fn body_rests_on_registered_world_mesh() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &floor_mesh(0.0), TEST_TAG);

        let y = fall_for_one_second(&mut world, 3.0);

        assert!((y - 0.5).abs() < 0.2, "should rest on floor at 0.5, y={y}");
    }

    #[test]
    fn replacing_world_mesh_moves_the_floor() {
        let mut world = PhysicsWorld::new();
        let id = WorldMeshId(1);
        world.set_world_mesh(id, &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(id, &floor_mesh(2.0), TEST_TAG);

        let y = fall_for_one_second(&mut world, 5.0);

        assert!((y - 2.5).abs() < 0.2, "should rest on higher floor, y={y}");
        assert!(world.world_mesh_collider(id).is_some());
        assert_eq!(world.collider_set.len(), 1 + 1, "one floor plus the ball");
    }

    #[test]
    fn removed_world_mesh_lets_body_fall() {
        let mut world = PhysicsWorld::new();
        let id = WorldMeshId(1);
        world.set_world_mesh(id, &floor_mesh(0.0), TEST_TAG);
        world.remove_world_mesh(id);

        let y = fall_for_one_second(&mut world, 3.0);

        assert!(y < -3.0, "should fall past the old floor, y={y}");
        assert!(world.world_mesh_collider(id).is_none());
        assert_eq!(world.collider_set.len(), 1, "only the ball remains");
    }

    #[test]
    fn empty_world_mesh_holds_no_collider() {
        let mut world = PhysicsWorld::new();
        let id = WorldMeshId(1);
        world.set_world_mesh(id, &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(id, &empty_mesh(), TEST_TAG);

        assert!(world.world_mesh_collider(id).is_none());
        assert_eq!(world.collider_set.len(), 0);
    }

    #[test]
    fn empty_world_mesh_on_fresh_id_adds_nothing() {
        let mut world = PhysicsWorld::new();

        world.set_world_mesh(WorldMeshId(9), &empty_mesh(), TEST_TAG);

        assert!(world.world_mesh_collider(WorldMeshId(9)).is_none());
        assert_eq!(world.collider_set.len(), 0);
    }

    #[test]
    fn removing_unknown_world_mesh_is_a_no_op() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &floor_mesh(0.0), TEST_TAG);

        world.remove_world_mesh(WorldMeshId(2));

        assert!(world.world_mesh_collider(WorldMeshId(1)).is_some());
        assert_eq!(world.collider_set.len(), 1);
    }

    #[test]
    fn distinct_ids_hold_distinct_colliders() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(2), &floor_mesh(1.0), TEST_TAG);

        let a = world.world_mesh_collider(WorldMeshId(1)).unwrap();
        let b = world.world_mesh_collider(WorldMeshId(2)).unwrap();

        assert_ne!(a, b);
        assert_eq!(world.collider_set.len(), 2);
    }

    #[test]
    fn a_new_world_pulls_at_18() {
        let mut world = PhysicsWorld::new();

        let y = fall_for_one_second(&mut world, 10.0);

        assert_eq!(world.gravity(), 18.0);
        assert!(
            (0.7..1.2).contains(&y),
            "a body falls about 9 m in 1 s at 18, y={y}"
        );
    }

    #[test]
    fn setting_gravity_changes_how_bodies_fall() {
        let mut world = PhysicsWorld::new();
        world.set_gravity(9.0);

        let y = fall_for_one_second(&mut world, 10.0);

        assert_eq!(world.gravity(), 9.0);
        assert!(
            (5.2..5.7).contains(&y),
            "a body falls about 4.5 m in 1 s at 9, y={y}"
        );
    }

    #[test]
    fn a_body_and_a_character_dropped_together_land_on_the_same_tick() {
        const DT: f32 = 1.0 / 60.0;
        // A bottom this close to the floor counts as landed: the character's skin.
        const SKIN: f32 = 0.0115;
        let mut world = PhysicsWorld::new();
        world.set_gravity(20.32);
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        // Bottoms 5 m above the floor: sphere radius 0.5, capsule half-height plus radius 1.15.
        let body = world.add_dynamic_sphere(Vec3::new(2.0, 5.5, 0.0), 0.5);
        let character = world.add_character(Vec3::new(-2.0, 6.15, 0.0));
        let settings = MoveSettings {
            gravity: None,
            ..test_settings()
        };
        let mut character_landed = None;
        let mut body_landed = None;

        for tick in 1..=120 {
            world
                .move_character(character, &CharacterCommand::default(), &settings, DT)
                .expect("character is live");
            world.step(DT);
            let character_state = world.character(character).expect("character is live");
            let body_speed = world.rigid_body_set[body].linvel().y;
            if character_state.grounded {
                character_landed.get_or_insert(tick);
            }
            if world.body_position(body).expect("body is live").y - 0.5 <= SKIN {
                body_landed.get_or_insert(tick);
            }
            if character_landed.is_some() || body_landed.is_some() {
                break;
            }
            assert!(
                (character_state.velocity.y - body_speed).abs() < 1e-4,
                "tick {tick}: character {} vs body {body_speed}",
                character_state.velocity.y
            );
        }

        assert_eq!(character_landed, body_landed);
        // 5 m at 20.32 m/s² takes 0.70 s, 42 ticks; 18 m/s² would take 45.
        assert_eq!(character_landed, Some(42));
    }

    #[test]
    fn remove_world_meshes_removes_exactly_the_range() {
        let mut world = PhysicsWorld::new();
        for id in 0..6 {
            world.set_world_mesh(WorldMeshId(id), &floor_mesh(id as f32), TEST_TAG);
        }
        let range = WorldMeshRange::new(WorldMeshId(2), 3).expect("fits");

        world.remove_world_meshes(range);

        for id in [0, 1, 5] {
            assert!(
                world.world_mesh_collider(WorldMeshId(id)).is_some(),
                "id {id} is outside the range and keeps its collider"
            );
        }
        for id in [2, 3, 4] {
            assert!(
                world.world_mesh_collider(WorldMeshId(id)).is_none(),
                "id {id} is inside the range"
            );
        }
        assert_eq!(world.collider_set.len(), 3);
    }

    #[derive(Clone, Debug)]
    enum Op {
        Set(u64, bool),
        Remove(u64),
        RemoveRange(u64, u32),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u64..4, any::<bool>()).prop_map(|(id, empty)| Op::Set(id, empty)),
            (0u64..4).prop_map(Op::Remove),
            (0u64..5, 0u32..5).prop_map(|(first, len)| Op::RemoveRange(first, len)),
        ]
    }

    proptest! {
        #[test]
        fn colliders_match_live_world_mesh_ids(
            ops in proptest::collection::vec(op_strategy(), 0..40)
        ) {
            let mut world = PhysicsWorld::new();
            let mut live = std::collections::HashSet::new();

            for op in ops {
                match op {
                    Op::Set(id, true) => {
                        world.set_world_mesh(WorldMeshId(id), &empty_mesh(), TEST_TAG);
                        live.remove(&id);
                    }
                    Op::Set(id, false) => {
                        world.set_world_mesh(WorldMeshId(id), &floor_mesh(id as f32), TEST_TAG);
                        live.insert(id);
                    }
                    Op::Remove(id) => {
                        world.remove_world_mesh(WorldMeshId(id));
                        live.remove(&id);
                    }
                    Op::RemoveRange(first, len) => {
                        let range = WorldMeshRange::new(WorldMeshId(first), len).expect("fits");
                        world.remove_world_meshes(range);
                        live.retain(|id| !(first..first + u64::from(len)).contains(id));
                    }
                }
            }

            for id in 0u64..4 {
                prop_assert_eq!(
                    world.world_mesh_collider(WorldMeshId(id)).is_some(),
                    live.contains(&id)
                );
            }
            prop_assert_eq!(world.collider_set.len(), live.len());
        }
    }
}
