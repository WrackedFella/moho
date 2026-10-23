use super::query::{encode_tag, HitTag};
use super::slide::Slider;
use super::PhysicsWorld;
use crate::movement::{self, CharacterCommand, Impact, MoveSettings};
use glam::{Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, InteractionGroups, Pose, QueryFilter, RigidBodyBuilder,
    RigidBodyHandle,
};

/// Opaque identity of one character in a [`PhysicsWorld`].
///
/// Only meaningful to the world that issued it: another world may resolve it to
/// one of its own characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CharacterHandle(pub(super) RigidBodyHandle);

/// Snapshot of one character's simulation state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterState {
    /// Capsule centre, world space.
    pub position: Vec3,
    /// Velocity in m/s; along the ground while grounded.
    pub velocity: Vec3,
    /// Facing in radians; 0 faces +Z.
    pub yaw: f32,
    /// Look angle in radians, positive up, within `PITCH_LIMIT`.
    pub pitch: f32,
    /// Standing on walkable ground after the last tick.
    pub grounded: bool,
}

/// A box in the character's body frame, rotated by its facing about +Y, offset from the
/// capsule centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitVolume {
    /// Box centre relative to the capsule centre, in the body frame.
    pub offset: Vec3,
    /// Box half-sizes along the body frame's axes.
    pub half_extents: Vec3,
    /// Tag reported when a ray hits this volume.
    pub tag: HitTag,
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

impl Character {
    /// This character's state with the body at `position`.
    fn state(&self, position: Vec3) -> CharacterState {
        CharacterState {
            position,
            velocity: self.velocity,
            yaw: self.yaw,
            pitch: self.pitch,
            grounded: self.grounded,
        }
    }

    /// Keep everything of `state` except the position, which lives on the body.
    fn store(&mut self, state: &CharacterState) {
        self.velocity = state.velocity;
        self.yaw = state.yaw;
        self.pitch = state.pitch;
        self.grounded = state.grounded;
    }
}

impl PhysicsWorld {
    /// Move the character's body to `position`, facing `yaw`, as both the next and the current
    /// pose so the value is available immediately.
    fn place_body(&mut self, handle: CharacterHandle, position: Vec3, yaw: f32) {
        let body = &mut self.rigid_body_set[handle.0];
        let pose = Pose::from_parts(position, Quat::from_rotation_y(yaw));
        body.set_next_kinematic_position(pose);
        body.set_position(pose, false);
    }

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

    /// Attach a hit volume to the character's body.
    ///
    /// The volume follows the body when the world steps, never blocks movement or props, and
    /// is removed with the character.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn add_hit_volume(
        &mut self,
        handle: CharacterHandle,
        volume: HitVolume,
    ) -> Result<(), UnknownCharacter> {
        if !self.characters.contains_key(&handle) {
            return Err(UnknownCharacter);
        }
        let collider = ColliderBuilder::cuboid(
            volume.half_extents.x,
            volume.half_extents.y,
            volume.half_extents.z,
        )
        .translation(volume.offset)
        .sensor(true)
        .collision_groups(InteractionGroups::none())
        .user_data(encode_tag(volume.tag))
        .build();
        self.collider_set
            .insert_with_parent(collider, handle.0, &mut self.rigid_body_set);
        Ok(())
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
    /// Sensors, hit volumes among them, never block the slide.
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
        let collider_handle = character.collider;
        let pose = *self.rigid_body_set[handle.0].position();
        let mut state = character.state(pose.translation);
        let was_grounded = state.grounded;

        // Transient QueryPipeline view without this character's capsule and hit volumes, nor
        // any other character's hit volumes; capsules still meet capsules.
        let filter = QueryFilter::default()
            .exclude_rigid_body(handle.0)
            .exclude_sensors();
        let queries = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter,
        );
        let slider = Slider {
            controller: &self.character_controller,
            queries: &queries,
            shape: self.collider_set[collider_handle].shape(),
            dt,
        };

        let impacts = movement::step(&mut state, command, settings, dt, |asked| {
            slider.slide(&pose, asked, was_grounded)
        });

        self.characters
            .get_mut(&handle)
            .expect("the handle was found at the start of this call")
            .store(&state);
        self.place_body(handle, state.position, state.yaw);
        Ok(impacts)
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
        character.velocity = Vec3::ZERO;
        let yaw = character.yaw;
        self.place_body(handle, position, yaw);
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
        self.characters
            .get_mut(&handle)
            .ok_or(UnknownCharacter)?
            .velocity += velocity;
        Ok(())
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
        let character = self.characters.get_mut(&handle).ok_or(UnknownCharacter)?;
        character.yaw = yaw;
        character.pitch = pitch;
        let position = self.rigid_body_set[handle.0].position().translation;
        self.place_body(handle, position, yaw);
        Ok(())
    }

    /// The character's current state, or `None` for an unknown handle.
    pub fn character(&self, handle: CharacterHandle) -> Option<CharacterState> {
        let character = self.characters.get(&handle)?;
        Some(character.state(self.rigid_body_set[handle.0].position().translation))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::TEST_TAG;
    use super::super::test_support::{
        floor_mesh, quad_mesh, seam_floor_meshes, stairs_mesh, step_box_mesh, test_settings,
        wall_mesh,
    };
    use super::super::{HitTag, RayHit, RayQuery};
    use super::*;
    use moho_render_api::{WorldMesh, WorldMeshId};

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
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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

    /// Like [`run`] with `command`, also returning the position after every tick.
    fn trace(
        world: &mut PhysicsWorld,
        handle: CharacterHandle,
        command: &CharacterCommand,
        settings: &MoveSettings,
        ticks: usize,
    ) -> (Vec<Impact>, Vec<Vec3>) {
        let mut impacts = Vec::new();
        let mut path = Vec::new();
        for _ in 0..ticks {
            let tick = world
                .move_character(handle, command, settings, DT)
                .expect("character is live");
            impacts.extend(tick.into_iter().filter(|i| i.speed > 1e-3));
            world.step(DT);
            path.push(state(world, handle).position);
        }
        (impacts, path)
    }

    /// A straight climb of a 0.45 m step: no sideways drift, no backward step, no tick
    /// that jumps further than the walk or the step-up reach, never above the step top.
    fn assert_clean_climb(start: Vec3, path: &[Vec3], speed: f32) {
        let mut previous = start;
        for (tick, position) in path.iter().enumerate() {
            let advance = position.z - previous.z;
            assert!(position.x.abs() < 1e-3, "tick {tick}: {position:?}");
            assert!(
                advance >= -1e-4,
                "tick {tick} went back: {previous:?} -> {position:?}"
            );
            assert!(
                advance <= speed * DT + 0.25,
                "tick {tick} advanced {advance}: {previous:?} -> {position:?}"
            );
            assert!(
                position.y <= 0.45 + 1.15 + 0.05,
                "tick {tick} rose above the step top: {position:?}"
            );
            previous = *position;
        }
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
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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
            (y - 9.75).abs() < 1e-4,
            "10 frames of -18 m/s^2 should drop g*t^2/2 = 0.25 m, y={y}"
        );
    }

    /// Settles on a floor at `dt`, jumps once at 6.5 m/s under 18 m/s^2, and returns how high
    /// the centre got above its resting height.
    fn jump_peak(dt: f32) -> f32 {
        let settings = MoveSettings {
            jump_speed: 6.5,
            ..test_settings()
        };
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        for _ in 0..(1.0 / dt) as usize {
            world
                .move_character(h, &idle(), &settings, dt)
                .expect("character is live");
            world.step(dt);
        }
        let rest = state(&world, h).position.y;
        assert!(state(&world, h).grounded, "settled before the jump");
        let jump = CharacterCommand {
            jump: true,
            ..idle()
        };

        let mut peak = rest;
        for tick in 0..(2.0 / dt) as usize {
            let command = if tick == 0 { jump } else { idle() };
            world
                .move_character(h, &command, &settings, dt)
                .expect("character is live");
            world.step(dt);
            peak = peak.max(state(&world, h).position.y);
        }

        peak - rest
    }

    #[test]
    fn jump_peaks_at_v_squared_over_2g_at_60_hz() {
        let height = jump_peak(1.0 / 60.0);

        assert!((height - 1.1736).abs() < 0.01, "peak {height}, want 1.1736");
    }

    #[test]
    fn jump_peaks_at_v_squared_over_2g_at_120_hz() {
        let height = jump_peak(1.0 / 120.0);

        assert!((height - 1.1736).abs() < 0.01, "peak {height}, want 1.1736");
    }

    #[test]
    fn fall_of_5_m_lands_on_the_tick_nearest_sqrt_2h_over_g() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let h = world.add_character(Vec3::new(0.0, 5.0 + 1.15, 0.0));
        world.step(DT);

        let mut landed_on = None;
        for tick in 1..=120 {
            move_idle(&mut world, h);
            world.step(DT);
            if state(&world, h).grounded {
                landed_on = Some(tick);
                break;
            }
        }

        assert_eq!(landed_on, Some(45), "sqrt(2*5/18)*60 = 44.7");
    }

    #[test]
    fn moving_one_character_leaves_the_other_in_place() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(5.0), TEST_TAG);
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
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(2.0), TEST_TAG);
        let along = 4.0 * std::f32::consts::FRAC_1_SQRT_2;
        world
            .add_character_velocity(h, Vec3::new(along, 0.0, along))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 90);

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
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
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
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        world
            .add_character_velocity(h, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();

        let start = state(&world, h).position;

        let (impacts, path) = trace(&mut world, h, &idle(), &frictionless(), 60);

        assert!(impacts.is_empty(), "{impacts:?}");
        assert_clean_climb(start, &path, 3.0);
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

    #[test]
    fn step_taller_than_the_autostep_blocks_and_reports_one_impact() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.7), TEST_TAG);
        let step = world
            .world_mesh_collider(WorldMeshId(1))
            .expect("step registered");
        world
            .add_character_velocity(h, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        let after = state(&world, h);
        assert!(
            after.position.y < 1.5,
            "did not climb: {:?}",
            after.position
        );
        assert!(after.position.z < 2.0, "{:?}", after.position);
        assert!(after.velocity.z.abs() < 1e-2, "{:?}", after.velocity);
        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert_eq!(impacts[0].collider, step);
        assert!((impacts[0].speed - 3.0).abs() < 0.05, "{:?}", impacts[0]);
    }

    #[test]
    fn jumping_into_a_low_ceiling_reports_the_hit_and_stops_rising() {
        let (mut world, h) = standing_on_floor();
        let ceiling = quad_mesh(
            &[
                [-10.0, 2.8, -10.0],
                [10.0, 2.8, -10.0],
                [10.0, 2.8, 10.0],
                [-10.0, 2.8, 10.0],
            ],
            &[0, 1, 2, 0, 2, 3],
        );
        world.set_world_mesh(WorldMeshId(1), &ceiling, TEST_TAG);
        let ceiling_collider = world
            .world_mesh_collider(WorldMeshId(1))
            .expect("ceiling registered");
        let jump = CharacterCommand {
            jump: true,
            ..idle()
        };
        let mut hits: Vec<(usize, Impact)> = Vec::new();
        let mut vy_after_hit = None;

        for tick in 0..120 {
            let command = if tick == 0 { jump } else { idle() };
            let impacts = world
                .move_character(h, &command, &test_settings(), DT)
                .unwrap();
            world.step(DT);
            if vy_after_hit.is_none() && !hits.is_empty() {
                vy_after_hit = Some(state(&world, h).velocity.y);
            }
            hits.extend(
                impacts
                    .into_iter()
                    .filter(|i| i.speed > 1e-3)
                    .map(|i| (tick, i)),
            );
        }

        let ceiling_hits: Vec<_> = hits
            .iter()
            .filter(|(_, i)| i.collider == ceiling_collider)
            .collect();
        assert_eq!(ceiling_hits.len(), 1, "{hits:?}");
        assert!(ceiling_hits[0].1.normal.y < -0.9, "{:?}", ceiling_hits[0]);
        assert!(ceiling_hits[0].1.speed > 0.0);
        let vy = vy_after_hit.expect("a hit was recorded before the last tick");
        assert!(vy <= 0.0, "no longer rising after the hit: {vy}");
        let end = state(&world, h);
        assert!(end.grounded, "{end:?}");
        assert!((1.1..1.3).contains(&end.position.y), "{:?}", end.position);
    }

    #[test]
    fn walking_off_a_ledge_falls_without_an_impact_and_keeps_horizontal_speed() {
        let mut world = PhysicsWorld::new();
        let ledge = quad_mesh(
            &[
                [-10.0, 0.0, -10.0],
                [10.0, 0.0, -10.0],
                [10.0, 0.0, 1.0],
                [-10.0, 0.0, 1.0],
            ],
            &[0, 2, 1, 0, 3, 2],
        );
        world.set_world_mesh(WorldMeshId(0), &ledge, TEST_TAG);
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[h]);
        assert!(state(&world, h).grounded);
        world
            .add_character_velocity(h, Vec3::new(0.0, 0.0, 3.0))
            .unwrap();
        let mut impacts = Vec::new();
        let mut left_ground = false;

        for _ in 0..40 {
            let tick = world
                .move_character(h, &idle(), &frictionless(), DT)
                .unwrap();
            world.step(DT);
            impacts.extend(tick.into_iter().filter(|i| i.speed > 1e-3));
            left_ground |= !state(&world, h).grounded;
        }

        let after = state(&world, h);
        assert!(left_ground, "walked off the edge: {after:?}");
        assert!(!after.grounded, "{after:?}");
        assert!(after.velocity.y < 0.0, "{:?}", after.velocity);
        assert!(
            (after.velocity.z - 3.0).abs() < 1e-2,
            "{:?}",
            after.velocity
        );
        assert!(impacts.is_empty(), "{impacts:?}");
    }

    #[test]
    fn upward_knockback_lifts_a_grounded_character_and_landing_reports_it() {
        let (mut world, h) = standing_on_floor();
        let floor = world
            .world_mesh_collider(WorldMeshId(0))
            .expect("floor registered");
        let start_y = state(&world, h).position.y;
        world
            .add_character_velocity(h, Vec3::new(0.0, 5.0, 0.0))
            .unwrap();

        let first = world
            .move_character(h, &idle(), &test_settings(), DT)
            .unwrap();
        world.step(DT);

        let lifted = state(&world, h);
        assert!(first.iter().all(|i| i.speed <= 1e-3), "{first:?}");
        assert!(!lifted.grounded, "{lifted:?}");
        assert!(lifted.position.y > start_y, "{lifted:?}");
        let impacts = run(&mut world, h, &test_settings(), 119);
        let end = state(&world, h);
        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert_eq!(impacts[0].collider, floor);
        assert!((impacts[0].speed - 5.0).abs() < 0.5, "{:?}", impacts[0]);
        assert!(end.grounded, "{end:?}");
    }

    fn strategy_settings() -> MoveSettings {
        MoveSettings {
            max_speed: 4.0,
            ground_accel: 360.0,
            friction: 60.0,
            stop_speed: 1.0,
            air_accel: 360.0,
            air_speed_cap: 6.0,
            gravity: 18.0,
            jump_speed: 8.0,
        }
    }

    /// Holds `command` for `ticks`, collecting impacts faster than 1 m/s.
    fn hold(
        world: &mut PhysicsWorld,
        handle: CharacterHandle,
        command: &CharacterCommand,
        ticks: usize,
    ) -> Vec<Impact> {
        let mut impacts = Vec::new();
        for _ in 0..ticks {
            let tick = world
                .move_character(handle, command, &strategy_settings(), DT)
                .expect("character is live");
            impacts.extend(tick.into_iter().filter(|i| i.speed > 1.0));
            world.step(DT);
        }
        impacts
    }

    #[test]
    fn walking_up_stairs_reaches_the_top_without_impacts() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(
            WorldMeshId(1),
            &stairs_mesh(5, 0.2, 0.3, 2.0, 4.0),
            TEST_TAG,
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        let impacts = hold(&mut world, h, &forward, 75);

        let after = state(&world, h);
        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(
            (after.position.y - (1.0 + 1.15)).abs() < 0.1,
            "{:?}",
            after.position
        );
        assert!(after.position.z > 2.0 + 4.0 * 0.3, "{:?}", after.position);
        assert!(after.grounded, "{after:?}");
    }

    fn searchlight_settings() -> MoveSettings {
        MoveSettings {
            max_speed: 6.0,
            ground_accel: 80.0,
            friction: 10.0,
            stop_speed: 1.0,
            air_accel: 20.0,
            air_speed_cap: 1.0,
            gravity: 18.0,
            jump_speed: 6.5,
        }
    }

    /// A character resting on the world at `at`, facing `yaw` and carrying `velocity`.
    fn resting_then_moving(
        world: &mut PhysicsWorld,
        at: Vec3,
        yaw: f32,
        velocity: Vec3,
    ) -> CharacterHandle {
        let h = world.add_character(at);
        world.set_character_look(h, yaw, 0.0).unwrap();
        settle_on_floor(world, &[h]);
        assert!(state(world, h).grounded, "resting before the test starts");
        world.add_character_velocity(h, velocity).unwrap();
        h
    }

    /// One tick of `command` at searchlight's settings: the state after it and its impacts.
    fn searchlight_tick(
        world: &mut PhysicsWorld,
        handle: CharacterHandle,
        command: &CharacterCommand,
    ) -> (CharacterState, Vec<Impact>) {
        let impacts = world
            .move_character(handle, command, &searchlight_settings(), DT)
            .expect("character is live");
        world.step(DT);
        let impacts = impacts.into_iter().filter(|i| i.speed > 1e-3).collect();
        (state(world, handle), impacts)
    }

    /// Walks from the top landing of five `rise` m steps (0.3 m treads) at 6 m/s facing -Z
    /// down to the bottom floor, asserting it is grounded after every tick and takes no impact.
    fn assert_descends_stairs_grounded(rise: f32) {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(
            WorldMeshId(1),
            &stairs_mesh(5, rise, 0.3, 2.0, 4.0),
            TEST_TAG,
        );
        let landing_y = 5.0 * rise + 1.15;
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, landing_y + 0.05, 6.0),
            std::f32::consts::PI,
            Vec3::new(0.0, 0.0, -6.0),
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        let mut reached_the_bottom = false;

        for tick in 0..200 {
            let (now, impacts) = searchlight_tick(&mut world, h, &forward);

            assert!(now.grounded, "rise {rise}, tick {tick}: airborne, {now:?}");
            assert!(
                impacts.is_empty(),
                "rise {rise}, tick {tick}: {impacts:?}, {now:?}"
            );
            if now.position.z < 1.0 {
                reached_the_bottom = true;
                break;
            }
        }

        let end = state(&world, h);
        assert!(reached_the_bottom, "never got down: {end:?}");
        assert!(
            (end.position.y - 1.15).abs() < 0.1,
            "rise {rise}: standing on the bottom floor: {end:?}"
        );
    }

    #[test]
    fn walking_down_0_2_m_stairs_stays_grounded_without_impacts() {
        assert_descends_stairs_grounded(0.2);
    }

    #[test]
    fn walking_down_0_3_m_stairs_stays_grounded_without_impacts() {
        assert_descends_stairs_grounded(0.3);
    }

    #[test]
    fn walking_down_0_45_m_stairs_stays_grounded_without_impacts() {
        assert_descends_stairs_grounded(0.45);
    }

    /// Level ground at y = 0 that ends at z = 1.
    fn ledge_mesh() -> WorldMesh {
        quad_mesh(
            &[
                [-10.0, 0.0, -10.0],
                [10.0, 0.0, -10.0],
                [10.0, 0.0, 1.0],
                [-10.0, 0.0, 1.0],
            ],
            &[0, 2, 1, 0, 3, 2],
        )
    }

    #[test]
    fn walker_with_its_centre_over_a_ledge_edge_stays_grounded() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &ledge_mesh(), TEST_TAG);
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        settle_on_floor(&mut world, &[h]);
        assert!(state(&world, h).grounded);
        // Centre 0.2 m past the edge and 6 cm up: the rim hangs more than the controller's own
        // snap distance above the edge, so only the ground query keeps the walker standing.
        let over_the_void = Vec3::new(0.0, 1.21, 1.2);
        world.set_character_position(h, over_the_void).unwrap();

        let (now, impacts) = searchlight_tick(&mut world, h, &idle());

        assert!(now.grounded, "{now:?}");
        assert!(impacts.is_empty(), "{impacts:?}");
        // Rim resting on the edge a skin's width away: 0.85 + sqrt((0.3 + 0.0115)^2 - 0.2^2).
        // Lowering onto the floor's plane instead would end near 1.15.
        assert!(
            (1.075..1.11).contains(&now.position.y),
            "lowered until the capsule meets the edge: {now:?}"
        );
    }

    #[test]
    fn walking_off_a_drop_taller_than_the_step_height_falls() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(-6.0, 6.0, 0.6), TEST_TAG);
        let floor = world
            .world_mesh_collider(WorldMeshId(0))
            .expect("floor registered");
        let platform_y = 0.6 + 1.15;
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, platform_y + 0.05, -3.0),
            0.0,
            Vec3::new(0.0, 0.0, 6.0),
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        let mut left_the_ground_at = None;
        let mut impacts = Vec::new();

        for _ in 0..90 {
            let (now, tick_impacts) = searchlight_tick(&mut world, h, &forward);
            impacts.extend(tick_impacts);
            if left_the_ground_at.is_none() && !now.grounded {
                left_the_ground_at = Some(now);
            }
        }

        let left = left_the_ground_at.expect("walked off a 0.6 m drop and never fell");
        assert!(
            left.position.z <= 0.3 + 0.1 + 1e-3,
            "airborne within a tick of the capsule clearing the edge: {left:?}"
        );
        assert!(
            left.position.y > platform_y - 0.2,
            "not lowered down the drop before falling: {left:?}"
        );
        let landing = impacts
            .iter()
            .find(|i| i.collider == floor)
            .expect("the fall lands on the floor with an impact");
        assert!(landing.speed > 3.0, "{landing:?}");
    }

    #[test]
    fn jumping_on_a_stair_is_airborne_on_that_tick() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(
            WorldMeshId(1),
            &stairs_mesh(5, 0.2, 0.3, 2.0, 4.0),
            TEST_TAG,
        );
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, 1.0 + 1.15 + 0.05, 3.3),
            std::f32::consts::PI,
            Vec3::ZERO,
        );
        let before = state(&world, h);
        let jump = CharacterCommand {
            forward: 1.0,
            jump: true,
            ..idle()
        };

        let (after, _) = searchlight_tick(&mut world, h, &jump);

        assert!(!after.grounded, "{after:?}");
        assert!(
            (after.velocity.y - (6.5 - 18.0 * DT)).abs() < 0.05,
            "{:?}",
            after.velocity
        );
        assert!(after.position.y > before.position.y, "{before:?} {after:?}");
    }

    #[test]
    fn crossing_a_seam_between_two_meshes_stays_grounded() {
        for offset in 0..10 {
            let mut world = PhysicsWorld::new();
            for (id, mesh) in seam_floor_meshes(0.025).iter().enumerate() {
                world.set_world_mesh(WorldMeshId(id as u64), mesh, TEST_TAG);
            }
            let start = Vec3::new(0.0, 1.2, -1.5 + offset as f32 * 0.01);
            let h = resting_then_moving(&mut world, start, 0.0, Vec3::new(0.0, 0.0, 6.0));
            let forward = CharacterCommand {
                forward: 1.0,
                ..idle()
            };

            for tick in 0..30 {
                let (now, impacts) = searchlight_tick(&mut world, h, &forward);

                assert!(
                    now.grounded,
                    "offset {offset}, tick {tick}: airborne, {now:?}"
                );
                assert!(
                    impacts.is_empty(),
                    "offset {offset}, tick {tick}: {impacts:?}"
                );
            }
            assert!(state(&world, h).position.z > 1.0, "crossed the seam");
        }
    }

    #[test]
    fn walking_into_a_step_at_45_degrees_climbs_it() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        world
            .set_character_look(h, std::f32::consts::FRAC_PI_4, 0.0)
            .unwrap();
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        let impacts = hold(&mut world, h, &forward, 120);

        let after = state(&world, h);
        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(
            (after.position.y - (0.45 + 1.15)).abs() < 0.1,
            "{:?}",
            after.position
        );
        assert!(after.grounded, "{after:?}");
    }

    /// Stands the character against the riser of a step of `height`, jumps with forward
    /// held, and returns the state before and after that one tick.
    fn jump_pressed_against_step(height: f32) -> (CharacterState, CharacterState) {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, height), TEST_TAG);
        let rest_y = state(&world, h).position.y;
        world
            .set_character_position(h, Vec3::new(0.0, rest_y, 2.0 - 0.3 - 0.03))
            .unwrap();
        for _ in 0..10 {
            world
                .move_character(h, &idle(), &strategy_settings(), DT)
                .unwrap();
            world.step(DT);
        }
        let before = state(&world, h);
        let jump = CharacterCommand {
            forward: 1.0,
            jump: true,
            ..idle()
        };

        world
            .move_character(h, &jump, &strategy_settings(), DT)
            .unwrap();
        world.step(DT);

        (before, state(&world, h))
    }

    fn assert_rose_by_jump_velocity(before: &CharacterState, after: &CharacterState) {
        assert!(before.grounded, "pressed against the riser: {before:?}");
        assert!(
            (after.velocity.y - (8.0 - 18.0 / 60.0)).abs() < 1e-3,
            "{:?}",
            after.velocity
        );
        assert!(!after.grounded, "{after:?}");
        assert!(
            ((after.position.y - before.position.y) - (8.0 - 18.0 / 60.0) / 60.0).abs() < 1e-2,
            "before {:?}, after {:?}",
            before.position,
            after.position
        );
    }

    #[test]
    fn jumping_while_pressed_against_a_step_rises_on_the_first_tick() {
        let (before, after) = jump_pressed_against_step(0.7);

        assert_rose_by_jump_velocity(&before, &after);
    }

    #[test]
    fn walking_onto_a_half_metre_step_reports_no_impact_and_keeps_walking_speed() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        let start = state(&world, h).position;
        let mut fastest: f32 = 0.0;
        let mut path = Vec::new();
        let mut impacts = Vec::new();

        for _ in 0..60 {
            let (tick_impacts, tick_path) = trace(&mut world, h, &forward, &strategy_settings(), 1);
            impacts.extend(tick_impacts.into_iter().filter(|i| i.speed > 0.5));
            path.extend(tick_path);
            let v = state(&world, h).velocity;
            fastest = fastest.max(Vec3::new(v.x, 0.0, v.z).length());
        }
        assert_clean_climb(start, &path, 4.0);

        let after = state(&world, h);
        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(fastest <= 4.0 + 1e-3, "fastest horizontal speed {fastest}");
        assert!(
            (after.position.y - (0.45 + 1.15)).abs() < 0.1,
            "{:?}",
            after.position
        );
        assert!(after.grounded, "{after:?}");
    }

    #[test]
    fn airborne_character_is_not_stepped_onto_a_ledge() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        let step = world
            .world_mesh_collider(WorldMeshId(1))
            .expect("step registered");
        let h = world.add_character(Vec3::new(0.0, 1.15 + 0.1, 2.0 - 0.3 - 0.03));
        world
            .add_character_velocity(h, Vec3::new(0.0, -1.0, 3.0))
            .unwrap();
        world.step(DT);
        assert!(!state(&world, h).grounded);

        let (impacts, path) = trace(&mut world, h, &idle(), &frictionless(), 1);

        assert!(
            path[0].y < 0.45 + 1.15 - 0.2,
            "not lifted onto the step: {:?}",
            path[0]
        );
        assert!(
            impacts
                .iter()
                .any(|i| i.collider == step && i.normal.z < -0.5),
            "riser impact expected: {impacts:?}"
        );
    }

    #[test]
    fn landing_while_pressed_into_a_wall_reports_the_fall_speed() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(0.35), TEST_TAG);
        let floor = world
            .world_mesh_collider(WorldMeshId(0))
            .expect("floor registered");
        let h = world.add_character(Vec3::new(0.0, 3.0, 0.0));
        world
            .add_character_velocity(h, Vec3::new(0.0, -12.0, 0.0))
            .unwrap();
        world.step(DT);
        let no_gravity = MoveSettings {
            gravity: 0.0,
            ..test_settings()
        };
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        let (impacts, _) = trace(&mut world, h, &forward, &no_gravity, 30);

        let floor_hits: Vec<_> = impacts.iter().filter(|i| i.collider == floor).collect();
        assert_eq!(floor_hits.len(), 1, "{impacts:?}");
        assert!(
            (floor_hits[0].speed - 12.0).abs() < 1e-2,
            "{:?}",
            floor_hits[0]
        );
        let after = state(&world, h);
        assert_eq!(after.velocity.y, 0.0);
        assert!(after.grounded, "{after:?}");
    }

    const HEAD: HitTag = HitTag(10);
    const TORSO: HitTag = HitTag(11);
    const ARM: HitTag = HitTag(12);

    fn volume(offset: Vec3, half_extents: Vec3, tag: HitTag) -> HitVolume {
        HitVolume {
            offset,
            half_extents,
            tag,
        }
    }

    fn attach(world: &mut PhysicsWorld, handle: CharacterHandle, volume: HitVolume) {
        world
            .add_hit_volume(handle, volume)
            .expect("character is live");
    }

    fn cast(
        world: &PhysicsWorld,
        origin: Vec3,
        direction: Vec3,
        ignore: Option<CharacterHandle>,
    ) -> Option<RayHit> {
        world.cast_ray(&RayQuery {
            origin,
            direction,
            max_distance: 20.0,
            ignore,
        })
    }

    /// A head above a torso, both centred on the character.
    fn attach_head_and_torso(world: &mut PhysicsWorld, handle: CharacterHandle) {
        attach(
            world,
            handle,
            volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD),
        );
        attach(
            world,
            handle,
            volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
        );
    }

    #[test]
    fn ray_at_head_height_hits_the_head_volume() {
        let (mut world, h) = standing_on_floor();
        attach_head_and_torso(&mut world, h);
        world.step(DT);
        let at = state(&world, h).position;

        let head = cast(&world, at + Vec3::new(-3.0, 0.7, 0.0), Vec3::X, None);
        let torso = cast(&world, at + Vec3::new(-3.0, 0.1, 0.0), Vec3::X, None);

        let head = head.expect("a ray level with the head hits it");
        assert_eq!(head.tag, HEAD);
        assert!((head.distance - 2.8).abs() < 0.02, "{head:?}");
        assert_eq!(torso.map(|hit| hit.tag), Some(TORSO));
    }

    #[test]
    fn volumes_follow_the_character_when_it_moves() {
        let (mut world, h) = standing_on_floor();
        attach_head_and_torso(&mut world, h);
        world.step(DT);
        let start = state(&world, h).position;
        let walk_east = CharacterCommand {
            forward: 1.0,
            yaw_delta: std::f32::consts::FRAC_PI_2,
            ..idle()
        };
        let forward = CharacterCommand {
            yaw_delta: 0.0,
            ..walk_east
        };

        let mut command = walk_east;
        for _ in 0..200 {
            if state(&world, h).position.x >= start.x + 3.0 {
                break;
            }
            world
                .move_character(h, &command, &test_settings(), DT)
                .unwrap();
            world.step(DT);
            command = forward;
        }

        let at = state(&world, h).position;
        assert!(at.x >= start.x + 3.0, "walked 3 m along X: {at:?}");
        let new_head = cast(&world, at + Vec3::new(0.0, 0.7, -3.0), Vec3::Z, None);
        let old_head = cast(&world, start + Vec3::new(0.0, 0.7, -3.0), Vec3::Z, None);
        assert_eq!(new_head.map(|hit| hit.tag), Some(HEAD), "at {at:?}");
        assert_eq!(old_head, None, "nothing is left where the head was");
    }

    #[test]
    fn volumes_turn_with_the_character() {
        let (mut world, h) = standing_on_floor();
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.4, 0.0, 0.0), Vec3::new(0.1, 0.3, 0.1), ARM),
        );
        world.step(DT);
        let at = state(&world, h).position;
        let from_original_side = at + Vec3::new(3.0, 0.0, 0.0);
        let from_opposite_side = at + Vec3::new(-3.0, 0.0, 0.0);
        let before = cast(&world, from_original_side, -Vec3::X, None);
        assert_eq!(before.map(|hit| hit.tag), Some(ARM), "arm starts at +X");

        let turn = CharacterCommand {
            yaw_delta: std::f32::consts::PI,
            ..idle()
        };
        world
            .move_character(h, &turn, &test_settings(), DT)
            .unwrap();
        world.step(DT);

        // Reaches only to the capsule centre: the capsule never stops a ray, so a longer ray
        // would go on to the arm on the far side.
        let after_original = world.cast_ray(&RayQuery {
            origin: from_original_side,
            direction: -Vec3::X,
            max_distance: 3.0,
            ignore: None,
        });
        let after_opposite = cast(&world, from_opposite_side, Vec3::X, None)
            .expect("the arm is now on the opposite side");
        assert_eq!(after_opposite.tag, ARM);
        assert!(
            (after_opposite.point.x - (at.x - 0.5)).abs() < 0.02,
            "{after_opposite:?}"
        );
        assert_eq!(after_original, None, "the arm left its original side");
    }

    #[test]
    fn ray_through_a_gap_between_volumes_hits_the_wall() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(3.0), TEST_TAG);
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
        );
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.7, 0.1, 0.0), Vec3::new(0.1, 0.4, 0.15), ARM),
        );
        world.step(DT);
        let at = state(&world, h).position;

        let through_gap = cast(&world, at + Vec3::new(0.45, 0.1, -3.0), Vec3::Z, None);
        let through_arm = cast(&world, at + Vec3::new(0.7, 0.1, -3.0), Vec3::Z, None);

        let through_gap = through_gap.expect("the wall is behind the gap");
        assert_eq!(through_gap.tag, TEST_TAG);
        assert!((through_gap.point.z - 3.0).abs() < 1e-3, "{through_gap:?}");
        assert_eq!(through_arm.map(|hit| hit.tag), Some(ARM));
    }

    #[test]
    fn ray_ignoring_the_shooter_hits_the_other_characters_torso() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let shooter = world.add_character(Vec3::new(0.0, 1.2, -2.0));
        let target = world.add_character(Vec3::new(0.0, 1.2, 2.0));
        settle_on_floor(&mut world, &[shooter, target]);
        attach(
            &mut world,
            shooter,
            volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.3, 0.2), HEAD),
        );
        attach(
            &mut world,
            target,
            volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
        );
        attach(
            &mut world,
            target,
            volume(Vec3::new(0.0, 0.95, 0.0), Vec3::new(0.2, 0.2, 0.2), ARM),
        );
        world.step(DT);
        let muzzle = state(&world, shooter).position + Vec3::new(0.0, 0.45, 0.0);

        let ignoring = cast(&world, muzzle, Vec3::Z, Some(shooter));

        let ignoring = ignoring.expect("the target's torso is in line");
        assert_eq!(ignoring.tag, TORSO);
        assert!((ignoring.distance - 3.85).abs() < 0.02, "{ignoring:?}");
    }

    #[test]
    fn overlapping_volumes_do_not_stop_a_walking_character() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let walker = world.add_character(Vec3::new(0.0, 1.2, -1.5));
        let standing = world.add_character(Vec3::new(0.0, 1.2, 1.5));
        settle_on_floor(&mut world, &[walker, standing]);
        let reach = Vec3::new(0.3, 0.4, 0.3);
        attach(
            &mut world,
            walker,
            volume(Vec3::new(0.0, 0.0, 0.8), reach, ARM),
        );
        attach(
            &mut world,
            standing,
            volume(Vec3::new(0.0, 0.0, -0.8), reach, ARM),
        );
        world.step(DT);
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        trace(&mut world, walker, &forward, &test_settings(), 150);

        let gap = state(&world, standing).position.z - state(&world, walker).position.z;
        assert!(
            (gap - 0.6).abs() < 0.08,
            "capsules of radius 0.3 meet with centres 0.6 apart, gap={gap}"
        );
    }

    #[test]
    fn removing_a_character_removes_its_volumes() {
        let mut world = PhysicsWorld::new();
        let colliders = world.collider_set.len();
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        attach_head_and_torso(&mut world, h);
        world.step(DT);
        let head_ray = |world: &PhysicsWorld| cast(world, Vec3::new(-3.0, 1.9, 0.0), Vec3::X, None);
        assert_eq!(head_ray(&world).map(|hit| hit.tag), Some(HEAD));

        world.remove_character(h).unwrap();
        world.step(DT);

        assert_eq!(head_ray(&world), None);
        assert_eq!(world.collider_set.len(), colliders);
    }

    #[test]
    fn adding_a_volume_to_a_removed_character_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        world.remove_character(h).unwrap();
        let colliders = world.collider_set.len();

        let added = world.add_hit_volume(h, volume(Vec3::ZERO, Vec3::splat(0.2), HEAD));

        assert_eq!(added, Err(UnknownCharacter));
        assert_eq!(world.collider_set.len(), colliders);
    }

    #[test]
    fn a_characters_own_volumes_do_not_block_its_movement() {
        let (mut world, h) = standing_on_floor();
        let start = state(&world, h).position;
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.0, 0.52), Vec3::new(0.2, 0.3, 0.2), ARM),
        );
        world.step(DT);
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        trace(&mut world, h, &forward, &test_settings(), 30);

        let advanced = state(&world, h).position.z - start.z;
        assert!(
            advanced > 1.5,
            "30 ticks at 4 m/s covers about 1.9 m, advanced={advanced}"
        );
    }

    #[test]
    fn a_falling_prop_passes_through_a_hit_volume() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 5.0, 0.0));
        attach(
            &mut world,
            h,
            volume(Vec3::new(2.0, 0.0, 0.0), Vec3::new(1.0, 0.5, 1.0), TORSO),
        );
        // Beside the capsule, so only the volume is under the prop.
        let prop = world.add_dynamic_sphere(Vec3::new(2.0, 8.0, 0.0), 0.2);

        for _ in 0..90 {
            world.step(DT);
        }

        let y = world.body_position(prop).expect("prop is live").y;
        assert!(
            y < 4.0,
            "the prop fell through the volume (4.5..5.5), y={y}"
        );
    }

    #[test]
    fn ray_starting_inside_an_unignored_volume_hits_it_at_zero_distance() {
        let (mut world, h) = standing_on_floor();
        attach_head_and_torso(&mut world, h);
        world.step(DT);
        let inside = state(&world, h).position + Vec3::new(0.0, 0.7, 0.0);

        let hit = cast(&world, inside, Vec3::X, None);

        let hit = hit.expect("a muzzle inside a volume hits it");
        assert_eq!(hit.tag, HEAD);
        assert!(hit.distance.abs() < 1e-4, "{hit:?}");
    }

    /// An arm at +X before any turn, with the rays that find it on either side.
    fn arm_on_plus_x() -> (PhysicsWorld, CharacterHandle) {
        let (mut world, h) = standing_on_floor();
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.4, 0.0, 0.0), Vec3::new(0.1, 0.3, 0.1), ARM),
        );
        world.step(DT);
        (world, h)
    }

    /// The x of the arm's face seen from -X, or `None` when no arm is on that side.
    fn arm_face_from_minus_x(world: &PhysicsWorld, h: CharacterHandle) -> Option<f32> {
        let at = state(world, h).position;
        let hit = world.cast_ray(&RayQuery {
            origin: at + Vec3::new(-3.0, 0.0, 0.0),
            direction: Vec3::X,
            max_distance: 3.0,
            ignore: None,
        })?;
        (hit.tag == ARM).then_some(hit.point.x - at.x)
    }

    #[test]
    fn setting_the_look_turns_the_volumes() {
        let (mut world, h) = arm_on_plus_x();

        world
            .set_character_look(h, std::f32::consts::PI, 0.0)
            .unwrap();
        world.step(DT);

        let face = arm_face_from_minus_x(&world, h).expect("the arm turned to -X");
        assert!((face + 0.5).abs() < 0.02, "face at {face}");
    }

    #[test]
    fn teleporting_keeps_the_volumes_facing() {
        let (mut world, h) = arm_on_plus_x();
        world
            .set_character_look(h, std::f32::consts::PI, 0.0)
            .unwrap();

        world
            .set_character_position(h, Vec3::new(2.0, 1.2, 0.0))
            .unwrap();
        world.step(DT);

        let face = arm_face_from_minus_x(&world, h).expect("the arm stays at -X");
        assert!((face + 0.5).abs() < 0.02, "face at {face}");
    }

    #[test]
    fn looking_up_does_not_tilt_the_volumes() {
        let (mut world, h) = standing_on_floor();
        attach_head_and_torso(&mut world, h);
        world.step(DT);
        let look_up = CharacterCommand {
            pitch_delta: 1.0,
            ..idle()
        };

        world
            .move_character(h, &look_up, &test_settings(), DT)
            .unwrap();
        world.step(DT);

        assert!(state(&world, h).pitch > 0.5, "{:?}", state(&world, h));
        let at = state(&world, h).position;
        let head = cast(&world, at + Vec3::new(-3.0, 0.7, 0.0), Vec3::X, None);
        let head = head.expect("the head stays level with where it was");
        assert_eq!(head.tag, HEAD);
        assert!((head.distance - 2.8).abs() < 0.02, "{head:?}");
    }

    #[test]
    fn a_quarter_turn_swings_a_forward_volume_toward_plus_x() {
        let (mut world, h) = standing_on_floor();
        attach(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.0, 0.4), Vec3::new(0.1, 0.3, 0.1), ARM),
        );

        world
            .set_character_look(h, std::f32::consts::FRAC_PI_2, 0.0)
            .unwrap();
        world.step(DT);

        let at = state(&world, h).position;
        let hit = cast(&world, at + Vec3::new(3.0, 0.0, 0.0), -Vec3::X, None)
            .expect("yaw pi/2 faces +X, so the forward arm is there");
        assert_eq!(hit.tag, ARM);
        assert!((hit.point.x - (at.x + 0.5)).abs() < 0.02, "{hit:?}");
    }

    mod take_off {
        use super::*;
        use crate::world::test_support::slope_mesh;

        const SLOPE_DEGREES: f32 = 25.0;
        const SPEED: f32 = 4.0;
        const AIRBORNE_TICKS: usize = 4;

        fn gravity() -> f32 {
            test_settings().gravity
        }

        fn rise_per_run() -> f32 {
            SLOPE_DEGREES.to_radians().tan()
        }

        /// The vertical speed of a walker moving at `SPEED` along a slope of `SLOPE_DEGREES`.
        fn slope_climb() -> f32 {
            SPEED * rise_per_run()
        }

        fn forward() -> CharacterCommand {
            CharacterCommand {
                forward: 1.0,
                ..idle()
            }
        }

        fn tick(
            world: &mut PhysicsWorld,
            handle: CharacterHandle,
            command: &CharacterCommand,
        ) -> CharacterState {
            world
                .move_character(handle, command, &test_settings(), DT)
                .expect("character is live");
            world.step(DT);
            state(world, handle)
        }

        fn assert_close(actual: Vec3, expected: Vec3, context: &str) {
            assert!(
                actual.distance(expected) < 1e-2,
                "{context}: expected {expected:?}, got {actual:?}"
            );
        }

        fn rest_at(world: &mut PhysicsWorld, at: Vec3, yaw: f32) -> CharacterHandle {
            let h = world.add_character(at);
            world.set_character_look(h, yaw, 0.0).unwrap();
            for _ in 0..60 {
                tick(world, h, &idle());
            }
            assert!(state(world, h).grounded, "resting on the ground");
            h
        }

        fn run_steadily(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
            ticks: usize,
        ) -> CharacterState {
            for _ in 0..ticks {
                tick(world, h, &forward());
            }
            let now = state(world, h);
            assert!(now.grounded, "{now:?}");
            assert!(
                (now.velocity.with_y(0.0).length() - SPEED).abs() < 1e-2,
                "running at speed: {now:?}"
            );
            now
        }

        /// Runs forward until a tick ends airborne; returns the state after the last tick that
        /// ended grounded and the state after the tick that left the ground.
        fn run_off_the_edge(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
        ) -> (CharacterState, CharacterState) {
            let mut last_grounded = state(world, h);
            for _ in 0..300 {
                let now = tick(world, h, &forward());
                if !now.grounded {
                    return (last_grounded, now);
                }
                last_grounded = now;
            }
            panic!("never left the ground: {:?}", state(world, h));
        }

        /// With no input only gravity changes the velocity, by `gravity * dt` each tick.
        fn assert_only_gravity_acts(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
            from: CharacterState,
        ) {
            let mut previous = from;
            for n in 0..AIRBORNE_TICKS {
                let now = tick(world, h, &idle());

                assert!(!now.grounded, "airborne tick {n}: {now:?}");
                assert_close(
                    now.velocity,
                    previous.velocity - Vec3::Y * gravity() * DT,
                    &format!("airborne tick {n}"),
                );
                previous = now;
            }
        }

        /// Jumps with forward held from `before` and checks the take-off keeps its velocity.
        fn assert_jump_keeps_velocity_plus_impulse(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
            before: CharacterState,
        ) {
            let jump = CharacterCommand {
                jump: true,
                ..forward()
            };

            let after = tick(world, h, &jump);

            assert!(!after.grounded, "{after:?}");
            assert_close(
                after.velocity,
                before.velocity + Vec3::Y * (test_settings().jump_speed - gravity() * DT),
                "first airborne tick",
            );
            assert_only_gravity_acts(world, h, after);
        }

        /// Walking off an edge keeps the velocity of the last grounded tick, then gravity acts.
        fn assert_walk_off_keeps_velocity(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
        ) -> CharacterState {
            let (last_grounded, left_ground) = run_off_the_edge(world, h);

            assert_close(left_ground.velocity, last_grounded.velocity, "take-off");
            assert_only_gravity_acts(world, h, left_ground);
            last_grounded
        }

        #[test]
        fn flat_jump_keeps_ground_velocity_plus_impulse() {
            let mut world = PhysicsWorld::new();
            world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
            let h = rest_at(&mut world, Vec3::new(0.0, 1.2, 0.0), 0.0);
            let before = run_steadily(&mut world, h, 30);

            assert_jump_keeps_velocity_plus_impulse(&mut world, h, before);

            assert_close(
                before.velocity,
                Vec3::new(0.0, 0.0, SPEED),
                "ground velocity",
            );
        }

        #[test]
        fn upslope_jump_keeps_slope_velocity_plus_impulse() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(
                WorldMeshId(0),
                &slope_mesh(-10.0, -10.0 * rise, 10.0, 10.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, -5.0 * rise + 1.5, -5.0), 0.0);
            let before = run_steadily(&mut world, h, 30);
            assert!(
                (before.velocity.y - slope_climb()).abs() < 0.15,
                "a walker climbing the slope rises with it: {:?}",
                before.velocity
            );

            assert_jump_keeps_velocity_plus_impulse(&mut world, h, before);
        }

        #[test]
        fn walking_off_a_slope_side_keeps_velocity() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(
                WorldMeshId(0),
                &slope_mesh(-10.0, -10.0 * rise, 10.0, 10.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(
                &mut world,
                Vec3::new(7.0, 1.5, 0.0),
                std::f32::consts::FRAC_PI_2,
            );

            let last_grounded = assert_walk_off_keeps_velocity(&mut world, h);

            assert_close(
                last_grounded.velocity,
                Vec3::new(SPEED, 0.0, 0.0),
                "contour velocity",
            );
        }

        #[test]
        fn walking_off_a_downhill_edge_keeps_its_descent() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(
                WorldMeshId(0),
                &slope_mesh(-8.0, 8.0 * rise, 0.0, 0.0),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, 6.0 * rise + 1.5, -6.0), 0.0);

            let last_grounded = assert_walk_off_keeps_velocity(&mut world, h);

            assert!(
                (last_grounded.velocity.y + slope_climb()).abs() < 0.15,
                "descending with the slope: {:?}",
                last_grounded.velocity
            );
            assert!(
                (last_grounded.velocity.z - SPEED).abs() < 1e-2,
                "{:?}",
                last_grounded.velocity
            );
        }

        #[test]
        fn walking_off_a_ledge_keeps_level_velocity() {
            let mut world = PhysicsWorld::new();
            world.set_world_mesh(WorldMeshId(0), &slope_mesh(-10.0, 0.0, 1.0, 0.0), TEST_TAG);
            let h = rest_at(&mut world, Vec3::new(0.0, 1.2, -4.0), 0.0);

            let last_grounded = assert_walk_off_keeps_velocity(&mut world, h);

            assert_close(
                last_grounded.velocity,
                Vec3::new(0.0, 0.0, SPEED),
                "level velocity",
            );
        }

        #[test]
        fn running_off_a_ramp_top_rises_before_falling() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(WorldMeshId(0), &slope_mesh(-10.0, 0.0, 0.0, 0.0), TEST_TAG);
            world.set_world_mesh(
                WorldMeshId(1),
                &slope_mesh(0.0, 0.0, 6.0, 6.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, 1.2, -4.0), 0.0);
            let mut path = Vec::new();
            for _ in 0..300 {
                let now = tick(&mut world, h, &forward());
                path.push((now.grounded, now.position));
            }

            let crest_z = 6.0;
            let take_off = path
                .iter()
                .position(|&(grounded, position)| !grounded && position.z > 0.5)
                .expect("left the ramp's top");
            let launch = path[take_off].1;

            assert!(
                (launch.z - crest_z).abs() < 0.5,
                "left the ground at the crest, not before it: {launch:?}"
            );
            assert!(
                path.iter()
                    .filter(|&&(_, p)| p.z > 0.5 && p.z < crest_z - 0.5)
                    .all(|&(grounded, _)| grounded),
                "stayed on the ramp until the crest"
            );
            let after = &path[take_off..];
            assert!(
                after.windows(2).take(3).all(|w| w[1].1.y > w[0].1.y),
                "keeps rising past the crest: {:?}",
                &after[..4]
            );
            let peak = after.iter().map(|&(_, p)| p.y).fold(f32::MIN, f32::max);
            assert!(
                peak - launch.y < 0.3,
                "a ramp launch rises about climb^2 / 2g = 0.1 m, not a jump's height: {}",
                peak - launch.y
            );
            let end = &path[path.len() - 2..];
            assert!(
                end[1].1.y < end[0].1.y && end[1].1.y < peak - 1.0,
                "then gravity brings it down: peak {peak}, end {end:?}"
            );
        }

        fn record(
            world: &mut PhysicsWorld,
            h: CharacterHandle,
            command: &CharacterCommand,
            ticks: usize,
        ) -> Vec<CharacterState> {
            (0..ticks).map(|_| tick(world, h, command)).collect()
        }

        /// Fails with the ticks around the first one that left the ground.
        fn assert_grounded_throughout(trace: &[CharacterState]) {
            if let Some(first) = trace.iter().position(|s| !s.grounded) {
                let from = first.saturating_sub(3);
                let window: Vec<_> = trace[from..(first + 4).min(trace.len())]
                    .iter()
                    .enumerate()
                    .map(|(n, s)| (from + n, s.grounded, s.position, s.velocity))
                    .collect();
                panic!("left the ground at tick {first}: {window:#?}");
            }
        }

        fn assert_velocity_y_where(
            trace: &[CharacterState],
            from_z: f32,
            expected: f32,
            what: &str,
        ) {
            let checked: Vec<_> = trace.iter().filter(|s| s.position.z > from_z).collect();
            assert!(!checked.is_empty(), "{what}: never got past z = {from_z}");
            for s in checked {
                assert!(
                    (s.velocity.y - expected).abs() < 0.15,
                    "{what}: expected velocity.y {expected}, got {:?} at {:?}",
                    s.velocity,
                    s.position
                );
            }
        }

        #[test]
        fn running_up_a_ramp_onto_a_plateau_stays_grounded() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(WorldMeshId(0), &slope_mesh(-10.0, 0.0, 0.0, 0.0), TEST_TAG);
            world.set_world_mesh(
                WorldMeshId(1),
                &slope_mesh(0.0, 0.0, 6.0, 6.0 * rise),
                TEST_TAG,
            );
            world.set_world_mesh(
                WorldMeshId(2),
                &slope_mesh(6.0, 6.0 * rise, 20.0, 6.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, 1.2, -4.0), 0.0);
            let floor_rest = state(&world, h).position.y;

            let trace = record(&mut world, h, &forward(), 240);

            assert_grounded_throughout(&trace);
            let standing = 6.0 * rise + floor_rest;
            let on_plateau: Vec<_> = trace.iter().filter(|s| s.position.z > 9.0).collect();
            assert!(!on_plateau.is_empty(), "never reached the plateau");
            for s in on_plateau {
                assert!(s.velocity.y.abs() < 0.05, "{s:?}");
                assert!(
                    s.position.y < standing + 0.02,
                    "hopped at the join: standing {standing}, {s:?}"
                );
            }
        }

        #[test]
        fn running_over_a_hill_crest_stays_grounded() {
            let mut world = PhysicsWorld::new();
            let rise = 20.0_f32.to_radians().tan();
            world.set_world_mesh(
                WorldMeshId(0),
                &slope_mesh(-8.0, 0.0, 0.0, 8.0 * rise),
                TEST_TAG,
            );
            world.set_world_mesh(
                WorldMeshId(1),
                &slope_mesh(0.0, 8.0 * rise, 10.0, -2.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, 2.0 * rise + 1.5, -6.0), 0.0);

            let trace = record(&mut world, h, &forward(), 220);

            assert_grounded_throughout(&trace);
            assert_velocity_y_where(&trace, 1.5, -SPEED * rise, "down the far side");
        }

        #[test]
        fn walking_from_level_ground_onto_a_downslope_stays_grounded() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(WorldMeshId(0), &slope_mesh(-10.0, 0.0, 0.0, 0.0), TEST_TAG);
            world.set_world_mesh(
                WorldMeshId(1),
                &slope_mesh(0.0, 0.0, 12.0, -12.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, 1.2, -4.0), 0.0);

            let trace = record(&mut world, h, &forward(), 200);

            assert_grounded_throughout(&trace);
            assert_velocity_y_where(&trace, 1.5, -slope_climb(), "on the downslope");
        }

        #[test]
        fn upslope_walker_releasing_input_stops_on_the_slope() {
            let mut world = PhysicsWorld::new();
            let rise = rise_per_run();
            world.set_world_mesh(
                WorldMeshId(0),
                &slope_mesh(-10.0, -10.0 * rise, 10.0, 10.0 * rise),
                TEST_TAG,
            );
            let h = rest_at(&mut world, Vec3::new(0.0, -5.0 * rise + 1.5, -5.0), 0.0);
            run_steadily(&mut world, h, 30);

            let trace = record(&mut world, h, &idle(), 120);

            assert_grounded_throughout(&trace);
            let end = trace.last().unwrap();
            assert!(end.velocity.length() < 1e-3, "{end:?}");
            let settled = &trace[trace.len() - 20..];
            for s in settled {
                assert!(
                    s.position.distance(end.position) < 1e-3,
                    "drifting: {:?} vs {:?}",
                    s.position,
                    end.position
                );
            }
        }
    }
}
