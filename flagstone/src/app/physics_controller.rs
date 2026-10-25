//! Physics controller — owns all physics state for the main application.
//!
//! Centralises `PhysicsWorld`, test bodies, and jump input
//! so callers don't need to touch `App` fields individually.

use flagstone_game::actors::ActorId;

/// Downward acceleration of the strategy world, m/s²; its player's move settings read it.
const WORLD_GRAVITY: f32 = 18.0;

fn new_world() -> moho_physics::PhysicsWorld {
    let mut world = moho_physics::PhysicsWorld::new();
    world.set_gravity(WORLD_GRAVITY);
    world
}

pub struct PhysicsController {
    pub world: Option<moho_physics::PhysicsWorld>,
    pub test_bodies: Vec<(moho_physics::RigidBodyHandle, ActorId)>,
    pub jump_pressed: bool,
    pub player: Option<moho_physics::CharacterHandle>,
    pub noclip: bool,
    /// The player's state before the latest tick, for blending the view.
    previous: Option<moho_physics::CharacterState>,
    /// A teleport or look is queued and not yet applied: the view has no states to blend until
    /// the next tick applies it, and that tick records its result as `previous` too, so the view
    /// does not blend across the change.
    snap_view: bool,
}

impl PhysicsController {
    pub fn new() -> Self {
        Self {
            world: Some(new_world()),
            test_bodies: Vec::new(),
            jump_pressed: false,
            player: None,
            noclip: false,
            previous: None,
            snap_view: false,
        }
    }

    /// Clear and recreate the physics world, removing all colliders and bodies.
    pub fn reset(&mut self) {
        self.world = Some(new_world());
        self.test_bodies.clear();
        self.player = None;
        self.previous = None;
        self.snap_view = false;
        self.noclip = false;
    }

    /// Place the player character at `spawn`, replacing any previous one.
    pub fn spawn_player(&mut self, spawn: impl Into<moho_physics::CharacterSpawn>) {
        let Some(pw) = self.world.as_mut() else {
            return;
        };
        if let Some(previous) = self.player.take() {
            pw.remove_character(previous)
                .expect("the player handle belongs to the current world");
        }
        let handle = pw.add_character(spawn);
        self.player = Some(handle);
        self.snap_view = false;
        self.previous = pw.character(handle);
    }

    /// The player's state before and after the latest tick, if there is a player and no
    /// teleport or look is waiting for the next tick to apply.
    pub fn player_states(
        &self,
    ) -> Option<(moho_physics::CharacterState, moho_physics::CharacterState)> {
        if self.snap_view {
            return None;
        }
        let current = self.world.as_ref()?.character(self.player?)?;
        Some((self.previous?, current))
    }

    /// Whether a kinematic character controller is active (and not in noclip).
    pub fn is_kcc_active(&self) -> bool {
        !self.noclip && self.world.is_some() && self.player.is_some()
    }

    /// Move the player for one tick of `command` under `settings`.
    ///
    /// Returns the player's state afterwards, or `None` if the physics world
    /// is not initialised or there is no player.
    pub fn move_character(
        &mut self,
        command: &moho_physics::CharacterCommand,
        settings: &moho_physics::MoveSettings,
        dt: f32,
    ) -> Option<moho_physics::CharacterState> {
        let handle = self.player?;
        let pw = self.world.as_mut()?;
        self.previous = pw.character(handle);
        pw.move_character(handle, command, settings, dt).ok()?;
        let moved = pw.character(handle);
        if std::mem::take(&mut self.snap_view) {
            self.previous = moved;
        }
        moved
    }

    /// Queue facing the player character `yaw`/`pitch`, so the next tick turns from the view on
    /// screen.
    pub fn set_player_look(&mut self, yaw: f32, pitch: f32) {
        if let (Some(pw), Some(handle)) = (self.world.as_mut(), self.player) {
            pw.queue_effect(
                handle,
                moho_physics::CharacterEffect::SetLook { yaw, pitch },
            )
            .expect("the player handle belongs to the current world");
            self.snap_view = true;
        }
    }

    /// Queue a teleport of the player (e.g. respawn after falling off the map), applied by the
    /// next tick, which the view does not blend across.
    pub fn teleport_character(&mut self, pos: glam::Vec3) {
        if let (Some(pw), Some(handle)) = (self.world.as_mut(), self.player) {
            pw.queue_effect(
                handle,
                moho_physics::CharacterEffect::Teleport {
                    position: pos,
                    keep_velocity: false,
                },
            )
            .expect("the player handle belongs to the current world");
            self.snap_view = true;
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
    use flagstone_game::actors::{ActorStore, Cube, Sphere};
    use flagstone_voxel::MaterialType;
    use moho_physics::HitTag;

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
        (
            moho_physics::RigidBodyHandle,
            flagstone_game::actors::ActorId,
        ),
        (
            moho_physics::RigidBodyHandle,
            flagstone_game::actors::ActorId,
        ),
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
            app.entities
                .chunks
                .iter()
                .filter(|chunk| !chunk.is_empty())
                .count(),
            "one collider per non-empty chunk, character aside"
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

    const DT: f32 = 1.0 / 60.0;

    fn big_floor_mesh() -> moho_render_api::WorldMesh {
        moho_render_api::WorldMesh::new(
            vec![
                [-20.0, 0.0, -20.0],
                [20.0, 0.0, -20.0],
                [-20.0, 0.0, 20.0],
                [20.0, 0.0, 20.0],
            ],
            vec![[0.0, 1.0, 0.0]; 4],
            vec![1.0; 4],
            vec![[0.0; 3]; 4],
            vec![1.0; 4],
            vec![0; 4],
            vec![0, 2, 1, 1, 2, 3],
        )
        .expect("valid floor mesh")
    }

    fn strategy_settings() -> moho_physics::MoveSettings {
        moho_physics::MoveSettings {
            max_speed: 4.0,
            ground_accel: 360.0,
            friction: 60.0,
            stop_speed: 1.0,
            air_accel: 360.0,
            air_speed_cap: 6.0,
            gravity: None,
            jump_speed: 8.0,
            max_velocity: 90.0,
            autohop: true,
            crouch_ticks: 0,
            prone_ticks: 0,
            lean_ticks: 0,
        }
    }

    fn idle() -> moho_physics::CharacterCommand {
        moho_physics::CharacterCommand::default()
    }

    fn jumping() -> moho_physics::CharacterCommand {
        moho_physics::CharacterCommand {
            jump: true,
            ..idle()
        }
    }

    fn grounded_player_on_floor() -> PhysicsController {
        let mut controller = PhysicsController::new();
        controller
            .world
            .as_mut()
            .expect("new controller has a world")
            .set_world_mesh(
                moho_render_api::WorldMeshId(1),
                &big_floor_mesh(),
                HitTag(0),
            );
        controller.spawn_player(glam::Vec3::new(0.0, 1.5, 0.0));
        for _ in 0..300 {
            controller.move_character(&idle(), &strategy_settings(), DT);
            controller.step(DT);
        }
        controller
    }

    fn player_state(controller: &PhysicsController) -> moho_physics::CharacterState {
        controller
            .world
            .as_ref()
            .expect("physics world")
            .character(controller.player.expect("player"))
            .expect("player resolves")
    }

    #[test]
    fn move_character_without_a_jump_command_on_the_ground_does_not_rise() {
        let mut controller = grounded_player_on_floor();
        assert!(player_state(&controller).grounded, "settled on the floor");

        let state = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");

        assert!(state.velocity.y <= 0.0, "{state:?}");
    }

    #[test]
    fn jump_command_on_the_ground_launches_upward() {
        let mut controller = grounded_player_on_floor();
        let before = player_state(&controller).position.y;

        let returned = controller
            .move_character(&jumping(), &strategy_settings(), DT)
            .expect("player is live");

        let state = player_state(&controller);
        assert_eq!(returned, state, "returns the state it left the player in");
        assert!(state.position.y > before, "{state:?}");
        assert!(state.velocity.y > 0.0, "{state:?}");
    }

    #[test]
    fn jump_command_in_the_air_does_not_launch() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));

        controller.move_character(&jumping(), &strategy_settings(), DT);

        let state = player_state(&controller);
        assert!(!state.grounded);
        assert!(state.velocity.y < 0.0, "{state:?}");
    }

    #[test]
    fn set_player_look_applies_on_the_next_tick() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));

        controller.set_player_look(0.7, -0.3);

        let queued = player_state(&controller);
        assert_eq!((queued.yaw, queued.pitch), (0.0, 0.0), "not yet applied");
        let moved = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        assert_eq!((moved.yaw, moved.pitch), (0.7, -0.3));
        assert_eq!(player_state(&controller), moved);
    }

    #[test]
    fn set_player_look_without_a_player_does_nothing() {
        let mut controller = PhysicsController::new();

        controller.set_player_look(0.7, -0.3);

        assert_eq!(controller.player, None);
    }

    #[test]
    fn move_character_without_a_player_returns_none() {
        let mut controller = PhysicsController::new();

        let moved = controller.move_character(&idle(), &strategy_settings(), DT);

        assert_eq!(moved, None);
    }

    #[test]
    fn teleport_moves_the_player_on_the_next_tick_and_zeroes_velocity() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }
        assert!(player_state(&controller).velocity.y < 0.0);
        let before = player_state(&controller);
        let target = glam::Vec3::new(3.0, 20.0, -4.0);

        controller.teleport_character(target);

        assert_eq!(player_state(&controller), before, "not yet applied");
        let state = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        assert!(state.position.distance(target) < 0.01, "{state:?}");
        assert!(
            (state.velocity - glam::Vec3::new(0.0, -18.0 * DT, 0.0)).length() < 1e-4,
            "velocity restarted from rest: {state:?}"
        );
    }

    #[test]
    fn a_kill_plane_teleport_followed_by_two_ticks_blends_only_the_second() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }

        controller.teleport_character(glam::Vec3::new(3.0, 20.0, -4.0));
        let first = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        let (previous, current) = controller.player_states().expect("player is live");
        assert_eq!((previous, current), (first, first), "snapped on arrival");
        let second = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");

        let (previous, current) = controller.player_states().expect("player is live");
        assert_eq!(previous, first);
        assert_eq!(current, second);
        assert_ne!(first, second);
    }

    #[test]
    fn player_states_are_withheld_until_a_queued_teleport_applies() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }
        assert!(controller.player_states().is_some());

        controller.teleport_character(glam::Vec3::new(3.0, 20.0, -4.0));

        assert_eq!(controller.player_states(), None);
        let moved = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        assert_eq!(controller.player_states(), Some((moved, moved)));
    }

    #[test]
    fn player_states_are_withheld_until_a_queued_look_applies() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }
        assert!(controller.player_states().is_some());

        controller.set_player_look(0.7, -0.3);

        assert_eq!(controller.player_states(), None);
        let moved = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        assert_eq!((moved.yaw, moved.pitch), (0.7, -0.3));
        assert_eq!(controller.player_states(), Some((moved, moved)));
    }

    #[test]
    fn teleport_is_not_blended_by_the_view() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }

        controller.teleport_character(glam::Vec3::new(3.0, 20.0, -4.0));
        let teleported = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");

        let (previous, current) = controller.player_states().expect("player is live");
        assert_eq!(previous, teleported, "the view starts from the arrival");
        assert_eq!(current, teleported);
        let after = controller
            .move_character(&idle(), &strategy_settings(), DT)
            .expect("player is live");
        let (previous, current) = controller.player_states().expect("player is live");
        assert_eq!(previous, teleported, "the next tick blends again");
        assert_eq!(current, after);
        assert_ne!(previous, current);
    }

    #[test]
    fn respawn_resets_previous_state() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }

        controller.spawn_player(glam::Vec3::new(5.0, 60.0, 5.0));

        assert_eq!(controller.previous, Some(player_state(&controller)));
    }

    #[test]
    fn reset_clears_previous_state() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }

        controller.reset();

        assert_eq!(controller.previous, None);
    }

    #[test]
    fn move_character_keeps_the_state_before_the_tick() {
        let mut controller = PhysicsController::new();
        controller.spawn_player(glam::Vec3::new(0.0, 50.0, 0.0));
        for _ in 0..10 {
            controller.move_character(&idle(), &strategy_settings(), DT);
        }
        let before_tick = player_state(&controller);

        controller.move_character(&idle(), &strategy_settings(), DT);

        assert_eq!(controller.previous, Some(before_tick));
        assert_ne!(controller.previous, Some(player_state(&controller)));
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
        pw.set_world_mesh(unowned, &stale_floor_mesh(), HitTag(0));
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
