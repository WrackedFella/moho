use super::PhysicsWorld;
use super::GRAVITY;
use glam::Vec3;
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, Pose, QueryFilter, RigidBodyBuilder, RigidBodyHandle,
};

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
    pub vertical_velocity: f32,
    pub grounded: bool,
}

/// The handle names a character that was removed.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("unknown character")]
pub struct UnknownCharacter;

/// Per-character state; the capsule's body handle is the map key.
pub(super) struct Character {
    collider: ColliderHandle,
    vertical_velocity: f32,
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
                vertical_velocity: 0.0,
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

    /// Move the character by `desired_horizontal` with gravity applied, returning its new position.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn move_character(
        &mut self,
        handle: CharacterHandle,
        desired_horizontal: Vec3,
        dt: f32,
    ) -> Result<Vec3, UnknownCharacter> {
        let character = self.characters.get_mut(&handle).ok_or(UnknownCharacter)?;
        let body_handle = handle.0;
        let collider_handle = character.collider;

        if !character.grounded {
            character.vertical_velocity += GRAVITY * dt;
        }

        let desired = Vec3::new(
            desired_horizontal.x,
            character.vertical_velocity * dt,
            desired_horizontal.z,
        );

        let shape = self.collider_set[collider_handle].shape();
        let current_pos = *self.rigid_body_set[body_handle].position();

        // Transient QueryPipeline view that excludes only this character's capsule.
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

        character.grounded = movement.grounded;
        if character.grounded && character.vertical_velocity < 0.0 {
            character.vertical_velocity = 0.0;
        }

        // Set both the "next" and "current" position so the value is available immediately.
        let new_translation = current_pos.translation + movement.translation;
        let new_pose = Pose::from_parts(new_translation, current_pos.rotation);
        self.rigid_body_set[body_handle].set_next_kinematic_position(new_pose);
        self.rigid_body_set[body_handle].set_position(new_pose, false);

        Ok(new_translation)
    }

    /// Teleport the character to `position` and zero its vertical velocity.
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
        character.vertical_velocity = 0.0;
        Ok(())
    }

    /// Set the character's vertical velocity.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn set_vertical_velocity(
        &mut self,
        handle: CharacterHandle,
        velocity: f32,
    ) -> Result<(), UnknownCharacter> {
        self.characters
            .get_mut(&handle)
            .ok_or(UnknownCharacter)?
            .vertical_velocity = velocity;
        Ok(())
    }

    /// The character's current state, or `None` for an unknown handle.
    pub fn character(&self, handle: CharacterHandle) -> Option<CharacterState> {
        let character = self.characters.get(&handle)?;
        Some(CharacterState {
            position: self.rigid_body_set[handle.0].position().translation,
            vertical_velocity: character.vertical_velocity,
            grounded: character.grounded,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::floor_mesh;
    use super::*;
    use moho_render_api::WorldMeshId;

    const DT: f32 = 1.0 / 60.0;

    fn state(world: &PhysicsWorld, handle: CharacterHandle) -> CharacterState {
        world.character(handle).expect("character is live")
    }

    fn settle_on_floor(world: &mut PhysicsWorld, handles: &[CharacterHandle]) {
        for _ in 0..60 {
            for &handle in handles {
                world
                    .move_character(handle, Vec3::ZERO, DT)
                    .expect("character is live");
            }
            world.step(DT);
        }
    }

    #[test]
    fn move_character_with_character_returns_its_new_position() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        let moved = world.move_character(h, Vec3::ZERO, DT);

        assert_eq!(moved, Ok(state(&world, h).position));
    }

    #[test]
    fn test_character_controller_falls_to_floor() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..120 {
            world.move_character(h, Vec3::ZERO, DT).unwrap();
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
            world.move_character(h, Vec3::ZERO, DT).unwrap();
        }
        let speed = state(&world, h).vertical_velocity;
        assert!(
            (speed + 3.0).abs() < 1e-4,
            "10 airborne frames at -18 should give -3.0, v={speed}"
        );

        let target = Vec3::new(3.0, 20.0, -4.0);
        let teleported = world.set_character_position(h, target);

        assert_eq!(teleported, Ok(()));
        assert_eq!(state(&world, h).position, target);
        assert_eq!(state(&world, h).vertical_velocity, 0.0);
    }

    #[test]
    fn test_character_horizontal_movement() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let h = world.add_character(Vec3::new(0.0, 1.0, 0.0));
        settle_on_floor(&mut world, &[h]);
        let start = state(&world, h).position;

        for _ in 0..30 {
            world
                .move_character(h, Vec3::new(4.0 * DT, 0.0, 0.0), DT)
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
            world.move_character(h, Vec3::ZERO, DT).unwrap();
            world.step(DT);
        }

        assert!(state(&world, h).grounded);
        assert_eq!(state(&world, h).vertical_velocity, 0.0);
    }

    #[test]
    fn airborne_character_falls_by_integrated_gravity() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..10 {
            world.move_character(h, Vec3::ZERO, DT).unwrap();
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
            .move_character(first, Vec3::new(1.0, 0.0, 0.0), DT)
            .unwrap();
        world.step(DT);

        let first_after = state(&world, first).position;
        assert!(
            (first_after.x - first_before.x - 1.0).abs() < 1e-3,
            "first should move 1 m along X, before={first_before}, after={first_after}"
        );
        assert_eq!(state(&world, second), second_before);
    }

    #[test]
    fn each_character_keeps_its_own_grounded_state() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0));
        let standing = world.add_character(Vec3::new(-4.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[standing]);
        let airborne = world.add_character(Vec3::new(4.0, 8.0, 0.0));

        world.move_character(standing, Vec3::ZERO, DT).unwrap();
        world.move_character(airborne, Vec3::ZERO, DT).unwrap();
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
        let moved_second = world.move_character(second, Vec3::ZERO, DT);
        let moved_first = world.move_character(first, Vec3::ZERO, DT);

        assert_eq!(removed, Ok(()));
        assert_eq!(moved_second, Ok(state(&world, second).position));
        assert!(moved_second.unwrap().y < 10.0, "second still falls");
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

        let moved = world.move_character(old, Vec3::new(1.0, 0.0, 0.0), DT);
        world.step(DT);

        assert_ne!(old, new);
        assert_eq!(moved, Err(UnknownCharacter));
        assert_eq!(
            world.character(new),
            Some(CharacterState {
                position: new_position,
                vertical_velocity: 0.0,
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
            world.move_character(first, Vec3::ZERO, DT).unwrap();
            world.move_character(second, Vec3::ZERO, DT).unwrap();
        }

        world
            .set_character_position(first, Vec3::new(0.0, 20.0, 0.0))
            .unwrap();

        let kept = state(&world, second).vertical_velocity;
        assert!((kept + 3.0).abs() < 1e-4, "second keeps -3.0, v={kept}");
        assert_eq!(state(&world, first).vertical_velocity, 0.0);
    }

    #[test]
    fn set_vertical_velocity_changes_only_that_character() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        let second = world.add_character(Vec3::new(4.0, 10.0, 0.0));

        let set = world.set_vertical_velocity(first, 8.0);

        assert_eq!(set, Ok(()));
        assert_eq!(state(&world, first).vertical_velocity, 8.0);
        assert_eq!(state(&world, second).vertical_velocity, 0.0);
    }

    #[test]
    fn set_vertical_velocity_on_a_removed_handle_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world.remove_character(h).unwrap();

        let set = world.set_vertical_velocity(h, 8.0);

        assert_eq!(set, Err(UnknownCharacter));
    }
}
