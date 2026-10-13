use super::PhysicsWorld;
use crate::movement::{CharacterCommand, Impact, MoveSettings};
use glam::Vec3;
use rapier3d::prelude::{ColliderBuilder, ColliderHandle, Pose, RigidBodyBuilder, RigidBodyHandle};

/// Opaque identity of one character in a [`PhysicsWorld`].
///
/// Only meaningful to the world that issued it: another world may resolve it to
/// one of its own characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CharacterHandle(RigidBodyHandle);

/// Snapshot of one character's simulation state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
}

/// The handle names a character that was removed.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("unknown character")]
pub struct UnknownCharacter;

/// Per-character state; the capsule's body handle is the map key.
pub(super) struct Character {
    collider: ColliderHandle,
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    grounded: bool,
}

impl PhysicsWorld {
    /// Add a kinematic character (capsule) to the world.
    pub fn add_character(&mut self, position: Vec3) -> CharacterHandle {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(position)
            .build();
        let body_handle = self.rigid_body_set.insert(body);

        // Capsule: half-height 0.85, radius 0.3 -> total height ~2m
        let collider = ColliderBuilder::capsule_y(0.85, 0.3)
            .friction(0.0) // No friction on character capsule itself
            .build();
        let collider =
            self.collider_set
                .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        let handle = CharacterHandle(body_handle);
        self.characters.insert(
            handle,
            Character {
                collider,
                velocity: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                grounded: false,
            },
        );

        tracing::info!(pos = ?position, "Character controller spawned");
        handle
    }

    /// Remove the character's body and capsule.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was already removed.
    pub fn remove_character(&mut self, handle: CharacterHandle) -> Result<(), UnknownCharacter> {
        self.characters.remove(&handle).ok_or(UnknownCharacter)?;
        self.rigid_body_set.remove(
            handle.0,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            true,
        );
        Ok(())
    }

    /// Run one tick of movement for the character and return the impacts it took.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn move_character(
        &mut self,
        handle: CharacterHandle,
        command: &CharacterCommand,
        settings: &MoveSettings,
        dt: f32,
    ) -> Result<Vec<Impact>, UnknownCharacter> {
        let character = self.characters.get(&handle).ok_or(UnknownCharacter)?;
        let _ = (
            command,
            settings,
            dt,
            character.collider,
            &self.character_controller,
        );
        todo!()
    }

    /// Teleport the character to `position` and zero its velocity.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn set_character_position(
        &mut self,
        handle: CharacterHandle,
        position: Vec3,
    ) -> Result<(), UnknownCharacter> {
        let character = self.characters.get_mut(&handle).ok_or(UnknownCharacter)?;
        let body = &mut self.rigid_body_set[handle.0];
        let new_pose = Pose::from_parts(position, body.position().rotation);
        body.set_next_kinematic_position(new_pose);
        body.set_position(new_pose, false);
        character.velocity = Vec3::ZERO;
        Ok(())
    }

    /// Add `velocity` to the character's current velocity (knockback).
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn add_character_velocity(
        &mut self,
        handle: CharacterHandle,
        velocity: Vec3,
    ) -> Result<(), UnknownCharacter> {
        let _ = velocity;
        self.characters.get(&handle).ok_or(UnknownCharacter)?;
        todo!()
    }

    /// Set the character's facing without moving it.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn set_character_look(
        &mut self,
        handle: CharacterHandle,
        yaw: f32,
        pitch: f32,
    ) -> Result<(), UnknownCharacter> {
        let _ = (yaw, pitch);
        self.characters.get(&handle).ok_or(UnknownCharacter)?;
        todo!()
    }

    /// The character's current state, or `None` for an unknown handle.
    pub fn character(&self, handle: CharacterHandle) -> Option<CharacterState> {
        let character = self.characters.get(&handle)?;
        Some(CharacterState {
            position: self.rigid_body_set[handle.0].position().translation,
            velocity: character.velocity,
            yaw: character.yaw,
            pitch: character.pitch,
            grounded: character.grounded,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{floor_mesh, step_box_mesh, test_settings, wall_mesh};
    use super::*;
    use moho_render_api::WorldMeshId;

    const DT: f32 = 1.0 / 60.0;

    fn state(world: &PhysicsWorld, handle: CharacterHandle) -> CharacterState {
        world.character(handle).expect("character is live")
    }

    fn idle() -> CharacterCommand {
        CharacterCommand::default()
    }

    fn frictionless() -> MoveSettings {
        MoveSettings {
            friction: 0.0,
            ..test_settings()
        }
    }

    fn move_idle(world: &mut PhysicsWorld, handle: CharacterHandle) -> Vec<Impact> {
        world
            .move_character(handle, &idle(), &test_settings(), DT)
            .expect("character is live")
    }

    fn settle_on_floor(world: &mut PhysicsWorld, handles: &[CharacterHandle]) {
        for _ in 0..60 {
            for &handle in handles {
                move_idle(world, handle);
            }
            world.step(DT);
        }
    }

    /// Floor at y = 0 and a character standing on it at the origin.
    fn standing_on_floor() -> (PhysicsWorld, CharacterHandle) {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[h]);
        (world, h)
    }

    /// Idles for `ticks`, collecting every impact with a speed above numeric noise.
    fn run(
        world: &mut PhysicsWorld,
        handle: CharacterHandle,
        settings: &MoveSettings,
        ticks: usize,
    ) -> Vec<Impact> {
        let mut impacts = Vec::new();
        for _ in 0..ticks {
            let tick = world
                .move_character(handle, &idle(), settings, DT)
                .expect("character is live");
            impacts.extend(tick.into_iter().filter(|i| i.speed > 1e-3));
            world.step(DT);
        }
        impacts
    }

    #[test]
    fn move_character_in_open_air_reports_no_impacts_and_falls() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        let moved = world.move_character(h, &idle(), &test_settings(), DT);

        assert_eq!(moved, Ok(Vec::new()));
        assert!(state(&world, h).position.y < 10.0);
    }

    #[test]
    fn test_character_controller_falls_to_floor() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..120 {
            move_idle(&mut world, h);
            world.step(DT);
        }

        let pos = state(&world, h).position;
        assert!(
            (1.1..1.3).contains(&pos.y),
            "Capsule centre should rest at half-height plus radius, y={}",
            pos.y
        );
        assert!(state(&world, h).grounded);
    }

    #[test]
    fn set_character_position_teleports_and_stops_falling() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        for _ in 0..10 {
            move_idle(&mut world, h);
        }
        let speed = state(&world, h).velocity.y;
        assert!(
            (speed + 3.0).abs() < 1e-4,
            "10 airborne frames at -18 should give -3.0, v={speed}"
        );

        let target = Vec3::new(3.0, 20.0, -4.0);
        let teleported = world.set_character_position(h, target);

        assert_eq!(teleported, Ok(()));
        assert_eq!(state(&world, h).position, target);
        assert_eq!(state(&world, h).velocity.y, 0.0);
    }

    #[test]
    fn teleport_zeroes_velocity() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world
            .add_character_velocity(h, Vec3::new(3.0, 4.0, 5.0))
            .unwrap();

        world
            .set_character_position(h, Vec3::new(1.0, 20.0, 1.0))
            .unwrap();

        assert_eq!(state(&world, h).velocity, Vec3::ZERO);
    }

    #[test]
    fn test_character_horizontal_movement() {
        let (mut world, h) = standing_on_floor();
        world
            .set_character_look(h, std::f32::consts::FRAC_PI_2, 0.0)
            .unwrap();
        world
            .add_character_velocity(h, Vec3::new(4.0, 0.0, 0.0))
            .unwrap();
        let start = state(&world, h).position;
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        for _ in 0..30 {
            world
                .move_character(h, &forward, &test_settings(), DT)
                .unwrap();
            world.step(DT);

            let y = state(&world, h).position.y;
            assert!(
                (1.1..1.3).contains(&y),
                "capsule centre left the floor, y={y}"
            );
            assert!(state(&world, h).grounded, "character lost ground contact");
        }

        let end = state(&world, h).position;
        assert!(
            (end.x - start.x - 2.0).abs() < 0.05,
            "30 frames at 4 m/s should move 2.0 m, start={}, end={}",
            start.x,
            end.x
        );
    }

    #[test]
    fn landing_character_stops_vertical_velocity() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let h = world.add_character(Vec3::new(0.0, 3.0, 0.0));

        for _ in 0..60 {
            move_idle(&mut world, h);
            world.step(DT);
        }

        assert!(state(&world, h).grounded);
        assert_eq!(state(&world, h).velocity.y, 0.0);
    }

    #[test]
    fn airborne_character_falls_by_integrated_gravity() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..10 {
            move_idle(&mut world, h);
            world.step(DT);
        }

        let y = state(&world, h).position.y;
        assert!(
            (y - 9.725).abs() < 1e-4,
            "10 frames of -18 m/s^2 should drop 0.275 m, y={y}"
        );
    }

    #[test]
    fn moving_one_character_leaves_the_other_in_place() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let first = world.add_character(Vec3::new(-4.0, 1.2, 0.0));
        let second = world.add_character(Vec3::new(4.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[first, second]);
        let first_before = state(&world, first).position;
        let second_before = state(&world, second);
        world
            .add_character_velocity(first, Vec3::new(3.0, 0.0, 0.0))
            .unwrap();
        let second_before_move = state(&world, second);

        world
            .move_character(first, &idle(), &frictionless(), DT)
            .unwrap();
        world.step(DT);

        let first_after = state(&world, first).position;
        assert!(
            (first_after.x - first_before.x - 3.0 * DT).abs() < 1e-3,
            "first should move 0.05 m along X, before={first_before}, after={first_after}"
        );
        assert_eq!(state(&world, second), second_before);
        assert_eq!(second_before_move, second_before);
    }

    #[test]
    fn each_character_keeps_its_own_grounded_state() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let standing = world.add_character(Vec3::new(-4.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[standing]);
        let airborne = world.add_character(Vec3::new(4.0, 8.0, 0.0));

        move_idle(&mut world, standing);
        move_idle(&mut world, airborne);
        world.step(DT);

        assert!(state(&world, standing).grounded);
        assert!(!state(&world, airborne).grounded);
    }

    #[test]
    fn removing_a_character_refuses_its_moves_and_keeps_the_others() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        let second = world.add_character(Vec3::new(4.0, 10.0, 0.0));

        let removed = world.remove_character(first);
        let moved_second = world.move_character(second, &idle(), &test_settings(), DT);
        let moved_first = world.move_character(first, &idle(), &test_settings(), DT);

        assert_eq!(removed, Ok(()));
        assert!(moved_second.is_ok());
        assert!(
            state(&world, second).position.y < 10.0,
            "second still falls"
        );
        assert_eq!(moved_first, Err(UnknownCharacter));
        assert_eq!(world.character(first), None);
    }

    #[test]
    fn stale_handle_does_not_reach_a_new_character() {
        let mut world = PhysicsWorld::new();
        let old = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world.remove_character(old).unwrap();
        let new_position = Vec3::new(5.0, 10.0, 0.0);
        let new = world.add_character(new_position);

        let moved = world.move_character(
            old,
            &CharacterCommand {
                forward: 1.0,
                ..idle()
            },
            &test_settings(),
            DT,
        );
        world.step(DT);

        assert_ne!(old, new);
        assert_eq!(moved, Err(UnknownCharacter));
        assert_eq!(
            world.character(new),
            Some(CharacterState {
                position: new_position,
                velocity: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                grounded: false,
            })
        );
    }

    #[test]
    fn removing_a_character_removes_its_body_and_capsule() {
        let mut world = PhysicsWorld::new();
        let bodies = world.rigid_body_set.len();
        let colliders = world.collider_set.len();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        assert_eq!(world.rigid_body_set.len(), bodies + 1);
        assert_eq!(world.collider_set.len(), colliders + 1);

        world.remove_character(h).unwrap();

        assert_eq!(world.rigid_body_set.len(), bodies);
        assert_eq!(world.collider_set.len(), colliders);
    }

    #[test]
    fn removing_a_character_twice_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world.remove_character(h).unwrap();

        let again = world.remove_character(h);

        assert_eq!(again, Err(UnknownCharacter));
    }

    #[test]
    fn teleporting_one_character_keeps_the_others_fall_speed() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        let second = world.add_character(Vec3::new(4.0, 10.0, 0.0));
        for _ in 0..10 {
            move_idle(&mut world, first);
            move_idle(&mut world, second);
        }

        world
            .set_character_position(first, Vec3::new(0.0, 20.0, 0.0))
            .unwrap();

        let kept = state(&world, second).velocity.y;
        assert!((kept + 3.0).abs() < 1e-4, "second keeps -3.0, v={kept}");
        assert_eq!(state(&world, first).velocity, Vec3::ZERO);
    }

    #[test]
    fn add_character_velocity_changes_only_that_character() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        let second = world.add_character(Vec3::new(4.0, 10.0, 0.0));

        let set = world.add_character_velocity(first, Vec3::new(0.0, 8.0, 0.0));

        assert_eq!(set, Ok(()));
        assert_eq!(state(&world, first).velocity, Vec3::new(0.0, 8.0, 0.0));
        assert_eq!(state(&world, second).velocity, Vec3::ZERO);
    }

    #[test]
    fn added_velocities_accumulate() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        world
            .add_character_velocity(h, Vec3::new(1.0, 2.0, 3.0))
            .unwrap();
        world
            .add_character_velocity(h, Vec3::new(1.0, 0.0, -1.0))
            .unwrap();

        assert_eq!(state(&world, h).velocity, Vec3::new(2.0, 2.0, 2.0));
    }

    #[test]
    fn debug_output_counts_live_characters() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        world.add_character(Vec3::new(4.0, 10.0, 0.0));
        world.remove_character(first).unwrap();

        let shown = format!("{world:?}");

        assert_eq!(shown, "PhysicsWorld { characters: 1, .. }");
    }

    #[test]
    fn add_velocity_on_removed_handle_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world.remove_character(h).unwrap();

        let set = world.add_character_velocity(h, Vec3::new(0.0, 8.0, 0.0));

        assert_eq!(set, Err(UnknownCharacter));
    }

    #[test]
    fn set_character_look_sets_facing_and_leaves_position_and_velocity() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world
            .add_character_velocity(h, Vec3::new(1.0, 0.0, 0.0))
            .unwrap();
        let before = state(&world, h);

        let set = world.set_character_look(h, 0.7, -0.3);

        let after = state(&world, h);
        assert_eq!(set, Ok(()));
        assert_eq!((after.yaw, after.pitch), (0.7, -0.3));
        assert_eq!(after.position, before.position);
        assert_eq!(after.velocity, before.velocity);
    }

    #[test]
    fn set_character_look_on_removed_handle_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world.remove_character(h).unwrap();

        let set = world.set_character_look(h, 0.7, -0.3);

        assert_eq!(set, Err(UnknownCharacter));
    }

    #[test]
    fn move_character_applies_look_delta_to_the_stored_facing() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        let turn = CharacterCommand {
            yaw_delta: 0.4,
            pitch_delta: 0.2,
            ..idle()
        };

        world
            .move_character(h, &turn, &test_settings(), DT)
            .unwrap();

        let after = state(&world, h);
        assert!((after.yaw - 0.4).abs() < 1e-6, "{after:?}");
        assert!((after.pitch - 0.2).abs() < 1e-6, "{after:?}");
    }

    #[test]
    fn running_into_a_wall_clears_speed_into_it_and_reports_one_impact() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(5.0));
        let wall = world
            .world_mesh_collider(WorldMeshId(1))
            .expect("wall registered");
        world
            .add_character_velocity(h, Vec3::new(0.0, 0.0, 10.0))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 40);

        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert_eq!(impacts[0].collider, wall);
        assert!((impacts[0].speed - 10.0).abs() < 1e-2, "{:?}", impacts[0]);
        assert!(impacts[0].normal.distance(Vec3::new(0.0, 0.0, -1.0)) < 1e-2);
        let after = state(&world, h);
        assert!(after.velocity.z <= 1e-3, "{:?}", after.velocity);
        assert!(after.position.z < 5.0, "{:?}", after.position);
    }

    #[test]
    fn running_at_45_degrees_keeps_speed_along_the_wall() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(5.0));
        let along = 4.0 * std::f32::consts::FRAC_1_SQRT_2;
        world
            .add_character_velocity(h, Vec3::new(along, 0.0, along))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert!((impacts[0].speed - 2.83).abs() < 1e-2, "{:?}", impacts[0]);
        let after = state(&world, h);
        assert!(
            (after.velocity.x - 2.83).abs() < 1e-2,
            "{:?}",
            after.velocity
        );
        assert!(after.velocity.z.abs() < 1e-2, "{:?}", after.velocity);
        assert!(after.position.x > 2.0, "kept sliding: {:?}", after.position);
    }

    #[test]
    fn landing_reports_fall_speed_and_zeroes_vertical_velocity() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let floor = world
            .world_mesh_collider(WorldMeshId(0))
            .expect("floor registered");
        let h = world.add_character(Vec3::new(0.0, 3.0, 0.0));
        world
            .add_character_velocity(h, Vec3::new(0.0, -12.0, 0.0))
            .unwrap();
        let no_gravity = MoveSettings {
            gravity: 0.0,
            ..test_settings()
        };

        let impacts = run(&mut world, h, &no_gravity, 30);

        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert_eq!(impacts[0].collider, floor);
        assert!((impacts[0].speed - 12.0).abs() < 1e-2, "{:?}", impacts[0]);
        assert!(impacts[0].normal.distance(Vec3::Y) < 1e-2);
        let after = state(&world, h);
        assert_eq!(after.velocity.y, 0.0);
        assert!(after.grounded);
    }

    #[test]
    fn added_velocity_moves_then_friction_stops() {
        let (mut world, h) = standing_on_floor();
        let start = state(&world, h).position;
        world
            .add_character_velocity(h, Vec3::new(5.0, 0.0, 0.0))
            .unwrap();

        move_idle(&mut world, h);
        world.step(DT);
        let after_one = state(&world, h);
        let impacts = run(&mut world, h, &test_settings(), 150);

        let end = state(&world, h);
        assert!(
            (after_one.velocity.x - (5.0 - 5.0 * 4.0 * DT)).abs() < 1e-2,
            "{:?}",
            after_one.velocity
        );
        assert!(after_one.position.x > start.x, "moves along +X");
        assert!(end.velocity.length() < 1e-3, "{:?}", end.velocity);
        let travelled = end.position.x - start.x;
        assert!((0.8..1.6).contains(&travelled), "travelled {travelled}");
        assert!(end.position.z.abs() < 1e-2 && (end.position.y - start.y).abs() < 0.05);
        assert!(impacts.is_empty(), "{impacts:?}");
    }

    #[test]
    fn stepping_up_a_half_metre_step_keeps_speed_and_reports_no_impact() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45));
        world
            .add_character_velocity(h, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        assert!(impacts.is_empty(), "{impacts:?}");
        let after = state(&world, h);
        assert!(
            after.position.z > 2.5,
            "climbed onto the step: {:?}",
            after.position
        );
        assert!(
            (after.position.y - (0.45 + 1.15)).abs() < 0.1,
            "standing on the step: {:?}",
            after.position
        );
        assert!(
            (after.velocity.z - 3.0).abs() < 1e-2,
            "{:?}",
            after.velocity
        );
        assert!(after.grounded);
    }
}
