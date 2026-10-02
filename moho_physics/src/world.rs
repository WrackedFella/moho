use glam::Vec3;
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

const GRAVITY: f32 = -18.0;

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
    ccd_solver: CCDSolver,
    character_controller: KinematicCharacterController,
    pub character_body: Option<RigidBodyHandle>,
    pub character_collider: Option<ColliderHandle>,
    pub vertical_velocity: f32,
    pub is_grounded: bool,
    pub noclip: bool,
}

impl std::fmt::Debug for PhysicsWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsWorld")
            .field("noclip", &self.noclip)
            .field("is_grounded", &self.is_grounded)
            .field("vertical_velocity", &self.vertical_velocity)
            .finish_non_exhaustive()
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        let gravity = Vec3::new(0.0, GRAVITY, 0.0);
        let integration_parameters = IntegrationParameters::default();
        let physics_pipeline = PhysicsPipeline::new();
        let island_manager = IslandManager::new();
        let broad_phase = BroadPhaseBvh::new();
        let narrow_phase = NarrowPhase::new();
        let rigid_body_set = RigidBodySet::new();
        let collider_set = ColliderSet::new();
        let impulse_joint_set = ImpulseJointSet::new();
        let multibody_joint_set = MultibodyJointSet::new();
        let ccd_solver = CCDSolver::new();

        let character_controller = KinematicCharacterController {
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.5),
                min_width: CharacterLength::Absolute(0.2),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.1)),
            ..Default::default()
        };

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
            ccd_solver,
            character_controller,
            character_body: None,
            character_collider: None,
            vertical_velocity: 0.0,
            is_grounded: false,
            noclip: false,
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
            &mut self.ccd_solver,
            &(),
            &(),
        );
    }

    /// Build a static terrain trimesh collider from world-space vertices/indices.
    pub fn add_terrain_trimesh(
        &mut self,
        vertices: &[[f32; 3]],
        indices: &[u32],
    ) -> ColliderHandle {
        let points: Vec<Vector> = vertices
            .iter()
            .map(|v| Vec3::new(v[0], v[1], v[2]))
            .collect();

        let tris: Vec<[u32; 3]> = indices
            .chunks(3)
            .filter_map(|c| {
                if c.len() == 3 {
                    Some([c[0], c[1], c[2]])
                } else {
                    None
                }
            })
            .collect();

        if tris.is_empty() || points.is_empty() {
            // Empty chunk — insert a dummy zero-size collider so the handle is valid.
            log::warn!("add_terrain_trimesh: empty mesh, inserting dummy collider");
            let dummy = ColliderBuilder::ball(0.001).build();
            return self.collider_set.insert(dummy);
        }

        let collider = match ColliderBuilder::trimesh(points, tris) {
            Ok(b) => b.friction(0.6).build(),
            Err(e) => {
                log::warn!(
                    "add_terrain_trimesh: trimesh build failed ({:?}), inserting dummy",
                    e
                );
                ColliderBuilder::ball(0.001).build()
            }
        };

        self.collider_set.insert(collider)
    }

    /// Remove a collider from the simulation.
    pub fn remove_collider(&mut self, handle: ColliderHandle) {
        self.collider_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.rigid_body_set,
            false,
        );
    }

    /// Add the kinematic character (capsule) to the world.
    pub fn add_character(&mut self, position: Vec3) -> (RigidBodyHandle, ColliderHandle) {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(position)
            .build();
        let body_handle = self.rigid_body_set.insert(body);

        // Capsule: half-height 0.85, radius 0.3 → total height ~2m
        let collider = ColliderBuilder::capsule_y(0.85, 0.3)
            .friction(0.0) // No friction on character capsule itself
            .build();
        let collider_handle =
            self.collider_set
                .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        self.character_body = Some(body_handle);
        self.character_collider = Some(collider_handle);
        self.vertical_velocity = 0.0;
        self.is_grounded = false;

        log::info!("Character controller spawned at {:?}", position);
        (body_handle, collider_handle)
    }

    /// Move the character by the desired translation (horizontal only).
    /// Gravity is applied internally via `vertical_velocity`.
    /// Returns the new world position after movement.
    pub fn move_character(&mut self, desired_horizontal: Vec3, dt: f32) -> Vec3 {
        let (body_handle, collider_handle) = match (self.character_body, self.character_collider) {
            (Some(b), Some(c)) => (b, c),
            _ => return Vec3::ZERO,
        };

        // Integrate gravity into vertical velocity
        if !self.is_grounded {
            self.vertical_velocity += GRAVITY * dt;
        }

        let desired = Vec3::new(
            desired_horizontal.x,
            self.vertical_velocity * dt,
            desired_horizontal.z,
        );

        let shape = self.collider_set[collider_handle].shape();
        let current_pos = *self.rigid_body_set[body_handle].position();

        // Build a transient QueryPipeline view filtered to exclude the character itself.
        let filter = QueryFilter::default().exclude_collider(collider_handle);
        let queries = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter,
        );

        let movement = self.character_controller.move_shape(
            dt,
            &queries,
            shape,
            &current_pos,
            desired,
            |_| {},
        );

        self.is_grounded = movement.grounded;
        if self.is_grounded && self.vertical_velocity < 0.0 {
            self.vertical_velocity = 0.0;
        }

        // Apply effective movement directly so character_position() reflects it immediately.
        let new_translation = current_pos.translation + movement.translation;
        let new_pose = Pose::from_parts(new_translation, current_pos.rotation);
        // Set both the "next" and "current" position so the value is available immediately.
        self.rigid_body_set[body_handle].set_next_kinematic_position(new_pose);
        self.rigid_body_set[body_handle].set_position(new_pose, false);

        new_translation
    }

    /// Teleport the character to `position` and zero out vertical velocity.
    pub fn set_character_position(&mut self, position: Vec3) {
        if let Some(handle) = self.character_body {
            let current = *self.rigid_body_set[handle].position();
            let new_pose = Pose::from_parts(position, current.rotation);
            self.rigid_body_set[handle].set_next_kinematic_position(new_pose);
            self.rigid_body_set[handle].set_position(new_pose, false);
            self.vertical_velocity = 0.0;
        }
    }

    /// Get the character's current world position.
    pub fn character_position(&self) -> Option<Vec3> {
        let handle = self.character_body?;
        Some(self.rigid_body_set[handle].position().translation)
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
    use super::*;

    #[test]
    fn test_gravity_drops_rigid_body() {
        let mut world = PhysicsWorld::new();
        let sphere = world.add_dynamic_sphere(Vec3::new(0.0, 10.0, 0.0), 0.5);

        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }

        let pos = world.body_position(sphere).unwrap();
        assert!(pos.y < 10.0, "Sphere should have fallen, y={}", pos.y);
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
        world.add_terrain_trimesh(&verts, &idxs);

        let sphere = world.add_dynamic_sphere(Vec3::new(0.0, 5.0, 0.0), 0.5);

        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        let pos = world.body_position(sphere).unwrap();
        // Should rest near y=0.5 (radius)
        assert!(pos.y < 2.0, "Sphere should rest on floor, y={}", pos.y);
    }

    #[test]
    fn test_character_controller_falls_to_floor() {
        let mut world = PhysicsWorld::new();

        // Build a flat floor at y=0
        let verts = vec![
            [-10.0f32, 0.0, -10.0],
            [10.0, 0.0, -10.0],
            [10.0, 0.0, 10.0],
            [-10.0, 0.0, 10.0],
        ];
        let idxs: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        world.add_terrain_trimesh(&verts, &idxs);
        world.add_character(Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..120 {
            world.move_character(Vec3::ZERO, 1.0 / 60.0);
        }

        let pos = world.character_position().unwrap();
        assert!(
            pos.y < 3.0,
            "Character should have fallen near floor, y={}",
            pos.y
        );
    }

    #[test]
    fn test_character_horizontal_movement() {
        let mut world = PhysicsWorld::new();

        // Build a flat floor at y=0
        let verts = vec![
            [-20.0f32, 0.0, -20.0],
            [20.0, 0.0, -20.0],
            [20.0, 0.0, 20.0],
            [-20.0, 0.0, 20.0],
        ];
        let idxs: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
        world.add_terrain_trimesh(&verts, &idxs);
        world.add_character(Vec3::new(0.0, 1.0, 0.0));

        // First, settle on the ground
        for _ in 0..60 {
            world.move_character(Vec3::ZERO, 1.0 / 60.0);
        }

        let start = world.character_position().unwrap();

        // Move in +X direction
        for _ in 0..30 {
            world.move_character(Vec3::new(4.0 * (1.0 / 60.0), 0.0, 0.0), 1.0 / 60.0);
        }

        let end = world.character_position().unwrap();
        assert!(
            end.x > start.x + 0.5,
            "Character should have moved in +X, start={}, end={}",
            start.x,
            end.x
        );
    }
}
