//! Physics controller — owns all physics state for the main application.
//!
//! Centralises `PhysicsWorld`, test bodies, and jump input
//! so callers don't need to touch `App` fields individually.

use moho_game::actors::ActorId;

const JUMP_VELOCITY: f32 = 8.0;

pub struct PhysicsController {
    pub world: Option<moho_physics::PhysicsWorld>,
    pub test_bodies: Vec<(moho_physics::RigidBodyHandle, ActorId)>,
    pub jump_pressed: bool,
    pub player: Option<moho_physics::CharacterHandle>,
    pub noclip: bool,
}

impl PhysicsController {
    pub fn new() -> Self {
        Self {
            world: Some(moho_physics::PhysicsWorld::new()),
            test_bodies: Vec::new(),
            jump_pressed: false,
            player: None,
            noclip: false,
        }
    }

    /// Clear and recreate the physics world, removing all colliders and bodies.
    pub fn reset(&mut self) {
        self.world = Some(moho_physics::PhysicsWorld::new());
        self.test_bodies.clear();
        self.player = None;
        self.noclip = false;
    }

    /// Place the player character at `position`, replacing any previous one.
    pub fn spawn_player(&mut self, position: glam::Vec3) {
        let Some(pw) = self.world.as_mut() else {
            return;
        };
        if let Some(previous) = self.player.take() {
            // Already gone after a world reset; nothing to remove then.
            let _ = pw.remove_character(previous);
        }
        self.player = Some(pw.add_character(position));
    }

    /// Whether a kinematic character controller is active (and not in noclip).
    pub fn is_kcc_active(&self) -> bool {
        !self.noclip && self.world.is_some() && self.player.is_some()
    }

    /// Move the player for one frame, applying jump if grounded.
    ///
    /// Returns the new world-space position, or `None` if the physics world
    /// is not initialised or there is no player.
    pub fn move_character(&mut self, horizontal: glam::Vec3, dt: f32) -> Option<glam::Vec3> {
        let handle = self.player?;
        let pw = self.world.as_mut()?;
        if self.jump_pressed && pw.character(handle).is_some_and(|c| c.grounded) {
            pw.set_vertical_velocity(handle, JUMP_VELOCITY).ok()?;
        }
        pw.move_character(handle, horizontal, dt).ok()
    }

    /// Teleport the player (e.g. respawn after falling off the map).
    pub fn teleport_character(&mut self, pos: glam::Vec3) {
        if let (Some(pw), Some(handle)) = (self.world.as_mut(), self.player) {
            // An unknown handle means the player is gone; nothing to teleport.
            let _ = pw.set_character_position(handle, pos);
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
}

impl Default for PhysicsController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_game::actors::{ActorStore, Cube, Sphere};
    use moho_voxel::MaterialType;

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

    #[test]
    fn reset_then_load_holds_one_collider_per_chunk() {
        use crate::app::world_geometry::{
            chunk_mesh_id,
            tests::{chunk_at, terrain_collider_count},
        };
        let positions = [
            glam::IVec3::new(7, 0, 7),
            glam::IVec3::new(8, 0, 7),
            glam::IVec3::new(7, 1, 9),
        ];
        let temp = tempfile::tempdir().expect("temp dir");
        let mut saved = crate::App::headless();
        for pos in positions {
            saved.entities.chunks.insert(chunk_at(pos));
        }
        crate::app::autosave::auto_save_on_shutdown(
            &mut saved,
            &moho_app::GameClock::default(),
            temp.path(),
            &[],
        )
        .expect("autosave");
        let mut app = crate::App::headless();

        crate::app::scene_loader::load_scene(
            &mut app,
            &mut moho_app::GameClock::default(),
            &temp.path().join("scene.bin"),
        )
        .expect("load");

        let pw = app.physics.world.as_ref().expect("physics world");
        for pos in positions {
            assert!(
                pw.world_mesh_collider(chunk_mesh_id(pos)).is_some(),
                "chunk {pos} has a collider after load"
            );
        }
        assert_eq!(
            terrain_collider_count(&app),
            positions.len(),
            "one collider per chunk, character aside"
        );
    }

    fn stale_floor_mesh() -> moho_render_api::WorldMesh {
        moho_render_api::WorldMesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![1.0; 3],
            vec![[0.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            vec![0, 2, 1],
        )
        .expect("valid triangle mesh")
    }

    #[test]
    fn spawning_the_player_twice_keeps_one_character() {
        let mut controller = PhysicsController::new();

        controller.spawn_player(glam::Vec3::new(0.0, 10.0, 0.0));
        let first = controller.player.expect("first player");
        controller.spawn_player(glam::Vec3::new(5.0, 10.0, 0.0));

        let pw = controller.world.as_ref().expect("physics world");
        let second = controller.player.expect("second player");
        assert_ne!(first, second);
        assert!(pw.character(first).is_none(), "first character is gone");
        assert!(pw.character(second).is_some());
        assert_eq!(pw.rigid_body_set.len(), 1, "one character body");
        assert_eq!(pw.collider_set.len(), 1, "one character collider");
    }

    #[test]
    fn reset_clears_the_player_and_noclip() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 10.0, 0.0));
        controller.noclip = true;

        controller.reset();

        assert_eq!(controller.player, None);
        assert!(!controller.noclip);
        assert!(!controller.is_kcc_active());
    }

    #[test]
    fn noclip_turns_the_kcc_off() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 10.0, 0.0));
        assert!(controller.is_kcc_active());

        controller.noclip = true;

        assert!(!controller.is_kcc_active());
    }

    #[test]
    fn load_replaces_the_previous_physics_world() {
        use crate::app::world_geometry::tests::{chunk_at, terrain_collider_count};
        let positions = [glam::IVec3::new(7, 0, 7), glam::IVec3::new(8, 0, 7)];
        let temp = tempfile::tempdir().expect("temp dir");
        let mut saved = crate::App::headless();
        for pos in positions {
            saved.entities.chunks.insert(chunk_at(pos));
        }
        crate::app::autosave::auto_save_on_shutdown(
            &mut saved,
            &moho_app::GameClock::default(),
            temp.path(),
            &[],
        )
        .expect("autosave");
        let mut app = crate::App::headless();
        let mut actors = ActorStore::new();
        let material = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        let stale_actor =
            actors.spawn_sphere(Sphere::new(glam::Vec3::new(0.0, 9.0, 0.0), 0.5, material));
        let pw = app.physics.world.as_mut().expect("physics world");
        let stale_body = pw.add_dynamic_sphere(glam::Vec3::new(0.0, 9.0, 0.0), 0.5);
        let unowned = moho_render_api::WorldMeshId(u64::MAX);
        pw.set_world_mesh(unowned, &stale_floor_mesh());
        app.physics.spawn_player(glam::Vec3::new(50.0, 50.0, 50.0));
        app.physics.test_bodies.push((stale_body, stale_actor));

        crate::app::scene_loader::load_scene(
            &mut app,
            &mut moho_app::GameClock::default(),
            &temp.path().join("scene.bin"),
        )
        .expect("load");

        let pw = app.physics.world.as_ref().expect("physics world");
        let with_geometry = app
            .entities
            .chunks
            .iter()
            .filter(|c| c.has_geometry())
            .count();
        assert!(with_geometry > 0, "the save holds chunks with geometry");
        assert!(app.physics.test_bodies.is_empty(), "no stale test bodies");
        let player = app.physics.player.expect("a new player is placed");
        assert!(pw.character(player).is_some(), "the player handle resolves");
        assert!(pw.world_mesh_collider(unowned).is_none(), "stale mesh gone");
        assert_eq!(terrain_collider_count(&app), with_geometry);
        assert_eq!(pw.collider_set.len(), with_geometry + 1);
        assert_eq!(pw.rigid_body_set.len(), 1, "only the character body");
    }
}
