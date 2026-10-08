//! Physics controller — owns all physics state for the main application.
//!
//! Centralises `PhysicsWorld`, chunk colliders, test bodies, and jump input
//! so callers don't need to touch `App` fields individually.

use moho_core::voxel::ChunkStore;
use moho_game::actors::ActorId;
use std::collections::HashMap;

pub struct PhysicsController {
    pub world: Option<moho_physics::PhysicsWorld>,
    pub chunk_colliders: HashMap<glam::IVec3, moho_physics::ColliderHandle>,
    pub test_bodies: Vec<(moho_physics::RigidBodyHandle, ActorId)>,
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

    /// Step dynamic rigid bodies and return (actor, new_position) pairs.
    ///
    /// Caller is responsible for updating actor positions from the returned list.
    pub fn step(&mut self, dt: f32) -> Vec<(ActorId, glam::Vec3)> {
        let Some(pw) = self.world.as_mut() else {
            return Vec::new();
        };
        pw.step(dt);
        self.test_bodies
            .iter()
            .filter_map(|(handle, actor)| pw.body_position(*handle).map(|p| (*actor, p)))
            .collect()
    }

    /// Stop tracking the body belonging to `actor` and return its handle, or
    /// `None` if the actor isn't tracked.
    ///
    /// The rigid body stays in the physics world until [`PhysicsController::reset`],
    /// because `moho_physics` has no body-removal API.
    pub fn forget_body(&mut self, actor: ActorId) -> Option<moho_physics::RigidBodyHandle> {
        let index = self.test_bodies.iter().position(|(_, id)| *id == actor)?;
        Some(self.test_bodies.remove(index).0)
    }

    /// Remove the old collider for `chunk_pos` (if any) and register a new one.
    pub fn update_chunk_collider(
        &mut self,
        chunk_pos: glam::IVec3,
        vertices: &[[f32; 3]],
        indices: &[u32],
    ) {
        let Some(pw) = self.world.as_mut() else {
            return;
        };
        if let Some(old) = self.chunk_colliders.remove(&chunk_pos) {
            pw.remove_collider(old);
        }
        let handle = pw.add_terrain_trimesh(vertices, indices);
        self.chunk_colliders.insert(chunk_pos, handle);
    }

    /// Remove the collider for `chunk_pos` (called when a chunk is evicted by streaming).
    pub fn remove_chunk_collider(&mut self, chunk_pos: glam::IVec3) {
        if let (Some(pw), Some(handle)) =
            (self.world.as_mut(), self.chunk_colliders.remove(&chunk_pos))
        {
            pw.remove_collider(handle);
        }
    }

    /// Register colliders for all stored chunks that don't yet have one.
    pub fn sync_colliders(&mut self, chunks: &ChunkStore) {
        let Some(pw) = self.world.as_mut() else {
            return;
        };

        let new_chunks: Vec<(glam::IVec3, Vec<[f32; 3]>, Vec<u32>)> = chunks
            .iter()
            .filter(|c| !self.chunk_colliders.contains_key(&c.chunk_pos()))
            .map(|c| (c.chunk_pos(), c.vertices().to_vec(), c.indices().to_vec()))
            .collect();

        for (pos, vertices, indices) in new_chunks {
            let handle = pw.add_terrain_trimesh(&vertices, &indices);
            self.chunk_colliders.insert(pos, handle);
            tracing::debug!(chunk = ?pos, "Registered terrain collider for chunk");
        }
    }
}

impl Default for PhysicsController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::materials::MaterialType;
    use moho_game::actors::{ActorStore, Cube, Sphere};

    #[test]
    fn step_reports_bodies_by_actor_id() {
        let mut controller = PhysicsController::new();
        let mut actors = ActorStore::new();
        let material = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        let low = actors.spawn_sphere(Sphere::new(glam::Vec3::new(0.0, 10.0, 0.0), 0.5, material));
        let high = actors.spawn_sphere(Sphere::new(glam::Vec3::new(5.0, 30.0, 0.0), 0.5, material));
        let (low_handle, high_handle) = {
            let pw = controller
                .world
                .as_mut()
                .expect("new controller has a world");
            (
                pw.add_dynamic_sphere(glam::Vec3::new(0.0, 10.0, 0.0), 0.5),
                pw.add_dynamic_sphere(glam::Vec3::new(5.0, 30.0, 0.0), 0.5),
            )
        };
        controller.test_bodies.push((low_handle, low));
        controller.test_bodies.push((high_handle, high));

        let reported = controller.step(1.0 / 60.0);

        let pw = controller.world.as_ref().expect("world still present");
        let low_pos = pw.body_position(low_handle).expect("low body exists");
        let high_pos = pw.body_position(high_handle).expect("high body exists");
        assert_eq!(reported.len(), 2);
        assert!(reported.contains(&(low, low_pos)));
        assert!(reported.contains(&(high, high_pos)));
        assert_ne!(low_pos, high_pos);
    }

    fn controller_with_two_tracked_spheres() -> (
        PhysicsController,
        (moho_physics::RigidBodyHandle, moho_game::actors::ActorId),
        (moho_physics::RigidBodyHandle, moho_game::actors::ActorId),
    ) {
        let mut controller = PhysicsController::new();
        let mut actors = ActorStore::new();
        let material = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        let a = actors.spawn_sphere(Sphere::new(glam::Vec3::new(0.0, 10.0, 0.0), 0.5, material));
        let b = actors.spawn_sphere(Sphere::new(glam::Vec3::new(5.0, 30.0, 0.0), 0.5, material));
        let pw = controller
            .world
            .as_mut()
            .expect("new controller has a world");
        let a_handle = pw.add_dynamic_sphere(glam::Vec3::new(0.0, 10.0, 0.0), 0.5);
        let b_handle = pw.add_dynamic_sphere(glam::Vec3::new(5.0, 30.0, 0.0), 0.5);
        controller.test_bodies.push((a_handle, a));
        controller.test_bodies.push((b_handle, b));
        (controller, (a_handle, a), (b_handle, b))
    }

    #[test]
    fn forget_body_stops_step_reporting_that_actor() {
        let (mut controller, (a_handle, a), (b_handle, b)) = controller_with_two_tracked_spheres();

        let forgotten = controller.forget_body(a);

        let reported = controller.step(1.0 / 60.0);
        let b_pos = controller
            .world
            .as_ref()
            .expect("world still present")
            .body_position(b_handle)
            .expect("kept body exists");
        assert_eq!(forgotten, Some(a_handle));
        assert_eq!(reported, vec![(b, b_pos)]);
        assert_eq!(controller.test_bodies, vec![(b_handle, b)]);
    }

    #[test]
    fn forget_body_for_untracked_actor_returns_none() {
        let (mut controller, first, second) = controller_with_two_tracked_spheres();
        let untracked = ActorStore::new().spawn_cube(Cube::new(
            glam::Vec3::ZERO,
            1.0,
            1.0,
            1.0,
            MaterialType::Lambertian {
                albedo: glam::Vec3::ONE,
            },
        ));

        let forgotten = controller.forget_body(untracked);

        assert_eq!(forgotten, None);
        assert_eq!(controller.test_bodies, vec![first, second]);
    }

    fn triangle_chunk(pos: glam::IVec3) -> moho_core::voxel::VoxelChunk {
        let origin = pos.as_vec3();
        moho_core::voxel::VoxelChunk::new(
            pos,
            vec![
                origin.to_array(),
                (origin + glam::Vec3::X).to_array(),
                (origin + glam::Vec3::Z).to_array(),
            ],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![1.0; 3],
            vec![1; 3],
            vec![1.0; 3],
            vec![[1.0, 1.0, 1.0]; 3],
            vec![1.0; 3],
            vec![0, 1, 2],
            0,
        )
    }

    #[test]
    fn sync_colliders_registers_only_chunks_without_a_collider() {
        let mut controller = PhysicsController::new();
        let known = glam::IVec3::new(0, 0, 0);
        let fresh = glam::IVec3::new(1, 0, 0);
        let known_chunk = triangle_chunk(known);
        controller.update_chunk_collider(known, known_chunk.vertices(), known_chunk.indices());
        let known_handle = controller.chunk_colliders[&known];
        let mut chunks = moho_core::voxel::ChunkStore::new();
        chunks.insert(known_chunk);
        chunks.insert(triangle_chunk(fresh));

        controller.sync_colliders(&chunks);

        assert_eq!(controller.chunk_colliders.len(), 2);
        assert_eq!(controller.chunk_colliders[&known], known_handle);
        assert!(controller.chunk_colliders.contains_key(&fresh));
    }
}
