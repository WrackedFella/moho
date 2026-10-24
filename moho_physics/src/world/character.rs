use super::query::{encode_tag, HitTag};
use super::slide::{Footing, Slider};
use super::PhysicsWorld;
use crate::movement::{
    self, CharacterBody, CharacterCommand, CharacterEffect, HullShape, Impact, MoveSettings, Tuning,
};
use glam::{Quat, Vec3};
use rapier3d::prelude::{
    ColliderBuilder, ColliderHandle, InteractionGroups, Pose, QueryFilter, RigidBodyBuilder,
    RigidBodyHandle, SharedShape,
};
use serde::{Deserialize, Serialize};

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
    /// The last tick's command had jump down; a held jump needs a release before the next
    /// take-off unless the settings allow autohop.
    pub jump_held: bool,
    /// The eye's offset from `position`, set by the step every tick.
    pub eye: Vec3,
}

/// A character's starting state and shape: where it stands, which way it faces, its hull.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterSpawn {
    /// Capsule centre, world space.
    pub position: Vec3,
    /// Facing in radians; 0 faces +Z.
    pub yaw: f32,
    /// Look angle in radians, positive up.
    pub pitch: f32,
    /// Hull and eye height.
    pub body: CharacterBody,
}

impl CharacterState {
    /// The eye's position in world space.
    pub fn eye_position(&self) -> Vec3 {
        self.position + self.eye
    }
}

impl From<Vec3> for CharacterSpawn {
    fn from(position: Vec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            body: CharacterBody::default(),
        }
    }
}

/// A box in the character's body frame, rotated by its facing about +Y, offset from the
/// capsule centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitVolume {
    /// Box centre relative to the capsule centre, in the body frame.
    pub offset: Vec3,
    /// Rotation in the body frame, applied before `offset`.
    pub rotation: Quat,
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
    body: CharacterBody,
    /// The hull as the ground query sees it, derived once from `body`.
    footing: Footing,
    /// Hit volume colliders, in the order the game last set them.
    volumes: Vec<ColliderHandle>,
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    grounded: bool,
    jump_held: bool,
    /// The eye's offset from the body, as the last step or the spawn set it.
    eye: Vec3,
    /// Changes from outside the step, applied in order by the next `move_character`.
    effects: Vec<CharacterEffect>,
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
            jump_held: self.jump_held,
            eye: self.eye,
        }
    }

    /// Keep everything of `state` except the position, which lives on the body.
    fn store(&mut self, state: &CharacterState) {
        self.velocity = state.velocity;
        self.yaw = state.yaw;
        self.pitch = state.pitch;
        self.grounded = state.grounded;
        self.jump_held = state.jump_held;
        self.eye = state.eye;
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

    /// Add a kinematic character to the world, with the hull `spawn.body` describes.
    ///
    /// # Panics
    /// On an impossible hull ([`CharacterBody::validate`]).
    pub fn add_character(&mut self, spawn: impl Into<CharacterSpawn>) -> CharacterHandle {
        let spawn = spawn.into();
        spawn.body.validate();
        let position = spawn.position;
        let body = RigidBodyBuilder::kinematic_position_based()
            .pose(Pose::from_parts(position, Quat::from_rotation_y(spawn.yaw)))
            .build();
        let body_handle = self.rigid_body_set.insert(body);

        let hull = spawn.body.standing.hull;
        let half_length = hull.length / 2.0;
        let shape = match hull.shape {
            HullShape::Capsule => {
                ColliderBuilder::capsule_y(half_length - hull.radius, hull.radius)
            }
            HullShape::RoundCylinder { rounding } => ColliderBuilder::round_cylinder(
                half_length - rounding,
                hull.radius - rounding,
                rounding,
            ),
        };
        let collider = shape.friction(0.0).build();
        let collider =
            self.collider_set
                .insert_with_parent(collider, body_handle, &mut self.rigid_body_set);

        let handle = CharacterHandle(body_handle);
        self.characters.insert(
            handle,
            Character {
                collider,
                body: spawn.body,
                footing: Footing::of(hull),
                volumes: Vec::new(),
                velocity: Vec3::ZERO,
                yaw: spawn.yaw,
                pitch: spawn
                    .pitch
                    .clamp(-movement::PITCH_LIMIT, movement::PITCH_LIMIT),
                grounded: false,
                jump_held: false,
                eye: spawn.body.standing.eye_offset(),
                effects: Vec::new(),
            },
        );

        tracing::info!(pos = ?position, "Character controller spawned");
        handle
    }

    /// Replace all of the character's hit volumes with `volumes`.
    ///
    /// The volumes take effect when the world steps: rays cast before the next step may see
    /// the old poses, or a mix of old poses and new shapes and tags. They follow the body,
    /// never block movement or props, and are removed with the character. An unchanged count
    /// updates the colliders in place; any other count rebuilds them.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn set_hit_volumes(
        &mut self,
        handle: CharacterHandle,
        volumes: &[HitVolume],
    ) -> Result<(), UnknownCharacter> {
        let character = self.characters.get_mut(&handle).ok_or(UnknownCharacter)?;

        if character.volumes.len() != volumes.len() {
            for stale in character.volumes.drain(..) {
                self.collider_set.remove(
                    stale,
                    &mut self.island_manager,
                    &mut self.rigid_body_set,
                    &mut self.soft_body_set,
                    true,
                );
            }
            for _ in volumes {
                let collider = ColliderBuilder::cuboid(1.0, 1.0, 1.0)
                    .sensor(true)
                    .collision_groups(InteractionGroups::none())
                    .build();
                let inserted = self.collider_set.insert_with_parent(
                    collider,
                    handle.0,
                    &mut self.rigid_body_set,
                );
                character.volumes.push(inserted);
            }
        }

        for (&collider, volume) in character.volumes.iter().zip(volumes) {
            let collider = &mut self.collider_set[collider];
            collider.set_position_wrt_parent(Pose::from_parts(volume.offset, volume.rotation));
            collider.set_shape(SharedShape::cuboid(
                volume.half_extents.x,
                volume.half_extents.y,
                volume.half_extents.z,
            ));
            collider.user_data = encode_tag(volume.tag);
        }
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
        let character = self.characters.get_mut(&handle).ok_or(UnknownCharacter)?;
        let collider_handle = character.collider;
        let effects = std::mem::take(&mut character.effects);
        let body = character.body;
        let footing = character.footing;
        let pose = *self.rigid_body_set[handle.0].position();
        let mut state = character.state(pose.translation);

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
            footing,
            dt,
        };

        let impacts = movement::step(
            &mut state,
            command,
            &effects,
            &Tuning {
                body,
                settings: *settings,
                gravity: settings.gravity.unwrap_or(self.gravity()),
            },
            dt,
            |from, asked| {
                let start = Pose::from_parts(from.position, pose.rotation);
                slider.slide(&start, asked, from.grounded)
            },
        );

        self.characters
            .get_mut(&handle)
            .expect("the handle was found at the start of this call")
            .store(&state);
        self.place_body(handle, state.position, state.yaw);
        Ok(impacts)
    }

    /// Queue `effect` to apply at the start of the character's next step.
    ///
    /// # Errors
    /// [`UnknownCharacter`] if `handle` was removed.
    pub fn queue_effect(
        &mut self,
        handle: CharacterHandle,
        effect: CharacterEffect,
    ) -> Result<(), UnknownCharacter> {
        self.characters
            .get_mut(&handle)
            .ok_or(UnknownCharacter)?
            .effects
            .push(effect);
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
        corner_mesh, doorway_mesh, floor_mesh, quad_mesh, seam_floor_meshes, slope_mesh,
        stairs_mesh, step_box_mesh, test_settings, wall_mesh,
    };
    use super::super::{HitTag, RayHit, RayQuery};
    use super::*;
    use crate::movement::{Hull, HullShape, Stature};
    use moho_render_api::{WorldMesh, WorldMeshId};

    const DT: f32 = 1.0 / 60.0;

    /// Half of the 60 degree inside corner the corner tests run into.
    const CORNER_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_6;

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

    fn teleport(world: &mut PhysicsWorld, handle: CharacterHandle, position: Vec3) {
        world
            .queue_effect(
                handle,
                CharacterEffect::Teleport {
                    position,
                    keep_velocity: false,
                },
            )
            .unwrap();
    }

    #[test]
    fn teleport_effect_moves_the_character_on_the_next_tick_and_stops_falling() {
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
        let before = state(&world, h);
        let target = Vec3::new(3.0, 20.0, -4.0);

        let queued = world.queue_effect(
            h,
            CharacterEffect::Teleport {
                position: target,
                keep_velocity: false,
            },
        );

        assert_eq!(queued, Ok(()));
        assert_eq!(state(&world, h), before, "queueing changes nothing yet");
        move_idle(&mut world, h);
        let after = state(&world, h);
        assert!(
            after.position.distance(target) < 0.01,
            "one tick of falling from the target, {:?}",
            after.position
        );
        assert!(
            (after.velocity.y + 18.0 * DT).abs() < 1e-4,
            "the fall restarted from rest, v={}",
            after.velocity.y
        );
    }

    #[test]
    fn test_character_horizontal_movement() {
        let (mut world, h) = standing_on_floor();
        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: std::f32::consts::FRAC_PI_2,
                    pitch: 0.0,
                },
            )
            .unwrap();
        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(4.0, 0.0, 0.0)))
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

    #[test]
    fn a_settings_gravity_overrides_the_world() {
        let mut world = PhysicsWorld::new();
        world.set_gravity(20.32);
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        let settings = MoveSettings {
            gravity: Some(9.0),
            ..test_settings()
        };

        world
            .move_character(h, &idle(), &settings, DT)
            .expect("character is live");

        let vy = state(&world, h).velocity.y;
        assert!(
            (vy + 9.0 * DT).abs() < 1e-5,
            "one airborne tick at the override's 9 m/s², vy={vy}"
        );
    }

    /// Settles on a floor at `dt`, jumps once at 6.5 m/s under 18 m/s^2, and returns how high
    /// the centre got above its resting height.
    fn jump_peak(dt: f32) -> f32 {
        let settings = MoveSettings {
            jump_speed: 6.5,
            max_velocity: 90.0,
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

    /// Moves one tick under `command` and returns the vertical speed it gave a character
    /// that ends the tick rising; a landing's stop is not a gain.
    fn tick_gain(
        world: &mut PhysicsWorld,
        handle: CharacterHandle,
        command: &CharacterCommand,
        settings: &MoveSettings,
    ) -> f32 {
        let before = state(world, handle).velocity.y;
        world
            .move_character(handle, command, settings, DT)
            .expect("character is live");
        world.step(DT);
        let after = state(world, handle).velocity.y;
        if after > 0.0 {
            after - before
        } else {
            0.0
        }
    }

    #[test]
    fn holding_jump_through_a_landing_does_not_jump_again() {
        let settings = test_settings();
        let (mut world, h) = standing_on_floor();
        let held = CharacterCommand {
            jump: true,
            ..idle()
        };
        let take_off = settings.jump_speed / 2.0;

        let take_offs = (0..120)
            .filter(|_| tick_gain(&mut world, h, &held, &settings) > take_off)
            .count();

        assert_eq!(take_offs, 1, "one jump, none on the landing");
        assert!(state(&world, h).grounded, "back on the floor");
        let released = tick_gain(&mut world, h, &idle(), &settings);
        let pressed = tick_gain(&mut world, h, &held, &settings);
        assert!(released < take_off, "a release is not a jump: {released}");
        assert!(pressed > take_off, "the new press jumps: {pressed}");
    }

    #[test]
    fn autohop_jumps_again_on_the_landing_tick() {
        let settings = MoveSettings {
            autohop: true,
            ..test_settings()
        };
        let (mut world, h) = standing_on_floor();
        let held = CharacterCommand {
            jump: true,
            ..idle()
        };
        tick_gain(&mut world, h, &held, &settings);
        assert!(!state(&world, h).grounded, "the first tick took off");
        let mut landed = false;
        for _ in 0..120 {
            tick_gain(&mut world, h, &held, &settings);
            if state(&world, h).grounded {
                landed = true;
                break;
            }
        }
        assert!(landed, "the jump comes down within two seconds");

        tick_gain(&mut world, h, &held, &settings);

        let after = state(&world, h);
        assert!(!after.grounded, "left the floor again: {after:?}");
        assert!(
            (after.velocity.y - (settings.jump_speed - 18.0 * DT)).abs() < 1e-3,
            "left with the jump speed: {:?}",
            after.velocity
        );
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
            .queue_effect(
                first,
                CharacterEffect::AddVelocity(Vec3::new(3.0, 0.0, 0.0)),
            )
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
                jump_held: false,
                eye: Vec3::ZERO,
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

        teleport(&mut world, first, Vec3::new(0.0, 20.0, 0.0));
        move_idle(&mut world, first);
        move_idle(&mut world, second);

        let kept = state(&world, second).velocity.y;
        assert!(
            (kept + 3.3).abs() < 1e-4,
            "second keeps falling, -3.0 and one more tick of gravity, v={kept}"
        );
        let restarted = state(&world, first).velocity.y;
        assert!(
            (restarted + 0.3).abs() < 1e-4,
            "first restarted from rest, v={restarted}"
        );
    }

    #[test]
    fn effects_for_one_character_leave_the_others_alone() {
        let mut world = PhysicsWorld::new();
        let first = world.add_character(Vec3::new(-4.0, 10.0, 0.0));
        let second = world.add_character(Vec3::new(4.0, 10.0, 0.0));

        let queued_first = world.queue_effect(
            first,
            CharacterEffect::AddVelocity(Vec3::new(0.0, 8.0, 0.0)),
        );
        let queued_second = world.queue_effect(
            second,
            CharacterEffect::SetVelocity(Vec3::new(0.0, 0.0, -2.0)),
        );

        assert_eq!((queued_first, queued_second), (Ok(()), Ok(())));
        assert_eq!(state(&world, first).velocity, Vec3::ZERO, "not yet applied");
        assert_eq!(
            state(&world, second).velocity,
            Vec3::ZERO,
            "not yet applied"
        );
        move_idle(&mut world, second);
        move_idle(&mut world, first);
        let lifted = state(&world, first).velocity;
        let set = state(&world, second).velocity;
        assert!(
            (lifted - Vec3::new(0.0, 8.0 - 18.0 * DT, 0.0)).length() < 1e-4,
            "first gets only its own: {lifted:?}"
        );
        assert!(
            (set - Vec3::new(0.0, -18.0 * DT, -2.0)).length() < 1e-4,
            "second gets only its own: {set:?}"
        );
    }

    #[test]
    fn queued_velocities_accumulate() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(1.0, 2.0, 3.0)))
            .unwrap();
        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(1.0, 0.0, -1.0)))
            .unwrap();
        move_idle(&mut world, h);

        let velocity = state(&world, h).velocity;
        assert!(
            (velocity - Vec3::new(2.0, 2.0 - 18.0 * DT, 2.0)).length() < 1e-4,
            "{velocity:?}"
        );
    }

    #[test]
    fn queued_effects_apply_in_queue_order() {
        let add = CharacterEffect::AddVelocity(Vec3::new(1.0, 0.0, 0.0));
        let set = CharacterEffect::SetVelocity(Vec3::new(0.0, 0.0, 6.0));
        let big_add = CharacterEffect::AddVelocity(Vec3::new(3.0, 4.0, 5.0));
        let to = CharacterEffect::Teleport {
            position: Vec3::new(1.0, 20.0, 1.0),
            keep_velocity: false,
        };
        let gravity = -18.0 * DT;
        let cases = [
            (
                "add then teleport",
                [big_add, to],
                Vec3::new(0.0, gravity, 0.0),
            ),
            ("add then set", [add, set], Vec3::new(0.0, gravity, 6.0)),
            ("set then add", [set, add], Vec3::new(1.0, gravity, 6.0)),
        ];

        for (what, queue, expected) in cases {
            let mut world = PhysicsWorld::new();
            let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
            for effect in queue {
                world.queue_effect(h, effect).unwrap();
            }

            move_idle(&mut world, h);

            let after = state(&world, h);
            assert!(
                (after.velocity - expected).length() < 1e-4,
                "{what}: {:?}, expected {expected:?}",
                after.velocity
            );
            if what == "add then teleport" {
                assert_eq!(
                    (after.position.x, after.position.z),
                    (1.0, 1.0),
                    "{what}: at the teleport target"
                );
            }
        }
    }

    #[test]
    fn effect_queued_after_the_move_applies_on_the_next_tick() {
        let (mut world, h) = standing_on_floor();
        move_idle(&mut world, h);
        let after_move = state(&world, h);

        let queued = world.queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(5.0, 0.0, 0.0)));

        assert_eq!(queued, Ok(()));
        assert_eq!(state(&world, h), after_move, "queueing changes nothing");
        world
            .move_character(h, &idle(), &frictionless(), DT)
            .unwrap();
        world.step(DT);
        let next = state(&world, h);
        assert!(
            (next.velocity.x - 5.0).abs() < 1e-3,
            "the next tick carries it: {:?}",
            next.velocity
        );
        assert!(
            (next.position.x - after_move.position.x - 5.0 * DT).abs() < 1e-3,
            "and asks its motion: {:?} from {:?}",
            next.position,
            after_move.position
        );
    }

    /// 300 ticks of commands with effects queued along the way, ending on a floor with a step.
    fn mixed_script() -> Vec<(CharacterCommand, Vec<CharacterEffect>)> {
        (0..300_u32)
            .map(|tick| {
                let command = CharacterCommand {
                    forward: if tick % 90 < 60 { 1.0 } else { 0.0 },
                    right: if tick % 110 < 20 { 0.5 } else { 0.0 },
                    yaw_delta: if tick % 40 < 10 { 0.03 } else { -0.01 },
                    pitch_delta: 0.0,
                    jump: tick % 70 == 5,
                };
                let effects = match tick {
                    20 => vec![CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 2.0))],
                    60 => vec![CharacterEffect::SetLook {
                        yaw: 0.2,
                        pitch: 0.1,
                    }],
                    100 => vec![
                        CharacterEffect::AddVelocity(Vec3::new(5.0, 0.0, 0.0)),
                        CharacterEffect::Teleport {
                            position: Vec3::new(0.0, 1.5, -4.0),
                            keep_velocity: false,
                        },
                    ],
                    150 => vec![CharacterEffect::SetVelocity(Vec3::new(1.0, 0.0, 2.0))],
                    200 => vec![CharacterEffect::Teleport {
                        position: Vec3::new(2.0, 3.0, 0.0),
                        keep_velocity: true,
                    }],
                    250 => vec![
                        CharacterEffect::AddVelocity(Vec3::new(-3.0, 4.0, 0.0)),
                        CharacterEffect::SetLook {
                            yaw: -1.0,
                            pitch: 0.0,
                        },
                    ],
                    _ => Vec::new(),
                };
                (command, effects)
            })
            .collect()
    }

    #[test]
    fn a_queued_teleport_moves_the_body_only_on_the_next_move() {
        let (mut world, h) = standing_on_floor();
        set_one_volume(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
        );
        world.step(DT);
        let old = state(&world, h).position;
        let target = Vec3::new(6.0, 1.2, 5.0);
        let torso_at = |world: &PhysicsWorld, at: Vec3| {
            cast(world, at + Vec3::new(-3.0, 0.1, 0.0), Vec3::X, None).map(|hit| hit.tag)
        };

        teleport(&mut world, h, target);
        world.step(DT);

        assert_eq!(torso_at(&world, old), Some(TORSO), "still at the old place");
        assert_eq!(torso_at(&world, target), None, "not yet at the target");
        move_idle(&mut world, h);
        world.step(DT);
        assert_eq!(torso_at(&world, old), None, "gone from the old place");
        assert_eq!(torso_at(&world, target), Some(TORSO), "now at the target");
    }

    type Script = Vec<(CharacterCommand, Vec<CharacterEffect>)>;

    fn floor_and_step_world() -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        world
    }

    fn recorded_spawn() -> CharacterSpawn {
        CharacterSpawn {
            position: Vec3::new(0.0, 1.5, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            body: CharacterBody::default(),
        }
    }

    /// Runs `script` for the recorded character alone in a fresh world, effects queued right
    /// before the tick's move, and returns every tick's position.
    fn replay(script: &Script) -> Vec<Vec3> {
        let mut world = floor_and_step_world();
        let h = world.add_character(recorded_spawn());
        script
            .iter()
            .map(|(command, effects)| {
                for effect in effects {
                    world.queue_effect(h, *effect).unwrap();
                }
                world
                    .move_character(h, command, &strategy_settings(), DT)
                    .unwrap();
                world.step(DT);
                state(&world, h).position
            })
            .collect()
    }

    /// The recorded character's positions in a busy world: a second character with effects of
    /// its own moves first each tick, the recorded one carries a hit volume that rays query,
    /// and its effects are queued right after the previous tick's move.
    fn record(script: &Script) -> Vec<Vec3> {
        let mut world = floor_and_step_world();
        let other = world.add_character(Vec3::new(-8.0, 1.2, -8.0));
        let h = world.add_character(recorded_spawn());
        set_one_volume(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD),
        );
        let others_effects = |tick: usize| match tick {
            10 => vec![CharacterEffect::SetVelocity(Vec3::new(0.0, 0.0, 1.5))],
            50 => vec![CharacterEffect::Teleport {
                position: Vec3::new(-8.0, 1.5, -6.0),
                keep_velocity: false,
            }],
            120 => vec![CharacterEffect::AddVelocity(Vec3::new(0.0, 5.0, 0.0))],
            200 => vec![CharacterEffect::SetLook {
                yaw: 1.0,
                pitch: 0.0,
            }],
            _ => Vec::new(),
        };
        for effect in &script[0].1 {
            world.queue_effect(h, *effect).unwrap();
        }
        let mut positions = Vec::new();
        for (tick, (command, _)) in script.iter().enumerate() {
            for effect in others_effects(tick) {
                world.queue_effect(other, effect).unwrap();
            }
            world
                .move_character(other, &idle(), &strategy_settings(), DT)
                .unwrap();
            world
                .move_character(h, command, &strategy_settings(), DT)
                .unwrap();
            if let Some((_, next)) = script.get(tick + 1) {
                for effect in next {
                    world.queue_effect(h, *effect).unwrap();
                }
            }
            world.step(DT);
            let position = state(&world, h).position;
            let head = cast(
                &world,
                position + Vec3::new(-3.0, 0.7, 0.0),
                Vec3::X,
                Some(other),
            )
            .unwrap_or_else(|| panic!("tick {tick}: the ray found no head at {position:?}"));
            assert_eq!(head.tag, HEAD, "tick {tick}");
            positions.push(position);
        }
        positions
    }

    fn assert_same_positions(actual: &[Vec3], expected: &[Vec3]) {
        assert_eq!(actual.len(), expected.len(), "tick counts");
        let diverged = actual
            .iter()
            .zip(expected)
            .position(|(a, e)| a.to_array().map(f32::to_bits) != e.to_array().map(f32::to_bits));
        if let Some(tick) = diverged {
            panic!(
                "first divergence at tick {tick}: {:?}, recorded {:?}",
                actual[tick], expected[tick]
            );
        }
    }

    #[test]
    fn replaying_commands_and_effects_reproduces_positions_exactly() {
        let script = mixed_script();
        let recording = record(&script);
        let bytes = postcard::to_allocvec(&script).expect("the script serialises");
        let replayed: Script = postcard::from_bytes(&bytes).expect("the script deserialises");

        let rerun = replay(&replayed);

        assert_eq!(rerun.len(), 300);
        assert_same_positions(&rerun, &recording);
        let lowest = recording.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!(
            lowest > 1.0,
            "the script stays on the floor, lowest y {lowest}"
        );
        assert!(
            recording.iter().any(|p| (p.y - (0.45 + 1.15)).abs() < 0.1),
            "some tick rests at step-top height"
        );
        let without_effects: Script = script.iter().map(|(c, _)| (*c, Vec::new())).collect();
        assert_ne!(
            replay(&without_effects),
            recording,
            "the effects changed where the character went"
        );
        assert!(
            recording[100].distance(Vec3::new(0.0, 1.5, -4.0)) < 0.5,
            "the tick-100 teleport took hold: {:?}",
            recording[100]
        );
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
    fn queueing_for_a_removed_character_is_refused() {
        let mut world = PhysicsWorld::new();
        let old = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        let effect = CharacterEffect::AddVelocity(Vec3::new(9.0, 0.0, 0.0));
        world.queue_effect(old, effect).unwrap();
        world.remove_character(old).unwrap();
        let new = world.add_character(Vec3::new(0.0, 10.0, 0.0));

        let queued = world.queue_effect(old, effect);
        move_idle(&mut world, new);

        assert_eq!(queued, Err(UnknownCharacter));
        let velocity = state(&world, new).velocity;
        assert!(
            (velocity - Vec3::new(0.0, -18.0 * DT, 0.0)).length() < 1e-4,
            "nothing queued for the removed character reaches the new one: {velocity:?}"
        );
    }

    #[test]
    fn set_look_effect_sets_facing_on_the_next_tick_and_keeps_velocity() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 10.0, 0.0));
        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(1.0, 0.0, 0.0)))
            .unwrap();
        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: 0.7,
                    pitch: -0.3,
                },
            )
            .unwrap();
        let before = state(&world, h);

        move_idle(&mut world, h);

        let after = state(&world, h);
        assert_eq!(
            (before.yaw, before.pitch),
            (0.0, 0.0),
            "not applied at queue time"
        );
        assert_eq!((after.yaw, after.pitch), (0.7, -0.3));
        assert_eq!(after.velocity.x, 1.0);
        assert!((after.position.x - DT).abs() < 1e-5, "{:?}", after.position);
    }

    #[test]
    fn spawn_look_is_the_characters_initial_facing() {
        let mut world = PhysicsWorld::new();
        let position = Vec3::new(1.0, 10.0, 2.0);

        let h = world.add_character(CharacterSpawn {
            position,
            yaw: 0.7,
            pitch: -0.3,
            body: CharacterBody::default(),
        });
        let plain = world.add_character(position);

        assert_eq!(
            world.character(h),
            Some(CharacterState {
                position,
                velocity: Vec3::ZERO,
                yaw: 0.7,
                pitch: -0.3,
                grounded: false,
                jump_held: false,
                eye: Vec3::ZERO,
            })
        );
        assert_eq!(state(&world, plain).yaw, 0.0, "a bare position faces 0");
    }

    #[test]
    fn spawn_pitch_is_clamped() {
        let mut world = PhysicsWorld::new();
        let spawn = |pitch| CharacterSpawn {
            position: Vec3::new(0.0, 10.0, 0.0),
            yaw: 0.0,
            pitch,
            body: CharacterBody::default(),
        };

        let up = world.add_character(spawn(3.0));
        let down = world.add_character(spawn(-3.0));

        assert_eq!(state(&world, up).pitch, crate::movement::PITCH_LIMIT);
        assert_eq!(state(&world, down).pitch, -crate::movement::PITCH_LIMIT);
    }

    #[test]
    fn spawn_round_trips_through_postcard() {
        let spawn = CharacterSpawn {
            position: Vec3::new(1.0, 2.0, 3.0),
            yaw: 0.5,
            pitch: -0.25,
            body: round_cylinder_body(),
        };

        let bytes = postcard::to_allocvec(&spawn).expect("the spawn serialises");

        let back: CharacterSpawn = postcard::from_bytes(&bytes).expect("the spawn deserialises");
        assert_eq!(back, spawn);
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 10.0)))
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
            .queue_effect(
                h,
                CharacterEffect::AddVelocity(Vec3::new(along, 0.0, along)),
            )
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

    /// A standing character, a floor and a 60 degree inside corner with its apex at `apex_z`.
    /// Returns the colliders of the +X wall and the -X wall.
    fn world_with_corner(apex_z: f32) -> (PhysicsWorld, CharacterHandle, [ColliderHandle; 2]) {
        let (mut world, h) = standing_on_floor();
        let [plus_x, minus_x] = corner_mesh(Vec3::new(0.0, 0.0, apex_z), CORNER_HALF_ANGLE);
        world.set_world_mesh(WorldMeshId(1), &plus_x, TEST_TAG);
        world.set_world_mesh(WorldMeshId(2), &minus_x, TEST_TAG);
        let walls = [WorldMeshId(1), WorldMeshId(2)].map(|id| {
            world
                .world_mesh_collider(id)
                .expect("corner wall registered")
        });
        (world, h, walls)
    }

    /// Normals of the walls from [`corner_mesh`], in the same order.
    fn corner_normals() -> [Vec3; 2] {
        let (sin, cos) = CORNER_HALF_ANGLE.sin_cos();
        [Vec3::new(-cos, 0.0, -sin), Vec3::new(cos, 0.0, -sin)]
    }

    #[test]
    fn running_into_a_60_degree_corner_stops_with_one_impact_per_wall() {
        let (mut world, h, walls) = world_with_corner(5.0);
        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 10.0)))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        for wall in walls {
            let count = impacts.iter().filter(|i| i.collider == wall).count();
            assert_eq!(count, 1, "wall {wall:?} in {impacts:?}");
        }
        assert_eq!(impacts.len(), 2, "{impacts:?}");
        let after = state(&world, h);
        assert!(
            after.position.z > 2.0 && after.position.z < 5.0,
            "reached the corner: {:?}",
            after.position
        );
        for normal in corner_normals() {
            let into = -after.velocity.dot(normal);
            assert!(
                into <= 1e-3,
                "speed into {normal:?} is {into}: {:?}",
                after.velocity
            );
        }
        assert!(
            after.velocity.with_y(0.0).length() <= 1e-3,
            "stopped in the corner: {:?}",
            after.velocity
        );
    }

    #[test]
    fn holding_forward_into_a_corner_reports_at_most_one_impact_per_wall_per_tick() {
        let (mut world, h, walls) = world_with_corner(3.0);
        let forward = CharacterCommand {
            forward: 1.0,
            ..CharacterCommand::default()
        };
        let mut impacts_in_the_corner = 0;

        for tick in 0..120 {
            let impacts = world
                .move_character(h, &forward, &test_settings(), DT)
                .expect("character is live");
            world.step(DT);

            for wall in walls {
                let count = impacts.iter().filter(|i| i.collider == wall).count();
                assert!(
                    count <= 1,
                    "tick {tick}: wall {wall:?} reported {count} times in {impacts:?}"
                );
            }
            impacts_in_the_corner += impacts.len();
        }

        assert!(impacts_in_the_corner > 0, "never reached the corner");
        assert!(state(&world, h).position.z < 3.0);
    }

    #[test]
    fn a_1000_m_s_character_stops_at_a_1_m_wall() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(5.0, 1.0, 10.0), TEST_TAG);
        world
            .queue_effect(h, CharacterEffect::SetVelocity(Vec3::new(0.0, 0.0, 1000.0)))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        let after = state(&world, h);
        assert!(
            after.position.z < 5.0 && after.position.z > 3.0,
            "ended beside the near face: {:?}",
            after.position
        );
        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert!(
            (impacts[0].speed - 90.0).abs() < 1e-2,
            "the slide ran at max_velocity: {:?}",
            impacts[0]
        );
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, -12.0, 0.0)))
            .unwrap();
        let no_gravity = MoveSettings {
            gravity: Some(0.0),
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(5.0, 0.0, 0.0)))
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 3.0)))
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 3.0)))
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 3.0)))
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 5.0, 0.0)))
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
            gravity: Some(18.0),
            jump_speed: 8.0,
            max_velocity: 90.0,
            autohop: true,
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
            gravity: Some(18.0),
            jump_speed: 6.5,
            max_velocity: 90.0,
            autohop: false,
        }
    }

    /// A character resting on the world at `at`, facing `yaw` and carrying `velocity`.
    fn resting_then_moving(
        world: &mut PhysicsWorld,
        at: Vec3,
        yaw: f32,
        velocity: Vec3,
    ) -> CharacterHandle {
        resting_then_moving_as(world, at, yaw, velocity, CharacterBody::default())
    }

    /// [`resting_then_moving`] for a character of `body`.
    fn resting_then_moving_as(
        world: &mut PhysicsWorld,
        at: Vec3,
        yaw: f32,
        velocity: Vec3,
        body: CharacterBody,
    ) -> CharacterHandle {
        let h = world.add_character(CharacterSpawn {
            position: at,
            yaw,
            pitch: 0.0,
            body,
        });
        settle_on_floor(world, &[h]);
        assert!(state(world, h).grounded, "resting before the test starts");
        world
            .queue_effect(h, CharacterEffect::AddVelocity(velocity))
            .unwrap();
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

    /// Walks down five `rise` m steps (0.3 m treads) from the top landing at 2, 4 and 6 m/s and
    /// from 20 start positions 1.37 cm apart, so the ticks meet the nosings at every phase.
    fn assert_descends_stairs_grounded(rise: f32) {
        for speed in [2.0, 4.0, 6.0] {
            for k in 0..20 {
                assert_descends_stairs_grounded_from(rise, speed, 4.0 + k as f32 * 0.0137);
            }
        }
    }

    /// Walks from the landing at `start_z` at `speed` facing -Z down to the bottom floor,
    /// asserting it is grounded after every tick and takes no impact.
    fn assert_descends_stairs_grounded_from(rise: f32, speed: f32, start_z: f32) {
        assert_body_descends_stairs_grounded_from(
            CharacterBody::default(),
            &searchlight_settings(),
            rise,
            speed,
            start_z,
        );
    }

    /// [`assert_descends_stairs_grounded_from`] for a character of `body` moving by `settings`,
    /// whose centre rests half its hull length above the surface it stands on.
    fn assert_body_descends_stairs_grounded_from(
        body: CharacterBody,
        settings: &MoveSettings,
        rise: f32,
        speed: f32,
        start_z: f32,
    ) {
        let half_length = body.standing.hull.length / 2.0;
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(
            WorldMeshId(1),
            &stairs_mesh(5, rise, 0.3, 2.0, 4.0),
            TEST_TAG,
        );
        let landing_y = 5.0 * rise + half_length;
        let h = resting_then_moving_as(
            &mut world,
            Vec3::new(0.0, landing_y + 0.05, start_z),
            std::f32::consts::PI,
            Vec3::new(0.0, 0.0, -speed),
            body,
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        let what = format!("rise {rise}, {speed} m/s, start z {start_z}");
        let mut reached_the_bottom = false;

        for tick in 0..1500 {
            let impacts: Vec<Impact> = world
                .move_character(h, &forward, settings, DT)
                .expect("character is live")
                .into_iter()
                .filter(|i| i.speed > 1e-3)
                .collect();
            world.step(DT);
            let now = state(&world, h);

            assert!(now.grounded, "{what}, tick {tick}: airborne, {now:?}");
            assert!(
                impacts.is_empty(),
                "{what}, tick {tick}: {impacts:?}, {now:?}"
            );
            if now.position.z < 1.0 {
                reached_the_bottom = true;
                break;
            }
        }

        let end = state(&world, h);
        assert!(reached_the_bottom, "{what}: never got down: {end:?}");
        assert!(
            (end.position.y - half_length).abs() < 0.1,
            "{what}: standing on the bottom floor: {end:?}"
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

    fn body_of(shape: HullShape, radius: f32, length: f32, eye_height: f32) -> CharacterBody {
        CharacterBody {
            standing: Stature {
                hull: Hull {
                    shape,
                    radius,
                    length,
                },
                eye_height,
            },
        }
    }

    /// Source's player: 0.81 m wide, 1.83 m tall, eye at 1.63 m.
    fn source_body() -> CharacterBody {
        body_of(HullShape::Capsule, 0.405, 1.83, 1.63)
    }

    /// Source's size with a flat base: its edges are rounded by 0.1 m.
    fn round_cylinder_body() -> CharacterBody {
        body_of(
            HullShape::RoundCylinder { rounding: 0.1 },
            0.405,
            1.83,
            1.63,
        )
    }

    /// A character of `body` settled on a floor 1.5 m before a `width` m wide, `height` m high
    /// doorway, sent through it at 4 m/s with no friction for 60 ticks (4 m).
    fn walk_at_a_doorway(
        body: CharacterBody,
        width: f32,
        height: f32,
    ) -> (Vec<Impact>, CharacterState) {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &doorway_mesh(width, height), TEST_TAG);
        let h = world.add_character(CharacterSpawn {
            position: Vec3::new(0.0, body.standing.hull.length / 2.0 + 0.2, -1.5),
            body,
            ..CharacterSpawn::from(Vec3::ZERO)
        });
        settle_on_floor(&mut world, &[h]);
        assert!(state(&world, h).grounded, "standing before the doorway");
        world
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, 0.0, 4.0)))
            .unwrap();

        let impacts = run(&mut world, h, &frictionless(), 60);

        (impacts, state(&world, h))
    }

    fn walk_at_the_doorway(body: CharacterBody) -> (Vec<Impact>, CharacterState) {
        walk_at_a_doorway(body, 1.2, 1.9)
    }

    #[test]
    fn the_default_body_walks_through_a_0_7_m_doorway_without_impact() {
        let (impacts, end) = walk_at_a_doorway(CharacterBody::default(), 0.7, 2.5);

        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(end.position.z > 1.0, "walked through: {end:?}");
    }

    #[test]
    fn a_0_81_m_wide_hull_is_blocked_by_a_0_7_m_doorway() {
        let (impacts, end) = walk_at_a_doorway(source_body(), 0.7, 2.5);

        // Wider than the opening, it meets both jambs: at most one impact each.
        assert!((1..=2).contains(&impacts.len()), "{impacts:?}");
        assert!(end.position.z < 0.0, "stopped before the wall: {end:?}");
    }

    #[test]
    fn a_round_cylinder_hull_walks_through_a_0_9_m_doorway_without_impact() {
        let (impacts, end) = walk_at_a_doorway(round_cylinder_body(), 0.9, 2.5);

        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(end.position.z > 1.0, "walked through: {end:?}");
    }

    #[test]
    fn a_1_83_m_hull_walks_through_a_1_9_m_doorway_without_impact() {
        let (impacts, end) = walk_at_the_doorway(source_body());

        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(end.position.z > 1.0, "walked through: {end:?}");
        assert!(end.grounded, "{end:?}");
    }

    #[test]
    fn a_2_3_m_hull_is_blocked_by_a_1_9_m_doorway_with_one_impact() {
        let tall = body_of(HullShape::Capsule, 0.3, 2.3, 1.15);

        let (impacts, end) = walk_at_the_doorway(tall);

        assert_eq!(impacts.len(), 1, "{impacts:?}");
        assert!(impacts[0].speed > 3.0, "{:?}", impacts[0]);
        assert!(end.position.z < 0.0, "stopped before the wall: {end:?}");
    }

    #[test]
    fn the_eye_sits_at_the_games_height_above_the_feet() {
        let cases = [
            source_body(),
            round_cylinder_body(),
            body_of(HullShape::Capsule, 0.3, 2.3, 2.0),
        ];
        for body in cases {
            let Stature { hull, eye_height } = body.standing;
            let mut world = PhysicsWorld::new();
            world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
            let h = world.add_character(CharacterSpawn {
                position: Vec3::new(0.0, hull.length / 2.0 + 0.2, 0.0),
                body,
                ..CharacterSpawn::from(Vec3::ZERO)
            });

            settle_on_floor(&mut world, &[h]);

            let rest = state(&world, h);
            assert!(rest.grounded, "{body:?}: {rest:?}");
            assert!(
                (rest.position.y + rest.eye.y - eye_height).abs() < 0.03,
                "{body:?}: the eye is {eye_height} m above the floor, got {rest:?}"
            );
            assert!(
                (rest.eye.y - (eye_height - hull.length / 2.0)).abs() < 1e-4
                    && rest.eye.x == 0.0
                    && rest.eye.z == 0.0,
                "{body:?}: eye offset from the centre, {:?}",
                rest.eye
            );
        }
    }

    #[test]
    fn a_fresh_character_has_its_eye_before_any_step() {
        let cases = [
            (CharacterBody::default(), 0.0),
            (source_body(), 1.63 - 0.915),
            (round_cylinder_body(), 1.63 - 0.915),
            (body_of(HullShape::Capsule, 0.3, 2.3, 2.0), 2.0 - 1.15),
        ];
        for (body, offset) in cases {
            let mut world = PhysicsWorld::new();

            let h = world.add_character(CharacterSpawn {
                body,
                ..CharacterSpawn::from(Vec3::new(0.0, 5.0, 0.0))
            });

            let fresh = state(&world, h);
            assert!(
                (fresh.eye.y - offset).abs() < 1e-4 && fresh.eye.x == 0.0 && fresh.eye.z == 0.0,
                "{body:?}: eye offset {offset} before any step, got {:?}",
                fresh.eye
            );
        }
    }

    #[test]
    fn two_characters_of_different_bodies_each_rest_at_their_own_height_and_eye() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let tall = world.add_character(CharacterSpawn {
            body: CharacterBody::default(),
            ..CharacterSpawn::from(Vec3::new(-3.0, 1.35, 0.0))
        });
        let short = world.add_character(CharacterSpawn {
            body: source_body(),
            ..CharacterSpawn::from(Vec3::new(3.0, 1.1, 0.0))
        });

        settle_on_floor(&mut world, &[tall, short]);

        let (tall, short) = (state(&world, tall), state(&world, short));
        assert!(tall.grounded && short.grounded, "{tall:?} {short:?}");
        assert!((tall.position.y - 1.15).abs() < 0.05, "{tall:?}");
        assert!((short.position.y - 0.915).abs() < 0.05, "{short:?}");
        assert!(tall.eye.y.abs() < 1e-4, "{tall:?}");
        assert!((short.eye.y - 0.715).abs() < 1e-4, "{short:?}");
    }

    #[test]
    fn the_default_body_is_todays_capsule_and_eye() {
        let body = CharacterBody::default();
        let (world, h) = standing_on_floor();

        assert_eq!(
            body.standing,
            Stature {
                hull: Hull {
                    shape: HullShape::Capsule,
                    radius: 0.3,
                    length: 2.3,
                },
                eye_height: 1.15,
            }
        );
        assert_eq!(CharacterSpawn::from(Vec3::ONE).body, body);
        let rest = state(&world, h);
        assert!(
            rest.eye.length() < 1e-4,
            "eye at the centre, {:?}",
            rest.eye
        );
    }

    #[test]
    fn round_cylinder_hull_walks_down_0_3_m_stairs_grounded() {
        for speed in [2.0, 4.0, 6.0] {
            for k in 0..20 {
                assert_body_descends_stairs_grounded_from(
                    round_cylinder_body(),
                    &searchlight_settings(),
                    0.3,
                    speed,
                    4.0 + k as f32 * 0.0137,
                );
            }
        }
    }

    /// A round-cylinder character dropped 0.05 m above a 0.3 m high step whose front edge is at
    /// z = 1, with its centre `past` m beyond that edge, idling for 120 ticks.
    fn idle_past_a_nosing(past: f32) -> CharacterState {
        let (mut world, h) = round_cylinder_on_a_nosing(past);

        for _ in 0..120 {
            searchlight_tick(&mut world, h, &idle());
        }

        state(&world, h)
    }

    /// The settled round-cylinder character of [`idle_past_a_nosing`], before its idle ticks.
    fn round_cylinder_on_a_nosing(past: f32) -> (PhysicsWorld, CharacterHandle) {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(-3.0, 4.0, 0.3), TEST_TAG);
        let h = resting_then_moving_as(
            &mut world,
            Vec3::new(0.0, 0.3 + 0.915 + 0.05, 1.0 + past),
            0.0,
            Vec3::ZERO,
            round_cylinder_body(),
        );
        (world, h)
    }

    #[test]
    fn round_cylinder_hull_stands_on_a_nosing_within_its_flat_base() {
        let past = 0.25;

        let end = idle_past_a_nosing(past);

        assert!(end.grounded, "{end:?}");
        assert!(
            (end.position.y - (0.3 + 0.915)).abs() < 0.03,
            "still on the step top, its flat base over the nosing: {end:?}"
        );
        assert!(
            (end.position.z - (1.0 + past)).abs() < 0.01,
            "did not roll off the nosing: {end:?}"
        );
    }

    /// Idles the round-cylinder character nominally `past` m past the nosing for 120 ticks,
    /// asserting grounded and no impact on every tick; returns the settled distance past the
    /// nosing, checked against `window`, and the end state.
    fn round_cylinder_idles_past_a_nosing(
        past: f32,
        window: std::ops::Range<f32>,
    ) -> (f32, CharacterState) {
        let (mut world, h) = round_cylinder_on_a_nosing(past);
        let settled = settled_past(&world, h, window);

        for tick in 0..120 {
            let (now, impacts) = searchlight_tick(&mut world, h, &idle());

            assert!(now.grounded, "tick {tick}: airborne, {now:?}");
            assert!(impacts.is_empty(), "tick {tick}: {impacts:?}, {now:?}");
        }

        (settled, state(&world, h))
    }

    #[test]
    fn round_cylinder_hull_rolls_off_past_its_flat_base() {
        // Its flat base reaches 0.376 m; a capsule-formula margin would be 0.286 m.
        let (_, end) = round_cylinder_idles_past_a_nosing(0.39, 0.377..0.5);

        assert!(
            (end.position.y - 0.915).abs() < 0.03,
            "rolled onto the floor, not resting on the rim at 1.17: {end:?}"
        );
    }

    #[test]
    fn round_cylinder_hull_stays_between_a_capsules_margin_and_its_flat_base() {
        let (settled, end) = round_cylinder_idles_past_a_nosing(0.36, 0.287..0.376);

        assert!(end.position.y > 1.1, "still on the step: {end:?}");
        assert!(
            (end.position.z - (1.0 + settled)).abs() < 0.01,
            "did not roll: {end:?}"
        );
    }

    fn add_with_body(body: CharacterBody) {
        let mut world = PhysicsWorld::new();
        world.add_character(CharacterSpawn {
            body,
            ..CharacterSpawn::from(Vec3::new(0.0, 5.0, 0.0))
        });
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_zero_radius() {
        add_with_body(body_of(HullShape::Capsule, 0.0, 1.83, 1.63));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_negative_radius() {
        add_with_body(body_of(HullShape::Capsule, -0.3, 1.83, 1.63));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_zero_length() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.1 },
            0.3,
            0.0,
            0.0,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_negative_length() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.1 },
            0.3,
            -1.83,
            1.0,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_an_infinite_radius() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.1 },
            f32::INFINITY,
            1.83,
            1.63,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_an_infinite_length() {
        add_with_body(body_of(HullShape::Capsule, 0.405, f32::INFINITY, 1.63));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_capsule_as_long_as_its_diameter() {
        add_with_body(body_of(HullShape::Capsule, 0.4, 0.8, 0.5));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_capsule_shorter_than_its_diameter() {
        add_with_body(body_of(HullShape::Capsule, 0.4, 0.5, 0.3));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_zero_rounding() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.0 },
            0.405,
            1.83,
            1.63,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_negative_rounding() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: -0.1 },
            0.405,
            1.83,
            1.63,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_rounding_beyond_the_radius() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.5 },
            0.405,
            1.83,
            1.63,
        ));
    }

    #[test]
    #[should_panic(expected = "hull")]
    fn an_impossible_hull_is_refused_with_a_round_cylinder_as_long_as_its_rounding_diameter() {
        add_with_body(body_of(
            HullShape::RoundCylinder { rounding: 0.3 },
            0.3,
            0.6,
            0.5,
        ));
    }

    #[test]
    fn a_hull_at_the_limits_is_accepted() {
        let at_the_limits = [
            body_of(HullShape::Capsule, 0.4, 0.81, 0.5),
            body_of(
                HullShape::RoundCylinder { rounding: 0.405 },
                0.405,
                1.83,
                1.63,
            ),
            body_of(
                HullShape::RoundCylinder { rounding: 0.001 },
                0.405,
                1.83,
                1.63,
            ),
            body_of(HullShape::RoundCylinder { rounding: 0.3 }, 0.3, 0.61, 0.5),
        ];

        for body in at_the_limits {
            add_with_body(body);
        }
    }

    /// A rectangle spanning x in [-10, 10] and z in [`z_from`, `z_to`] at height `y`, facing up.
    fn level_strip(y: f32, z_from: f32, z_to: f32) -> WorldMesh {
        quad_mesh(
            &[
                [-10.0, y, z_from],
                [10.0, y, z_from],
                [10.0, y, z_to],
                [-10.0, y, z_to],
            ],
            &[0, 2, 1, 0, 3, 2],
        )
    }

    #[test]
    fn walker_with_its_centre_over_a_ledge_edge_stays_grounded() {
        // Floor to z = 0.9, then a lip 6 cm lower to z = 1.0, then void.
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &level_strip(0.0, -10.0, 0.9), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &level_strip(-0.06, 0.9, 1.0), TEST_TAG);
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.6));
        settle_on_floor(&mut world, &[h]);
        let resting = state(&world, h);
        assert!(resting.grounded);
        // One tick that carries the centre 0.2 m past the lip's edge: the rim there hangs
        // 0.14 m above the lip, more than the controller's own 0.1 m snap, so only the ground
        // query keeps the walker standing.
        let past_the_edge = 1.2;
        let speed = (past_the_edge - resting.position.z) / DT;
        let lunge = MoveSettings {
            max_speed: speed,
            ground_accel: 2.0 * speed / DT,
            friction: 0.0,
            ..test_settings()
        };
        let carry = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        let impacts = world
            .move_character(h, &carry, &lunge, DT)
            .expect("character is live");
        world.step(DT);

        let now = state(&world, h);
        assert!(
            (now.position.z - past_the_edge).abs() < 1e-3,
            "carried over the edge: {now:?}"
        );
        assert!(now.grounded, "{now:?}");
        assert!(impacts.is_empty(), "{impacts:?}");
        // Rim resting on the lip's edge a skin's width away, 6 cm below the floor:
        // -0.06 + 0.85 + sqrt((0.3 + 0.0115)^2 - 0.2^2) = 1.029. Lowering onto the lip's plane
        // instead would end near 1.09.
        assert!(
            (1.015..1.05).contains(&now.position.y),
            "lowered until the capsule meets the lip's edge: {now:?}"
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

    /// Where a settled capsule's centre sits on level ground at y = 0.
    fn rest_height_on_floor() -> f32 {
        let (world, h) = standing_on_floor();
        state(&world, h).position.y
    }

    /// Forward for `ticks`, asserting after every tick: grounded, within 1.5 cm of the floor rest
    /// height (the controller's own contact noise on plain floor is about 3 mm), vertical speed
    /// under 1 cm/s and no impact.
    fn assert_walk_stays_grounded_and_level(
        world: &mut PhysicsWorld,
        h: CharacterHandle,
        ticks: usize,
        what: &str,
    ) {
        let rest = rest_height_on_floor();
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        for tick in 0..ticks {
            let (now, impacts) = searchlight_tick(world, h, &forward);

            assert!(now.grounded, "{what}, tick {tick}: airborne, {now:?}");
            assert!(
                (now.position.y - rest).abs() < 0.015,
                "{what}, tick {tick}: not level (rest {rest}), {now:?}"
            );
            assert!(
                now.velocity.y.abs() < 0.01,
                "{what}, tick {tick}: gained vertical speed, {now:?}"
            );
            assert!(
                impacts.is_empty(),
                "{what}, tick {tick}: {impacts:?}, {now:?}"
            );
        }
    }

    #[test]
    fn crossing_a_seam_between_two_meshes_keeps_the_walker_level() {
        for offset in 0..10 {
            let mut world = PhysicsWorld::new();
            for (id, mesh) in seam_floor_meshes(0.025).iter().enumerate() {
                world.set_world_mesh(WorldMeshId(id as u64), mesh, TEST_TAG);
            }
            let start = Vec3::new(0.0, 1.2, -1.5 + offset as f32 * 0.01);
            let h = resting_then_moving(&mut world, start, 0.0, Vec3::new(0.0, 0.0, 6.0));

            assert_walk_stays_grounded_and_level(
                &mut world,
                h,
                30,
                &format!("start offset {offset}"),
            );

            assert!(state(&world, h).position.z > 1.0, "crossed the seam");
        }
    }

    #[test]
    fn walking_along_a_seam_with_the_centre_over_the_gap_stays_grounded_and_level() {
        let mut world = PhysicsWorld::new();
        for (id, mesh) in seam_floor_meshes(0.025).iter().enumerate() {
            world.set_world_mesh(WorldMeshId(id as u64), mesh, TEST_TAG);
        }
        // Yaw pi/2 faces +X, along the seam.
        let h = resting_then_moving(
            &mut world,
            Vec3::new(-1.0, 1.2, 0.0),
            std::f32::consts::FRAC_PI_2,
            Vec3::new(6.0, 0.0, 0.0),
        );

        assert_walk_stays_grounded_and_level(&mut world, h, 20, "along the seam");

        let end = state(&world, h).position;
        assert!(end.x > 0.9, "walked about 2 m along the seam: {end:?}");
        assert!(end.z.abs() < 0.05, "stayed over the gap: {end:?}");
    }

    #[test]
    fn walking_down_stairs_diagonally_stays_grounded_without_impacts() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(
            WorldMeshId(1),
            &stairs_mesh(5, 0.3, 0.3, 2.0, 4.0),
            TEST_TAG,
        );
        // Yaw 5/4 pi heads toward -Z and -X at 45 degrees to the stairs.
        let yaw = std::f32::consts::PI * 1.25;
        let (sin, cos) = yaw.sin_cos();
        let h = resting_then_moving(
            &mut world,
            Vec3::new(3.0, 5.0 * 0.3 + 1.15 + 0.05, 6.5),
            yaw,
            Vec3::new(sin, 0.0, cos) * 6.0,
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };
        let mut reached_the_bottom = false;

        for tick in 0..200 {
            let (now, impacts) = searchlight_tick(&mut world, h, &forward);

            assert!(now.grounded, "tick {tick}: airborne, {now:?}");
            assert!(impacts.is_empty(), "tick {tick}: {impacts:?}, {now:?}");
            if now.position.z < 1.0 {
                reached_the_bottom = true;
                break;
            }
        }

        let end = state(&world, h);
        assert!(reached_the_bottom, "never got down: {end:?}");
        assert!(
            (end.position.y - rest_height_on_floor()).abs() < 0.1,
            "standing on the bottom floor: {end:?}"
        );
    }

    #[test]
    fn walking_off_a_ledge_within_the_step_height_steps_down_without_impact() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(-6.0, 6.0, 0.4), TEST_TAG);
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, 0.4 + 1.15 + 0.05, -3.0),
            0.0,
            Vec3::new(0.0, 0.0, 6.0),
        );
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        for tick in 0..60 {
            let (now, impacts) = searchlight_tick(&mut world, h, &forward);

            assert!(now.grounded, "tick {tick}: airborne, {now:?}");
            assert!(impacts.is_empty(), "tick {tick}: {impacts:?}, {now:?}");
        }

        let end = state(&world, h);
        assert!(end.position.z > 2.0, "walked well past the edge: {end:?}");
        assert!(
            (end.position.y - rest_height_on_floor()).abs() < 0.02,
            "stepped down onto the floor: {end:?}"
        );
    }

    /// A 0.3 m step over a floor, nosing at z = 1, with a settled walker whose centre is `past` m
    /// beyond the nosing, placed with its rim just above the nosing (0.85 + sqrt((0.3 + 0.0115)^2
    /// - past^2) above the step top).
    fn walker_on_a_nosing(past: f32) -> (PhysicsWorld, CharacterHandle) {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(-3.0, 4.0, 0.3), TEST_TAG);
        let on_the_nosing_y = 0.3 + 0.85 + (0.3115_f32 * 0.3115 - past * past).sqrt();
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, on_the_nosing_y + 0.003, 1.0 + past),
            0.0,
            Vec3::ZERO,
        );
        (world, h)
    }

    /// [`walker_on_a_nosing`] after 120 idle ticks.
    fn idle_on_a_nosing(past: f32) -> CharacterState {
        let (mut world, h) = walker_on_a_nosing(past);

        for _ in 0..120 {
            searchlight_tick(&mut world, h, &idle());
        }
        state(&world, h)
    }

    #[test]
    fn an_idle_walker_within_the_flat_base_stays_on_a_nosing() {
        let end = idle_on_a_nosing(0.1);

        assert!(end.grounded, "{end:?}");
        assert!(
            (end.position.y - (0.3 + 1.15)).abs() < 0.03,
            "still on the step top: {end:?}"
        );
    }

    #[test]
    fn an_idle_walker_past_the_flat_base_rolls_off_a_nosing() {
        for past in [0.22, 0.25, 0.28] {
            let (mut world, h) = walker_on_a_nosing(past);

            for tick in 0..120 {
                let (now, impacts) = searchlight_tick(&mut world, h, &idle());

                assert!(
                    now.grounded,
                    "{past} m past, tick {tick}: airborne, {now:?}"
                );
                assert!(
                    impacts.is_empty(),
                    "{past} m past, tick {tick}: {impacts:?}, {now:?}"
                );
            }

            let end = state(&world, h);
            assert!(
                (end.position.y - rest_height_on_floor()).abs() < 0.03,
                "{past} m past: stands on the lower floor: {end:?}"
            );
            assert!(
                (1.31..1.45).contains(&end.position.z),
                "{past} m past: rolled off just clear of the step's foot: {end:?}"
            );
            assert!(
                end.velocity.with_y(0.0).length() < 0.01,
                "{past} m past: the top-up moves the position, not the velocity: {end:?}"
            );
        }
    }

    /// The settled walker's distance past the nosing at z = 1, after checking it is in `window`.
    fn settled_past(world: &PhysicsWorld, h: CharacterHandle, window: std::ops::Range<f32>) -> f32 {
        let past = state(world, h).position.z - 1.0;
        assert!(
            window.contains(&past),
            "settled {past} m past, not in {window:?}"
        );
        past
    }

    /// The perch margin is the flat base radius, 0.3 * sin 45 = 0.2121 m. The outside case sits
    /// between it and 0.3115 * sin 45 = 0.2203 m, where a rule keyed to the nosing's normal being
    /// non-walkable would start rolling, so only the flat base separates the two. Settling
    /// drifts a walker about 4.5 mm outward, hence the nominal placements.
    #[test]
    fn the_perch_margin_is_the_flat_base_radius() {
        let (mut inside_world, inside_h) = walker_on_a_nosing(0.200);
        let (mut outside_world, outside_h) = walker_on_a_nosing(0.2115);
        let inside_past = settled_past(&inside_world, inside_h, 0.19..0.212);
        let outside_past = settled_past(&outside_world, outside_h, 0.212..0.2203);

        for _ in 0..120 {
            searchlight_tick(&mut inside_world, inside_h, &idle());
            searchlight_tick(&mut outside_world, outside_h, &idle());
        }

        let inside = state(&inside_world, inside_h);
        let outside = state(&outside_world, outside_h);
        assert!(
            inside.position.y > 0.3 + 1.0 && (inside.position.z - (1.0 + inside_past)).abs() < 0.01,
            "inside the flat base: still perched on the nosing: {inside:?}"
        );
        assert!(
            (outside.position.y - rest_height_on_floor()).abs() < 0.03,
            "{outside_past} m past, outside the flat base: rolled onto the floor: {outside:?}"
        );
    }

    #[test]
    fn walking_down_stairs_slowly_stays_grounded_without_impacts() {
        for rise in [0.2, 0.3, 0.45] {
            for speed in [0.25, 0.5, 1.0, 1.5] {
                // Slow walkers take many ticks, so fewer phases; the faster runs take 10.
                let phases = if speed < 1.0 { 2 } else { 10 };
                let settings = MoveSettings {
                    max_speed: speed,
                    ..searchlight_settings()
                };
                for k in 0..phases {
                    assert_body_descends_stairs_grounded_from(
                        CharacterBody::default(),
                        &settings,
                        rise,
                        speed,
                        3.4 + k as f32 * 0.0137 * (20 / phases) as f32,
                    );
                }
            }
        }
    }

    #[test]
    fn a_walker_against_a_steep_face_is_not_pushed() {
        let rise = 60.0_f32.to_radians().tan();
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        // A 60 degree face rising from the floor at z = 1; the capsule's rounded base meets it
        // at z = 0.820, counting the skin offset.
        world.set_world_mesh(
            WorldMeshId(1),
            &slope_mesh(1.0, 0.0, 3.0, 2.0 * rise),
            TEST_TAG,
        );
        let h = resting_then_moving(
            &mut world,
            Vec3::new(0.0, rest_height_on_floor(), 0.84),
            0.0,
            Vec3::ZERO,
        );
        let settled = state(&world, h).position;

        for tick in 0..120 {
            let (now, impacts) = searchlight_tick(&mut world, h, &idle());

            assert!(now.grounded, "tick {tick}: airborne, {now:?}");
            assert!(impacts.is_empty(), "tick {tick}: {impacts:?}, {now:?}");
            assert!(
                (now.position - settled).length() < 0.002,
                "tick {tick}: pushed off the settled {settled:?}: {now:?}"
            );
        }
    }

    #[test]
    fn walking_into_a_step_at_45_degrees_climbs_it() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, 0.45), TEST_TAG);
        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: std::f32::consts::FRAC_PI_4,
                    pitch: 0.0,
                },
            )
            .unwrap();
        let forward = CharacterCommand {
            forward: 1.0,
            ..idle()
        };

        let start = state(&world, h).position;

        let impacts = hold(&mut world, h, &forward, 120);

        let after = state(&world, h);
        assert!(impacts.is_empty(), "{impacts:?}");
        assert!(
            (after.position.y - (0.45 + 1.15)).abs() < 0.1,
            "{:?}",
            after.position
        );
        assert!(after.grounded, "{after:?}");
        let travelled = after.position - start;
        assert!(
            travelled.x > 0.5 && (travelled.x - travelled.z).abs() < 0.5,
            "kept its 45 degree heading: {travelled:?}"
        );
    }

    /// Stands the character against the riser of a step of `height`, jumps with forward
    /// held, and returns the state before and after that one tick.
    fn jump_pressed_against_step(height: f32) -> (CharacterState, CharacterState) {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 6.0, height), TEST_TAG);
        let rest_y = state(&world, h).position.y;
        teleport(&mut world, h, Vec3::new(0.0, rest_y, 2.0 - 0.3 - 0.03));
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, -1.0, 3.0)))
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
            .queue_effect(h, CharacterEffect::AddVelocity(Vec3::new(0.0, -12.0, 0.0)))
            .unwrap();
        world.step(DT);
        let no_gravity = MoveSettings {
            gravity: Some(0.0),
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
            rotation: Quat::IDENTITY,
            half_extents,
            tag,
        }
    }

    fn set_volumes(world: &mut PhysicsWorld, handle: CharacterHandle, volumes: &[HitVolume]) {
        world
            .set_hit_volumes(handle, volumes)
            .expect("character is live");
    }

    fn set_one_volume(world: &mut PhysicsWorld, handle: CharacterHandle, volume: HitVolume) {
        set_volumes(world, handle, &[volume]);
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
        set_volumes(
            world,
            handle,
            &[
                volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD),
                volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
            ],
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
    fn ray_through_a_gap_between_volumes_hits_the_wall() {
        let (mut world, h) = standing_on_floor();
        world.set_world_mesh(WorldMeshId(1), &wall_mesh(3.0), TEST_TAG);
        set_volumes(
            &mut world,
            h,
            &[
                volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
                volume(Vec3::new(0.7, 0.1, 0.0), Vec3::new(0.1, 0.4, 0.15), ARM),
            ],
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
        set_volumes(
            &mut world,
            shooter,
            &[volume(
                Vec3::new(0.0, 0.7, 0.0),
                Vec3::new(0.2, 0.3, 0.2),
                HEAD,
            )],
        );
        set_volumes(
            &mut world,
            target,
            &[
                volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO),
                volume(Vec3::new(0.0, 0.95, 0.0), Vec3::new(0.2, 0.2, 0.2), ARM),
            ],
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
        set_volumes(
            &mut world,
            walker,
            &[volume(Vec3::new(0.0, 0.0, 0.8), reach, ARM)],
        );
        set_volumes(
            &mut world,
            standing,
            &[volume(Vec3::new(0.0, 0.0, -0.8), reach, ARM)],
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
    fn replaced_volumes_are_hit_where_the_game_put_them() {
        let (mut world, h) = standing_on_floor();
        let size = Vec3::new(0.2, 0.15, 0.2);
        set_volumes(
            &mut world,
            h,
            &[volume(Vec3::new(0.0, 0.7, 0.0), size, HEAD)],
        );
        world.step(DT);
        let at = state(&world, h).position;

        set_volumes(
            &mut world,
            h,
            &[volume(Vec3::new(0.35, 0.7, 0.0), size, HEAD)],
        );
        world.step(DT);
        let new_head = cast(&world, at + Vec3::new(0.35, 0.7, -3.0), Vec3::Z, None);
        let old_head = cast(&world, at + Vec3::new(0.0, 0.7, -3.0), Vec3::Z, None);

        assert_eq!(new_head.map(|hit| hit.tag), Some(HEAD), "at {at:?}");
        assert_eq!(old_head, None, "nothing is left where the head was");
    }

    #[test]
    fn a_rotated_volume_lies_along_its_rotation() {
        let (mut world, h) = standing_on_floor();
        let centre = Vec3::new(0.0, 0.1, 0.0);
        let mut torso = volume(centre, Vec3::new(0.25, 0.4, 0.15), TORSO);
        torso.rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        set_volumes(&mut world, h, &[torso]);
        world.step(DT);
        let at = state(&world, h).position;
        let below = Vec3::new(0.0, -0.8, 0.0);

        let through_centre = cast(&world, at + below, Vec3::Y, None);
        let along_long_axis = cast(&world, at + below + Vec3::new(0.0, 0.0, 0.3), Vec3::Y, None);
        let past_long_axis = cast(&world, at + below + Vec3::new(0.0, 0.0, 0.5), Vec3::Y, None);

        let through_centre = through_centre.expect("a ray through the centre hits the torso");
        assert_eq!(through_centre.tag, TORSO);
        assert!(
            (through_centre.point.y - (at.y + centre.y - 0.15)).abs() < 0.02,
            "the 0.4 half-extent lies along Z, so the box is 0.15 deep in Y: {through_centre:?}"
        );
        assert_eq!(
            along_long_axis.map(|hit| hit.tag),
            Some(TORSO),
            "0.3 m along Z is inside the rotated long axis"
        );
        assert_eq!(past_long_axis, None, "0.5 m along Z is beyond it");
    }

    #[test]
    fn volumes_turn_with_the_body() {
        let (mut world, h) = standing_on_floor();
        set_volumes(
            &mut world,
            h,
            &[volume(
                Vec3::new(0.4, 0.0, 0.0),
                Vec3::new(0.1, 0.3, 0.1),
                ARM,
            )],
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
    fn replacing_with_fewer_volumes_removes_the_rest() {
        let (mut world, h) = standing_on_floor();
        let head = volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD);
        let torso = volume(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.25, 0.4, 0.15), TORSO);
        let colliders = world.collider_set.len();
        set_volumes(&mut world, h, &[head, torso]);
        world.step(DT);
        let at = state(&world, h).position;
        let head_ray = |world: &PhysicsWorld| {
            cast(world, at + Vec3::new(-3.0, 0.7, 0.0), Vec3::X, None).map(|hit| hit.tag)
        };
        let torso_ray = |world: &PhysicsWorld| {
            cast(world, at + Vec3::new(-3.0, 0.1, 0.0), Vec3::X, None).map(|hit| hit.tag)
        };
        assert_eq!(head_ray(&world), Some(HEAD));
        assert_eq!(world.collider_set.len(), colliders + 2);

        set_volumes(&mut world, h, &[torso]);
        world.step(DT);

        assert_eq!(head_ray(&world), None, "the head is gone");
        assert_eq!(torso_ray(&world), Some(TORSO), "the torso stays");
        assert_eq!(world.collider_set.len(), colliders + 1);
    }

    #[test]
    fn reposing_the_same_number_of_volumes_keeps_their_colliders() {
        let (mut world, h) = standing_on_floor();
        let head = volume(Vec3::new(0.0, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD);
        set_volumes(&mut world, h, &[head]);
        let before = world.characters[&h].volumes.clone();

        let leaned = volume(Vec3::new(0.35, 0.7, 0.0), Vec3::new(0.2, 0.15, 0.2), HEAD);
        set_volumes(&mut world, h, &[leaned]);

        assert_eq!(world.characters[&h].volumes, before);
    }

    #[test]
    fn setting_volumes_on_a_removed_character_is_refused() {
        let mut world = PhysicsWorld::new();
        let h = world.add_character(Vec3::new(0.0, 1.2, 0.0));
        world.remove_character(h).unwrap();
        let colliders = world.collider_set.len();

        let set = world.set_hit_volumes(h, &[volume(Vec3::ZERO, Vec3::splat(0.2), HEAD)]);

        assert_eq!(set, Err(UnknownCharacter));
        assert_eq!(world.collider_set.len(), colliders);
    }

    #[test]
    fn a_characters_own_volumes_do_not_block_its_movement() {
        let (mut world, h) = standing_on_floor();
        let start = state(&world, h).position;
        set_volumes(
            &mut world,
            h,
            &[volume(
                Vec3::new(0.0, 0.0, 0.52),
                Vec3::new(0.2, 0.3, 0.2),
                ARM,
            )],
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
        set_volumes(
            &mut world,
            h,
            &[volume(
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(1.0, 0.5, 1.0),
                TORSO,
            )],
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
        set_one_volume(
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
    fn spawn_look_turns_the_volumes_before_any_tick() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &floor_mesh(0.0), TEST_TAG);
        let h = world.add_character(CharacterSpawn {
            position: Vec3::new(0.0, 1.2, 0.0),
            yaw: std::f32::consts::PI,
            pitch: 0.0,
            body: CharacterBody::default(),
        });
        set_one_volume(
            &mut world,
            h,
            volume(Vec3::new(0.4, 0.0, 0.0), Vec3::new(0.1, 0.3, 0.1), ARM),
        );
        world.step(DT);

        let face = arm_face_from_minus_x(&world, h).expect("the arm turned to -X");
        assert!((face + 0.5).abs() < 0.02, "face at {face}");
    }

    #[test]
    fn teleporting_keeps_the_volumes_facing() {
        let (mut world, h) = arm_on_plus_x();
        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: std::f32::consts::PI,
                    pitch: 0.0,
                },
            )
            .unwrap();
        teleport(&mut world, h, Vec3::new(2.0, 1.2, 0.0));

        move_idle(&mut world, h);
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
    fn set_look_then_a_command_turn_in_the_same_tick_compose() {
        let (mut world, h) = standing_on_floor();
        set_one_volume(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.0, 0.4), Vec3::new(0.1, 0.3, 0.1), ARM),
        );
        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: 1.0,
                    pitch: 0.0,
                },
            )
            .unwrap();
        let turn = CharacterCommand {
            yaw_delta: 0.25,
            ..idle()
        };

        world
            .move_character(h, &turn, &test_settings(), DT)
            .unwrap();
        world.step(DT);

        let after = state(&world, h);
        assert!((after.yaw - 1.25).abs() < 1e-6, "{after:?}");
        let facing = Vec3::new(1.25_f32.sin(), 0.0, 1.25_f32.cos());
        let hit = cast(&world, after.position + facing * 3.0, -facing, None)
            .expect("the forward arm swung to yaw 1.25");
        assert_eq!(hit.tag, ARM);
        let reach = (hit.point - after.position).dot(facing);
        assert!((reach - 0.5).abs() < 0.02, "arm face at {reach}: {hit:?}");
    }

    #[test]
    fn a_quarter_turn_swings_a_forward_volume_toward_plus_x() {
        let (mut world, h) = standing_on_floor();
        set_one_volume(
            &mut world,
            h,
            volume(Vec3::new(0.0, 0.0, 0.4), Vec3::new(0.1, 0.3, 0.1), ARM),
        );

        world
            .queue_effect(
                h,
                CharacterEffect::SetLook {
                    yaw: std::f32::consts::FRAC_PI_2,
                    pitch: 0.0,
                },
            )
            .unwrap();
        move_idle(&mut world, h);
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
            test_settings().gravity.expect("test settings set gravity")
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
            let h = world.add_character(CharacterSpawn {
                position: at,
                yaw,
                pitch: 0.0,
                body: CharacterBody::default(),
            });
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
