//! Physics controller — owns all physics state for the main application.
//!
//! Centralises `PhysicsWorld`, chunk colliders, test bodies, and jump input
//! so callers don't need to touch `App` fields individually.

use std::collections::HashMap;

pub struct PhysicsController {
    pub world: Option<moho_physics::PhysicsWorld>,
    pub chunk_colliders: HashMap<glam::IVec3, moho_physics::ColliderHandle>,
    pub test_bodies: Vec<(moho_physics::RigidBodyHandle, legion::Entity)>,
    pub jump_pressed: bool,
}

impl PhysicsController {
    pub fn new() -> Self {
        Self {
            world: Some(moho_physics::PhysicsWorld::new()),
            chunk_colliders: HashMap::new(),
            test_bodies: Vec::new(),
            jump_pressed: false,
        }
    }

    /// Clear and recreate the physics world, removing all colliders and bodies.
    pub fn reset(&mut self) {
        self.world = Some(moho_physics::PhysicsWorld::new());
        self.chunk_colliders.clear();
        self.test_bodies.clear();
    }

    /// Whether a kinematic character controller is active (and not in noclip).
    pub fn is_kcc_active(&self) -> bool {
        self.world
            .as_ref()
            .is_some_and(|pw| !pw.noclip && pw.character_body.is_some())
    }

    /// Move character for one frame, applying jump if grounded.
    ///
    /// Returns the new world-space position, or `None` if the physics world
    /// is not initialised.
    pub fn move_character(&mut self, horizontal: glam::Vec3, dt: f32) -> Option<glam::Vec3> {
        const JUMP_VELOCITY: f32 = 8.0;
        let pw = self.world.as_mut()?;
        if self.jump_pressed && pw.is_grounded {
            pw.vertical_velocity = JUMP_VELOCITY;
        }
        Some(pw.move_character(horizontal, dt))
    }

    /// Teleport the character (e.g. respawn after falling off the map).
    pub fn teleport_character(&mut self, pos: glam::Vec3) {
        if let Some(pw) = &mut self.world {
            pw.set_character_position(pos);
        }
    }

    /// Step dynamic rigid bodies and return (entity, new_position) pairs.
    ///
    /// Caller is responsible for updating ECS components from the returned list.
    pub fn step(&mut self, dt: f32) -> Vec<(legion::Entity, glam::Vec3)> {
        let pw = match self.world.as_mut() {
            Some(pw) => pw,
            None => return Vec::new(),
        };
        pw.step(dt);
        self.test_bodies
            .iter()
            .filter_map(|(handle, entity)| pw.body_position(*handle).map(|p| (*entity, p)))
            .collect()
    }

    /// Remove the old collider for `chunk_pos` (if any) and register a new one.
    pub fn update_chunk_collider(
        &mut self,
        chunk_pos: glam::IVec3,
        vertices: &[[f32; 3]],
        indices: &[u32],
    ) {
        let pw = match self.world.as_mut() {
            Some(pw) => pw,
            None => return,
        };
        if let Some(old) = self.chunk_colliders.remove(&chunk_pos) {
            pw.remove_collider(old);
        }
        let handle = pw.add_terrain_trimesh(vertices, indices);
        self.chunk_colliders.insert(chunk_pos, handle);
    }

    /// Register colliders for all ECS chunks that don't yet have one.
    pub fn sync_colliders_from_ecs(&mut self, ecs_world: &legion::World) {
        use legion::IntoQuery;
        use moho_core::voxel::VoxelChunk;

        let pw = match self.world.as_mut() {
            Some(pw) => pw,
            None => return,
        };

        let mut query = <&VoxelChunk>::query();
        let new_chunks: Vec<(glam::IVec3, Vec<[f32; 3]>, Vec<u32>)> = query
            .iter(ecs_world)
            .filter(|c| !self.chunk_colliders.contains_key(&c.chunk_pos()))
            .map(|c| (c.chunk_pos(), c.vertices().to_vec(), c.indices().to_vec()))
            .collect();

        for (pos, vertices, indices) in new_chunks {
            let handle = pw.add_terrain_trimesh(&vertices, &indices);
            self.chunk_colliders.insert(pos, handle);
            log::debug!("Registered terrain collider for chunk {:?}", pos);
        }
    }
}

impl Default for PhysicsController {
    fn default() -> Self {
        Self::new()
    }
}
