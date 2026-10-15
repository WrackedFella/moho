//! The fixed-tick simulation: player, rifle, dummies and the feedback they produce.
//! It holds no renderer, audio or UI handle.

use std::sync::Arc;

use glam::{Quat, Vec3};
use moho_physics::{
    CharacterCommand, CharacterHandle, CharacterState, HitVolume, MoveSettings, PhysicsWorld,
    first_person_view,
};
use searchlight_rules::{
    Body, BodyPart, BodyProfile, HitReport, Magazine, MagazineLevel, Rifle, Shot,
};

use crate::arena::{Arena, SETTLE_DT};
use crate::brain::{Brain, EnemyTuning, Percept};
use crate::cover::{self, Threat};
use crate::input::FpsCommand;
use crate::shot::{ShotRay, Strike, resolve_shot};
use crate::tag::SurfaceTag;

/// Height of a character's capsule centre above its feet, in metres.
pub const STAND_HEIGHT: f32 = 1.15;

/// The target number and capsule centre of the dummy at `index` in a spawn list, for a dummy
/// standing on the floor point `spawn`.
///
/// # Panics
/// If `index` is more than a `u16` target number can name.
pub(crate) fn dummy_at(index: usize, spawn: Vec3) -> (u16, Vec3) {
    let target = u16::try_from(index).expect("an arena has fewer than 65536 dummies");
    (target, spawn + Vec3::Y * STAND_HEIGHT)
}

/// The index of the waypoint nearest `spawn` on the ground plane, and the yaw that faces it.
///
/// # Panics
/// If `route` is empty.
fn nearest_waypoint(spawn: Vec3, route: &[Vec3]) -> (usize, f32) {
    let flat = |waypoint: Vec3| (waypoint.x - spawn.x).hypot(waypoint.z - spawn.z);
    let (index, waypoint) = route
        .iter()
        .copied()
        .enumerate()
        .min_by(|a, b| flat(a.1).total_cmp(&flat(b.1)))
        .expect("an arena with an enemy spawn has a patrol route");
    (index, (waypoint.x - spawn.x).atan2(waypoint.z - spawn.z))
}

/// Tuning the simulation is built with.
#[derive(Clone, Debug)]
pub struct SimConfig {
    pub move_settings: MoveSettings,
    /// Height of the eye above the player's capsule centre, in metres.
    pub eye_height: f32,
    /// How far a shot reaches, in metres.
    pub range: f32,
    /// Damage of one hit.
    pub damage: u32,
    /// Ticks between one shot and the next allowed pull: shots on ticks `t` and
    /// `t + cooldown_ticks` are both allowed, one on `t + cooldown_ticks - 1` is not.
    pub cooldown_ticks: u32,
    /// Ticks a reload takes: pressed on tick `t`, it completes on tick `t + reload_ticks`,
    /// where the new magazine is in and the rifle can fire again.
    pub reload_ticks: u32,
    /// Pools of the dummies', enemies' and the player's bodies.
    pub body: BodyProfile,
    pub rifle: Rifle,
    /// Magazines the player can reload from, used first to last.
    pub spares: Vec<Magazine>,
    /// How every enemy notices, aims and fires. `damage` and `cooldown_ticks` here are the
    /// enemies'; the fields above are the player's.
    pub enemy: EnemyTuning,
    /// The player's part the enemies aim at.
    pub enemy_aim_at: BodyPart,
}

/// Something the player should see, hear or read, in the order it happened.
#[derive(Clone, Debug, PartialEq)]
pub enum Feedback {
    /// A round was spent.
    Fired,
    /// The trigger was pulled on an empty chamber.
    Dry,
    /// A shot struck a part of a dummy.
    Hit {
        target: u16,
        part: BodyPart,
        report: HitReport,
    },
    /// The player checked the magazine.
    MagazineLevel(MagazineLevel),
    /// A reload finished.
    ReloadDone,
    /// The enemy numbered `target` fired a shot.
    EnemyFired { target: u16 },
    /// An enemy shot struck `part` of the player; `from` is where the shot began.
    PlayerHit { part: BodyPart, from: Vec3 },
    /// The enemy numbered `target` died and left the arena.
    EnemyDied { target: u16 },
    /// The player died.
    PlayerDied,
}

/// The six hit volumes of the player, the same boxes as a dummy's but tagged
/// [`SurfaceTag::Player`].
pub fn player_hit_volumes() -> [HitVolume; 6] {
    body_volumes(SurfaceTag::Player)
}

/// The six hit volumes of a dummy numbered `target`, one per body part, in the body frame
/// around the capsule centre. Each volume's tag is `SurfaceTag::Part` for that part.
///
/// The body faces +Z at yaw 0, so its left side is +X. The head sits directly on the torso
/// and the legs under it, so a level ray from the player's eye height meets a part rather than
/// passing through a gap at the neck.
pub fn dummy_hit_volumes(target: u16) -> [HitVolume; 6] {
    body_volumes(|part| SurfaceTag::Part { target, part })
}

/// The six hit volumes every body shares, each tagged by `tag_of` for its part.
fn body_volumes(tag_of: impl Fn(BodyPart) -> SurfaceTag) -> [HitVolume; 6] {
    let volume = |part, offset: Vec3, half_extents: Vec3| HitVolume {
        offset,
        half_extents,
        tag: tag_of(part).into(),
    };
    let arm = Vec3::new(0.1, 0.3, 0.1);
    let leg = Vec3::new(0.1, 0.525, 0.1);
    [
        volume(
            BodyPart::Head,
            Vec3::new(0.0, 0.72, 0.0),
            Vec3::new(0.12, 0.12, 0.12),
        ),
        volume(
            BodyPart::Torso,
            Vec3::new(0.0, 0.25, 0.0),
            Vec3::new(0.2, 0.35, 0.12),
        ),
        volume(BodyPart::LeftArm, Vec3::new(0.3, 0.25, 0.0), arm),
        volume(BodyPart::RightArm, Vec3::new(-0.3, 0.25, 0.0), arm),
        volume(BodyPart::LeftLeg, Vec3::new(0.12, -0.625, 0.0), leg),
        volume(BodyPart::RightLeg, Vec3::new(-0.12, -0.625, 0.0), leg),
    ]
}

/// What the rifle is doing; one thing at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weapon {
    Ready,
    /// A shot was just fired; ready again when the count of ticks left runs out.
    Cooling(u32),
    /// A magazine swap is under way; done when the count of ticks left runs out.
    Reloading(u32),
}

/// A dummy or an enemy: a character with hit volumes and a body. Enemies also think.
#[derive(Debug)]
struct Target {
    /// `None` once an enemy died and left the world.
    handle: Option<CharacterHandle>,
    body: Body,
    brain: Option<Brain>,
    /// Where the player stood when it wounded this enemy; read by the next percept step.
    hurt: Option<Vec3>,
}

/// SplitMix64: a small seeded generator, so enemy spread repeats for a seed.
#[derive(Debug)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[-1, 1)`.
    fn next_signed(&mut self) -> f32 {
        let unit = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        unit * 2.0 - 1.0
    }
}

/// The world, the player, the rifle, the dummies and the enemies.
#[derive(Debug)]
pub struct FpsSim {
    world: PhysicsWorld,
    player: CharacterHandle,
    move_settings: MoveSettings,
    eye_height: f32,
    range: f32,
    damage: u32,
    cooldown_ticks: u32,
    reload_ticks: u32,
    rifle: Rifle,
    spares: Vec<Magazine>,
    /// Indexed by target number: the dummies, then the enemies.
    targets: Vec<Target>,
    dummy_count: usize,
    player_body: Body,
    player_max_health: u32,
    route: Vec<Vec3>,
    covers: Vec<Vec3>,
    enemy: EnemyTuning,
    /// Offset from the player's capsule centre to the part the enemies aim at, before the
    /// player's yaw is applied.
    aim_offset: Vec3,
    rng: SplitMix64,
    weapon: Weapon,
    outbox: Vec<Feedback>,
    /// What a restart rebuilds the characters from, in the same world.
    origin: (Spawns, SimConfig),
}

/// Where a sim's characters start.
#[derive(Clone, Debug)]
struct Spawns {
    player: Vec3,
    dummies: Vec<Vec3>,
    enemies: Vec<Vec3>,
    route: Vec<Vec3>,
    covers: Vec<Vec3>,
}

impl FpsSim {
    /// A sim over `world` (static geometry already in), the player standing at
    /// `player_spawn` and one dummy at each of `dummy_spawns`, numbered by position in the
    /// slice. Spawns are floor points: capsule centres sit [`STAND_HEIGHT`] above them.
    ///
    /// # Panics
    /// If `dummy_spawns` holds more dummies than a `u16` target number can name.
    pub fn new(
        world: PhysicsWorld,
        player_spawn: Vec3,
        dummy_spawns: &[Vec3],
        config: SimConfig,
    ) -> Self {
        let spawns = Spawns {
            player: player_spawn,
            dummies: dummy_spawns.to_vec(),
            enemies: Vec::new(),
            route: Vec::new(),
            covers: Vec::new(),
        };
        Self::assemble(world, spawns, config)
    }

    fn assemble(mut world: PhysicsWorld, spawns: Spawns, config: SimConfig) -> Self {
        let player = world.add_character(spawns.player + Vec3::Y * STAND_HEIGHT);
        let player_volumes = player_hit_volumes();
        for volume in player_volumes {
            world
                .add_hit_volume(player, volume)
                .expect("the player was added a moment ago");
        }
        let aim_offset = player_volumes
            .iter()
            .find(|volume| volume.tag == SurfaceTag::Player(config.enemy_aim_at).into())
            .expect("the player has a hit volume for every part")
            .offset;
        let mut targets = Vec::new();
        for (index, &spawn) in spawns.dummies.iter().chain(&spawns.enemies).enumerate() {
            let (target, centre) = dummy_at(index, spawn);
            let handle = world.add_character(centre);
            for volume in dummy_hit_volumes(target) {
                world
                    .add_hit_volume(handle, volume)
                    .expect("the target was added a moment ago");
            }
            let brain = (index >= spawns.dummies.len()).then(|| {
                let (nearest, yaw) = nearest_waypoint(spawn, &spawns.route);
                world
                    .set_character_look(handle, yaw, 0.0)
                    .expect("the enemy was added a moment ago");
                Brain::Patrol { next: nearest }
            });
            targets.push(Target {
                handle: Some(handle),
                body: Body::new(config.body),
                brain,
                hurt: None,
            });
        }
        // Index the new hit volumes so the first tick's rays can strike them.
        world.step(SETTLE_DT);
        Self {
            world,
            player,
            move_settings: config.move_settings,
            eye_height: config.eye_height,
            range: config.range,
            damage: config.damage,
            cooldown_ticks: config.cooldown_ticks,
            reload_ticks: config.reload_ticks,
            rifle: config.rifle.clone(),
            spares: config.spares.clone(),
            targets,
            dummy_count: spawns.dummies.len(),
            player_body: Body::new(config.body),
            player_max_health: config.body.max_health,
            route: spawns.route.clone(),
            covers: spawns.covers.clone(),
            enemy: config.enemy,
            aim_offset,
            rng: SplitMix64(config.enemy.seed),
            weapon: Weapon::Ready,
            outbox: Vec::new(),
            origin: (spawns, config),
        }
    }

    /// Runs one tick of `dt` seconds.
    ///
    /// While the player is dead only `command.restart` is read, and it puts every character
    /// back at its spawn, as a fresh sim over the same world would have them. Otherwise the enemies perceive and move, the player moves, the
    /// world steps, the player's weapon actions apply, then the enemies that decided to fire
    /// do, and enemies that died leave the world.
    pub fn tick(&mut self, command: &FpsCommand, dt: f32) {
        if self.player_body.is_dead() {
            if command.restart {
                self.restart();
            }
            return;
        }
        let firing = self.move_enemies(dt);
        self.world
            .move_character(self.player, &command.movement, &self.move_settings, dt)
            .expect("the sim never removes the player");
        self.world.step(dt);

        self.advance_weapon();
        if command.fire {
            self.pull_trigger();
        }
        if command.reload {
            self.begin_reload();
        }
        if command.check {
            self.outbox
                .push(Feedback::MagazineLevel(self.rifle.magazine().level()));
        }

        self.fire_enemies(&firing);
        self.remove_dead_enemies();
        if self.player_body.is_dead() {
            self.outbox.push(Feedback::PlayerDied);
        }
    }

    /// Puts every character back at its spawn in the same world, as a fresh sim would be.
    fn restart(&mut self) {
        let mut world = std::mem::replace(&mut self.world, PhysicsWorld::new());
        let characters = self
            .targets
            .iter()
            .filter_map(|target| target.handle)
            .chain([self.player]);
        for handle in characters {
            world
                .remove_character(handle)
                .expect("the sim removes each of its characters once");
        }
        let (spawns, config) = self.origin.clone();
        *self = Self::assemble(world, spawns, config);
    }

    /// Where the enemies aim: the chosen part of the player's body as it stands now.
    fn player_aim_point(&self) -> Vec3 {
        let state = self.player();
        state.position + Quat::from_rotation_y(state.yaw) * self.aim_offset
    }

    /// Lets every living enemy look for the player, think and move; returns the indices of
    /// those that want to fire.
    fn move_enemies(&mut self, dt: f32) -> Vec<usize> {
        let aim = self.player_aim_point();
        let threat = Threat {
            eye: self.player().position + Vec3::Y * self.eye_height,
            aim,
        };
        let mut claims: Vec<Option<usize>> = self
            .targets
            .iter()
            .map(|target| {
                target
                    .handle
                    .and(target.brain)
                    .and_then(|brain| brain.claim())
            })
            .collect();
        let mut firing = Vec::new();
        for (index, target) in self.targets.iter_mut().enumerate().skip(self.dummy_count) {
            let (Some(handle), Some(brain)) = (target.handle, target.brain.as_mut()) else {
                continue;
            };
            let state = self
                .world
                .character(handle)
                .expect("a living enemy is in the world");
            let eye = state.position + Vec3::Y * self.eye_height;
            let sees_player = resolve_shot(
                &self.world,
                ShotRay {
                    origin: eye,
                    direction: aim - eye,
                    range: self.enemy.sight_range,
                    ignore: Some(handle),
                },
            )
            .is_some_and(|strike| matches!(strike.tag, SurfaceTag::Player(_)));
            let hurt = target.hurt.take();
            let offer = hurt.and_then(|_| {
                let taken: Vec<usize> = claims
                    .iter()
                    .enumerate()
                    .filter(|&(other, _)| other != index)
                    .filter_map(|(_, claim)| *claim)
                    .collect();
                cover::offer(
                    &self.world,
                    &self.covers,
                    &taken,
                    state.position,
                    threat,
                    STAND_HEIGHT + self.eye_height,
                    &self.enemy,
                )
            });
            let percept = Percept {
                position: state.position,
                yaw: state.yaw,
                target: sees_player.then_some(aim),
                hurt,
                offer,
            };
            let intent = brain.think(&self.route, &percept, &self.enemy, dt);
            claims[index] = brain.claim();
            let command = CharacterCommand {
                forward: intent.forward,
                yaw_delta: intent.yaw_delta,
                ..CharacterCommand::default()
            };
            self.world
                .move_character(handle, &command, &self.move_settings, dt)
                .expect("a living enemy is in the world");
            if intent.fire {
                firing.push(index);
            }
        }
        firing
    }

    /// Fires the enemies in `firing`, in order, skipping any that died this tick and all of
    /// them once the player is dead.
    fn fire_enemies(&mut self, firing: &[usize]) {
        for &index in firing {
            if self.player_body.is_dead() {
                break;
            }
            let target = &self.targets[index];
            let Some(handle) = target.handle.filter(|_| !target.body.is_dead()) else {
                continue;
            };
            let state = self
                .world
                .character(handle)
                .expect("a living enemy is in the world");
            let eye = state.position + Vec3::Y * self.eye_height;
            let direction = self.with_spread(self.player_aim_point() - eye);
            self.outbox.push(Feedback::EnemyFired {
                target: u16::try_from(index).expect("target numbers fit a u16"),
            });
            let strike = resolve_shot(
                &self.world,
                ShotRay {
                    origin: eye,
                    direction,
                    range: self.enemy.sight_range,
                    ignore: Some(handle),
                },
            );
            if let Some(Strike {
                tag: SurfaceTag::Player(part),
                ..
            }) = strike
            {
                self.player_body.hit(part, self.enemy.damage);
                self.outbox.push(Feedback::PlayerHit { part, from: eye });
            }
        }
    }

    /// `direction` turned off by up to `aim_error` radians either way across and up.
    fn with_spread(&mut self, direction: Vec3) -> Vec3 {
        let forward = direction.normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let across = self.rng.next_signed() * self.enemy.aim_error;
        let rise = self.rng.next_signed() * self.enemy.aim_error;
        forward + right * across + up * rise
    }

    /// Takes enemies whose body died out of the world, so shots pass through.
    fn remove_dead_enemies(&mut self) {
        for (index, target) in self.targets.iter_mut().enumerate().skip(self.dummy_count) {
            if !target.body.is_dead() {
                continue;
            }
            if let Some(handle) = target.handle.take() {
                self.world
                    .remove_character(handle)
                    .expect("a living enemy is in the world");
                self.outbox.push(Feedback::EnemyDied {
                    target: u16::try_from(index).expect("target numbers fit a u16"),
                });
            }
        }
    }

    fn advance_weapon(&mut self) {
        self.weapon = match self.weapon {
            Weapon::Ready => Weapon::Ready,
            Weapon::Cooling(left) if left > 1 => Weapon::Cooling(left - 1),
            Weapon::Cooling(_) => Weapon::Ready,
            Weapon::Reloading(left) if left > 1 => Weapon::Reloading(left - 1),
            Weapon::Reloading(_) => {
                self.finish_reload();
                Weapon::Ready
            }
        };
    }

    fn pull_trigger(&mut self) {
        if self.weapon != Weapon::Ready {
            return;
        }
        match self.rifle.fire() {
            Shot::Dry => self.outbox.push(Feedback::Dry),
            Shot::Fired => {
                self.outbox.push(Feedback::Fired);
                if self.cooldown_ticks > 0 {
                    self.weapon = Weapon::Cooling(self.cooldown_ticks);
                }
                self.resolve_player_shot();
            }
        }
    }

    fn resolve_player_shot(&mut self) {
        let state = self.player();
        let view = first_person_view(&state, &state, self.eye_height, 1.0);
        let strike = resolve_shot(
            &self.world,
            ShotRay {
                origin: view.eye,
                direction: view.forward,
                range: self.range,
                ignore: Some(self.player),
            },
        );
        let Some(Strike {
            tag: SurfaceTag::Part { target, part },
            ..
        }) = strike
        else {
            return;
        };
        let Some(struck) = self.targets.get_mut(usize::from(target)) else {
            tracing::warn!(target, "shot struck a target with no body, dropped");
            return;
        };
        let report = struck.body.hit(part, self.damage);
        if struck.brain.is_some() && !struck.body.is_dead() {
            struck.hurt = Some(state.position);
        }
        self.outbox.push(Feedback::Hit {
            target,
            part,
            report,
        });
    }

    fn begin_reload(&mut self) {
        if matches!(self.weapon, Weapon::Reloading(_)) || self.spares.is_empty() {
            return;
        }
        if self.reload_ticks == 0 {
            self.finish_reload();
        } else {
            self.weapon = Weapon::Reloading(self.reload_ticks);
        }
    }

    /// Swaps in the first spare magazine; the ejected one goes to the back of the line.
    fn finish_reload(&mut self) {
        let spare = (!self.spares.is_empty()).then(|| self.spares.remove(0));
        if let Some(ejected) = self.rifle.reload(spare) {
            self.spares.push(ejected);
            self.outbox.push(Feedback::ReloadDone);
        }
    }

    /// The player's state after the last tick.
    pub fn player(&self) -> CharacterState {
        self.world
            .character(self.player)
            .expect("the sim never removes the player")
    }

    /// Height of the player's eye above the capsule centre, in metres.
    pub fn eye_height(&self) -> f32 {
        self.eye_height
    }

    pub fn rifle(&self) -> &Rifle {
        &self.rifle
    }

    /// Magazines left to reload from.
    pub fn spares(&self) -> &[Magazine] {
        &self.spares
    }

    /// The body of dummy `target`, or `None` if there is no such dummy.
    pub fn dummy_body(&self, target: u16) -> Option<&Body> {
        self.targets[..self.dummy_count]
            .get(usize::from(target))
            .map(|dummy| &dummy.body)
    }

    /// A sim started from `arena`: the player, one dummy per dummy spawn and one enemy per enemy
    /// spawn, numbered dummies first. Every enemy starts facing its nearest waypoint.
    ///
    /// # Panics
    /// If the arena has an enemy spawn but no route, or more targets than a `u16` can number.
    pub fn start(arena: Arc<Arena>, config: SimConfig) -> Self {
        let spawns = Spawns {
            player: arena.player_spawn,
            dummies: arena.dummy_spawns.clone(),
            enemies: arena.enemy_spawns.clone(),
            route: arena.route.clone(),
            covers: arena.covers.clone(),
        };
        Self::assemble(arena.physics_world(), spawns, config)
    }

    pub fn player_body(&self) -> &Body {
        &self.player_body
    }

    /// The most health the player's body can hold.
    pub fn player_max_health(&self) -> u32 {
        self.player_max_health
    }

    /// Enemies the arena spawned, dead ones included.
    pub fn enemy_count(&self) -> usize {
        self.targets.len() - self.dummy_count
    }

    /// The body of enemy `index` (arena order), dead or alive, or `None` if there is no such enemy.
    pub fn enemy_body(&self, index: usize) -> Option<&Body> {
        self.targets
            .get(self.dummy_count + index)
            .map(|enemy| &enemy.body)
    }

    /// Enemy `index`'s character after the last tick, or `None` if there is no such enemy or it
    /// died and was removed from the world.
    pub fn enemy_state(&self, index: usize) -> Option<CharacterState> {
        let handle = self.targets.get(self.dummy_count + index)?.handle?;
        self.world.character(handle)
    }

    /// Feedback produced and not yet drained.
    pub fn pending_feedback(&self) -> &[Feedback] {
        &self.outbox
    }

    /// Takes all pending feedback, oldest first.
    pub fn drain_feedback(&mut self) -> Vec<Feedback> {
        std::mem::take(&mut self.outbox)
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use moho_physics::CharacterCommand;
    use searchlight_rules::MagazineLevel;

    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{DT, flat_floor, mag, room, test_config};

    fn rifle_with(rounds: u32) -> Rifle {
        Rifle::loaded(mag(30, rounds))
    }

    fn idle() -> FpsCommand {
        FpsCommand::default()
    }

    fn pull() -> FpsCommand {
        FpsCommand {
            fire: true,
            ..FpsCommand::default()
        }
    }

    fn reload() -> FpsCommand {
        FpsCommand {
            reload: true,
            ..FpsCommand::default()
        }
    }

    fn check() -> FpsCommand {
        FpsCommand {
            check: true,
            ..FpsCommand::default()
        }
    }

    fn arena_sim(rifle: Rifle, spares: Vec<Magazine>) -> FpsSim {
        FpsSim::new(flat_floor(), Vec3::ZERO, &[], test_config(rifle, spares))
    }

    /// Ticks once per command in `script`, draining feedback after each tick.
    fn run(sim: &mut FpsSim, script: &[FpsCommand]) -> Vec<Vec<Feedback>> {
        script
            .iter()
            .map(|command| {
                sim.tick(command, DT);
                sim.drain_feedback()
            })
            .collect()
    }

    fn fired_on(ticks: &[Vec<Feedback>]) -> Vec<usize> {
        ticks
            .iter()
            .enumerate()
            .filter(|(_, fb)| fb.contains(&Feedback::Fired))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn player_starts_at_the_arena_spawn() {
        let spawn = Vec3::new(3.0, 0.0, -2.0);

        let sim = FpsSim::new(flat_floor(), spawn, &[], test_config(rifle_with(5), vec![]));

        let at = sim.player().position;
        assert!(
            (at.x - spawn.x).abs() < 1e-3 && (at.z - spawn.z).abs() < 1e-3,
            "{at:?}"
        );
        assert!((at.y - (spawn.y + STAND_HEIGHT)).abs() < 1e-3, "{at:?}");
    }

    #[test]
    fn player_cannot_walk_out_of_the_arena() {
        for turns in 0..4 {
            let yaw = FRAC_PI_2 * turns as f32;
            let mut sim = FpsSim::new(room(), Vec3::ZERO, &[], test_config(rifle_with(5), vec![]));
            let walk = |yaw_delta| FpsCommand {
                movement: CharacterCommand {
                    forward: 1.0,
                    yaw_delta,
                    ..CharacterCommand::default()
                },
                ..FpsCommand::default()
            };

            sim.tick(&walk(yaw), DT);
            for _ in 0..480 {
                sim.tick(&walk(0.0), DT);
            }

            let at = sim.player().position;
            assert!(
                at.x.abs() <= 10.0 && at.z.abs() <= 10.0,
                "turn {turns}: left the room at {at:?}"
            );
            let heading = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            assert!(
                at.dot(heading) > 8.0,
                "turn {turns}: never reached the wall, at {at:?}"
            );
        }
    }

    #[test]
    fn trigger_pulls_spend_rounds_and_report_dry() {
        let mut sim = arena_sim(rifle_with(3), vec![]);
        let spaced = [pull(), idle(), idle()];
        let script: Vec<FpsCommand> = spaced.iter().cycle().take(10).copied().collect();

        let ticks = run(&mut sim, &script);

        let flat: Vec<Feedback> = ticks.into_iter().flatten().collect();
        assert_eq!(
            &flat[..4],
            [
                Feedback::Fired,
                Feedback::Fired,
                Feedback::Fired,
                Feedback::Dry
            ]
        );
        assert_eq!(flat.len(), 4);
        assert!(!sim.rifle().is_chambered());
        assert_eq!(sim.rifle().magazine().rounds(), 0);
    }

    #[test]
    fn a_dry_pull_does_not_delay_the_next_one() {
        let mut sim = arena_sim(rifle_with(0), vec![]);

        let ticks = run(&mut sim, &[pull(), pull()]);

        assert_eq!(ticks, [vec![Feedback::Dry], vec![Feedback::Dry]]);
    }

    #[test]
    fn the_cooldown_blocks_a_pull_until_its_ticks_pass() {
        let mut sim = arena_sim(rifle_with(30), vec![]);

        let ticks = run(&mut sim, &[pull(), pull(), pull(), pull(), pull()]);

        assert_eq!(fired_on(&ticks), [0, 3]);
        assert_eq!(sim.rifle().magazine().rounds(), 27);
    }

    #[test]
    fn a_blocked_pull_is_neither_fired_nor_dry() {
        let mut sim = arena_sim(rifle_with(30), vec![]);

        let ticks = run(&mut sim, &[pull(), pull()]);

        assert_eq!(ticks[1], []);
    }

    #[test]
    fn checking_the_magazine_reports_its_level() {
        let chambered_only = Rifle::loaded(mag(30, 1));
        let mut rifle = chambered_only;
        rifle.reload(Some(mag(30, 30)));
        let mut sim = arena_sim(rifle, vec![]);

        let full = run(&mut sim, &[check()]);
        let after_shot = run(&mut sim, &[pull(), idle(), idle(), check()]);

        assert_eq!(full, [vec![Feedback::MagazineLevel(MagazineLevel::Full)]]);
        assert_eq!(
            after_shot.last().expect("four ticks ran"),
            &[Feedback::MagazineLevel(MagazineLevel::MostlyFull)]
        );
    }

    #[test]
    fn checking_an_empty_magazine_reports_empty_without_spending_anything() {
        let mut sim = arena_sim(rifle_with(1), vec![]);

        let ticks = run(&mut sim, &[check()]);

        assert_eq!(ticks, [vec![Feedback::MagazineLevel(MagazineLevel::Empty)]]);
        assert!(sim.rifle().is_chambered());
    }

    #[test]
    fn the_rifle_cannot_fire_while_reloading() {
        let mut sim = arena_sim(rifle_with(10), vec![mag(30, 30)]);
        let mut script = vec![reload()];
        script.extend([pull(); 5]);
        script.extend([idle(); 5]);

        let ticks = run(&mut sim, &script);

        assert!(fired_on(&ticks).is_empty(), "fired during the reload");
        assert!(ticks.iter().flatten().all(|fb| *fb != Feedback::Dry));
        assert_eq!(ticks[10], [Feedback::ReloadDone]);
        assert_eq!(sim.rifle().magazine(), &mag(30, 30));
        assert!(sim.rifle().is_chambered());
        assert_eq!(sim.spares(), [mag(30, 9)]);
    }

    #[test]
    fn the_reload_ends_on_exactly_the_configured_tick() {
        let mut early = arena_sim(rifle_with(10), vec![mag(30, 30)]);
        let mut on_time = arena_sim(rifle_with(10), vec![mag(30, 30)]);
        let wait = |ticks| {
            let mut script = vec![reload()];
            script.extend(std::iter::repeat_n(idle(), ticks));
            script.push(pull());
            script
        };

        let before = run(&mut early, &wait(8));
        let after = run(&mut on_time, &wait(9));

        assert!(fired_on(&before).is_empty());
        assert_eq!(before[8], []);
        assert_eq!(fired_on(&after), [10]);
        assert_eq!(after[10], [Feedback::ReloadDone, Feedback::Fired]);
    }

    #[test]
    fn reloading_with_no_spare_magazine_changes_nothing() {
        let mut sim = arena_sim(rifle_with(10), vec![]);

        let ticks = run(
            &mut sim,
            &[
                reload(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                pull(),
            ],
        );

        assert!(
            ticks.iter().take(11).flatten().next().is_none(),
            "{ticks:?}"
        );
        assert_eq!(fired_on(&ticks), [11]);
        assert_eq!(sim.rifle().magazine().rounds(), 8);
    }

    #[test]
    fn a_shot_at_a_dummy_part_damages_that_part_and_reports_it() {
        let parts = [
            (BodyPart::Head, Some((100 - 30, 50 - 30, 60, 60))),
            (BodyPart::Torso, Some((100 - 30, 50, 60, 60))),
            (BodyPart::LeftLeg, Some((100 - 30, 50, 60, 60 - 30))),
        ];
        for (part, expected) in parts {
            let tag = SurfaceTag::Part { target: 0, part };
            let volume = dummy_hit_volumes(0)
                .into_iter()
                .find(|v| v.tag == tag.into())
                .expect("a volume per part");
            let mut config = test_config(rifle_with(5), vec![]);
            config.eye_height = volume.offset.y;
            let mut sim = FpsSim::new(
                flat_floor(),
                Vec3::new(volume.offset.x, 0.0, 0.0),
                &[Vec3::new(0.0, 0.0, 8.0)],
                config,
            );

            sim.tick(&pull(), DT);

            let feedback = sim.drain_feedback();
            let hit = feedback
                .iter()
                .find_map(|fb| match fb {
                    Feedback::Hit {
                        target,
                        part,
                        report,
                    } => Some((*target, *part, *report)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{part:?}: no hit in {feedback:?}"));
            assert_eq!((hit.0, hit.1), (0, part));
            assert_eq!((hit.2.part, hit.2.damage), (part, 30));
            let body = sim.dummy_body(0).expect("dummy 0 exists");
            let (health, head, left_arm, left_leg) = (
                body.health(),
                body.status(BodyPart::Head),
                body.status(BodyPart::LeftArm),
                body.status(BodyPart::LeftLeg),
            );
            let (eh, ehead, earm, eleg) = expected.expect("listed");
            assert_eq!(health, eh, "{part:?}");
            assert_eq!(head, Some(ehead), "{part:?}");
            assert_eq!(left_arm, Some(earm), "{part:?}");
            assert_eq!(left_leg, Some(eleg), "{part:?}");
            assert_eq!(body.status(BodyPart::Torso), None);
            assert!(feedback.contains(&Feedback::Fired));
        }
    }

    #[test]
    fn a_shot_over_a_dummy_reports_fired_and_no_hit() {
        let torso = SurfaceTag::Part {
            target: 0,
            part: BodyPart::Torso,
        };
        let volume = dummy_hit_volumes(0)
            .into_iter()
            .find(|v| v.tag == torso.into())
            .expect("a torso volume");
        let mut config = test_config(rifle_with(5), vec![]);
        config.eye_height = volume.offset.y + 5.0;
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::new(volume.offset.x, 0.0, 0.0),
            &[Vec3::new(0.0, 0.0, 8.0)],
            config,
        );

        sim.tick(&pull(), DT);

        assert_eq!(sim.drain_feedback(), [Feedback::Fired]);
        assert_eq!(sim.dummy_body(0).expect("dummy 0 exists").health(), 100);
    }

    #[test]
    fn a_dummy_beyond_the_range_is_not_hit() {
        let torso = SurfaceTag::Part {
            target: 0,
            part: BodyPart::Torso,
        };
        let volume = dummy_hit_volumes(0)
            .into_iter()
            .find(|v| v.tag == torso.into())
            .expect("a torso volume");
        let shoot_with_range = |range: f32| {
            let mut config = test_config(rifle_with(5), vec![]);
            config.eye_height = volume.offset.y;
            config.range = range;
            let mut sim = FpsSim::new(
                flat_floor(),
                Vec3::new(volume.offset.x, 0.0, 0.0),
                &[Vec3::new(0.0, 0.0, 8.0)],
                config,
            );
            sim.tick(&pull(), DT);
            sim.dummy_body(0).expect("dummy 0 exists").health()
        };

        let short = shoot_with_range(5.0);
        let long = shoot_with_range(12.0);

        assert_eq!(short, 100);
        assert_eq!(long, 70);
    }

    #[test]
    fn an_unknown_dummy_has_no_body() {
        let sim = arena_sim(rifle_with(5), vec![]);

        assert!(sim.dummy_body(0).is_none());
    }

    #[test]
    fn drained_feedback_is_not_reported_again() {
        let mut sim = arena_sim(rifle_with(5), vec![]);
        sim.tick(&pull(), DT);

        let pending = sim.pending_feedback().to_vec();
        let drained = sim.drain_feedback();

        assert_eq!(pending, [Feedback::Fired]);
        assert_eq!(drained, pending);
        assert!(sim.pending_feedback().is_empty());
    }
}

#[cfg(test)]
mod dummy_pose_tests {
    use searchlight_rules::BodyPart;

    use super::*;
    use crate::test_support::{DT, flat_floor, mag, test_config};

    #[test]
    fn a_dummy_still_takes_hits_where_it_stood_after_many_ticks() {
        let torso = SurfaceTag::Part {
            target: 0,
            part: BodyPart::Torso,
        };
        let volume = dummy_hit_volumes(0)
            .into_iter()
            .find(|v| v.tag == torso.into())
            .expect("a torso volume");
        let mut config = test_config(Rifle::loaded(mag(30, 5)), vec![]);
        config.eye_height = volume.offset.y;
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[Vec3::new(0.0, 0.0, 8.0)],
            config,
        );
        for _ in 0..600 {
            sim.tick(&FpsCommand::default(), DT);
        }

        sim.tick(
            &FpsCommand {
                fire: true,
                ..FpsCommand::default()
            },
            DT,
        );

        assert_eq!(sim.dummy_body(0).expect("dummy 0 exists").health(), 70);
    }
}

#[cfg(test)]
mod adversarial_tests {
    use moho_physics::{CharacterCommand, PITCH_LIMIT};
    use searchlight_rules::{BodyPart, MagazineLevel};

    use super::*;
    use crate::test_support::{DT, flat_floor, mag, test_config};

    fn rifle_with(rounds: u32) -> Rifle {
        Rifle::loaded(mag(30, rounds))
    }

    fn idle() -> FpsCommand {
        FpsCommand::default()
    }

    fn pull() -> FpsCommand {
        FpsCommand {
            fire: true,
            ..FpsCommand::default()
        }
    }

    fn reload() -> FpsCommand {
        FpsCommand {
            reload: true,
            ..FpsCommand::default()
        }
    }

    fn volume_of(target: u16, part: BodyPart) -> HitVolume {
        let tag = SurfaceTag::Part { target, part };
        dummy_hit_volumes(target)
            .into_iter()
            .find(|v| v.tag == tag.into())
            .expect("a volume per part")
    }

    fn run(sim: &mut FpsSim, script: &[FpsCommand]) -> Vec<Vec<Feedback>> {
        script
            .iter()
            .map(|command| {
                sim.tick(command, DT);
                sim.drain_feedback()
            })
            .collect()
    }

    #[test]
    fn a_shot_at_the_second_dummys_head_damages_only_that_dummy() {
        let head = volume_of(1, BodyPart::Head);
        let mut config = test_config(rifle_with(5), vec![]);
        config.eye_height = head.offset.y;
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::new(3.0, 0.0, 0.0),
            &[Vec3::new(0.0, 0.0, 8.0), Vec3::new(3.0, 0.0, 8.0)],
            config,
        );

        sim.tick(&pull(), DT);

        let feedback = sim.drain_feedback();
        let hits: Vec<_> = feedback
            .iter()
            .filter_map(|fb| match fb {
                Feedback::Hit { target, part, .. } => Some((*target, *part)),
                _ => None,
            })
            .collect();
        assert_eq!(hits, [(1, BodyPart::Head)], "{feedback:?}");
        let untouched = sim.dummy_body(0).expect("dummy 0 exists");
        assert_eq!(untouched.health(), 100);
        assert_eq!(untouched.status(BodyPart::Head), Some(50));
        let struck = sim.dummy_body(1).expect("dummy 1 exists");
        assert_eq!(struck.health(), 70);
        assert_eq!(struck.status(BodyPart::Head), Some(20));
    }

    #[test]
    fn extreme_target_numbers_round_trip_and_never_collide() {
        let parts = [
            BodyPart::Head,
            BodyPart::Torso,
            BodyPart::LeftArm,
            BodyPart::RightArm,
            BodyPart::LeftLeg,
            BodyPart::RightLeg,
        ];
        let mut seen = std::collections::HashMap::new();
        seen.insert(
            moho_physics::HitTag::from(SurfaceTag::World),
            SurfaceTag::World,
        );
        for target in [0, 1, u16::MAX - 1, u16::MAX] {
            for part in parts {
                let tag = SurfaceTag::Part { target, part };

                let engine = moho_physics::HitTag::from(tag);

                assert_eq!(SurfaceTag::try_from(engine), Ok(tag));
                let previous = seen.insert(engine, tag);
                assert_eq!(previous, None, "{tag:?} collides with {previous:?}");
            }
        }
    }

    #[test]
    fn pitch_extremes_keep_the_shot_finite_and_a_downward_shot_strikes_the_floor() {
        for pitch_delta in [-10.0_f32, 10.0] {
            let mut sim = FpsSim::new(
                flat_floor(),
                Vec3::ZERO,
                &[],
                test_config(rifle_with(5), vec![]),
            );
            let look = FpsCommand {
                movement: CharacterCommand {
                    pitch_delta,
                    ..CharacterCommand::default()
                },
                ..FpsCommand::default()
            };

            sim.tick(&look, DT);

            let state = sim.player();
            assert_eq!(state.pitch.abs(), PITCH_LIMIT);
            let view = first_person_view(&state, &state, sim.eye_height(), 1.0);
            assert!(view.eye.is_finite() && view.forward.is_finite(), "{view:?}");
            assert!((view.forward.length() - 1.0).abs() < 1e-3);
            let strike = resolve_shot(
                &flat_floor(),
                ShotRay {
                    origin: view.eye,
                    direction: view.forward,
                    range: 100.0,
                    ignore: None,
                },
            );
            if pitch_delta < 0.0 {
                let strike = strike.expect("a downward shot reaches the floor");
                assert_eq!(strike.tag, SurfaceTag::World);
                assert!(strike.point.is_finite() && strike.point.y.abs() < 1e-3);
            } else {
                assert!(strike.is_none(), "{strike:?}");
            }
            sim.tick(&pull(), DT);
            assert_eq!(sim.drain_feedback(), [Feedback::Fired]);
        }
    }

    #[test]
    fn a_reload_begun_in_a_cooldown_blocks_pulls_until_it_ends_and_leaves_no_cooldown() {
        for cooldown_ticks in [3, 30] {
            let mut config = test_config(rifle_with(10), vec![mag(30, 30)]);
            config.cooldown_ticks = cooldown_ticks;
            let mut sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);
            let mut script = vec![pull(), reload()];
            script.extend([pull(); 9]);
            script.push(pull());
            script.push(pull());

            let ticks = run(&mut sim, &script);

            for (i, fb) in ticks.iter().enumerate().take(11).skip(1) {
                assert!(
                    !fb.contains(&Feedback::Fired) && !fb.contains(&Feedback::Dry),
                    "cooldown {cooldown_ticks}: tick {i} acted during the reload: {fb:?}"
                );
            }
            assert_eq!(ticks[0], [Feedback::Fired]);
            assert_eq!(ticks[11], [Feedback::ReloadDone, Feedback::Fired]);
            assert_eq!(
                ticks[12],
                [],
                "the ordinary cooldown still applies after a shot"
            );
        }
    }

    #[test]
    fn pull_reload_and_check_in_one_tick_report_in_order_and_start_the_reload() {
        let mut reference = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(10), vec![mag(30, 30)]),
        );
        reference.tick(&pull(), DT);
        reference.drain_feedback();
        reference.tick(
            &FpsCommand {
                check: true,
                ..idle()
            },
            DT,
        );
        let Some(Feedback::MagazineLevel(before)) = reference.drain_feedback().pop() else {
            panic!("the reference check reported no level");
        };
        assert_ne!(
            before,
            MagazineLevel::Full,
            "the test needs a non-full pre-reload magazine"
        );
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(10), vec![mag(30, 30)]),
        );
        let all = FpsCommand {
            fire: true,
            reload: true,
            check: true,
            ..idle()
        };

        let ticks = run(
            &mut sim,
            &[
                all,
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
                idle(),
            ],
        );

        assert_eq!(ticks[0], [Feedback::Fired, Feedback::MagazineLevel(before)]);
        assert!(ticks[1..10].iter().all(Vec::is_empty), "{ticks:?}");
        assert_eq!(ticks[10], [Feedback::ReloadDone]);
        assert_eq!(sim.rifle().magazine(), &mag(30, 30));
    }

    #[test]
    fn an_instant_reload_completes_before_the_check_reads_the_level() {
        let mut config = test_config(rifle_with(10), vec![mag(30, 30)]);
        config.reload_ticks = 0;
        let mut sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let all = FpsCommand {
            fire: true,
            reload: true,
            check: true,
            ..idle()
        };

        let ticks = run(&mut sim, &[all]);

        assert_eq!(
            ticks[0],
            vec![
                Feedback::Fired,
                Feedback::ReloadDone,
                Feedback::MagazineLevel(MagazineLevel::Full),
            ]
        );
    }

    #[test]
    fn a_dead_dummy_keeps_reporting_hits_with_health_at_zero() {
        let torso = volume_of(0, BodyPart::Torso);
        let mut config = test_config(rifle_with(20), vec![]);
        config.eye_height = torso.offset.y;
        config.cooldown_ticks = 0;
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::new(torso.offset.x, 0.0, 0.0),
            &[Vec3::new(0.0, 0.0, 8.0)],
            config,
        );

        let ticks = run(&mut sim, &[pull(); 7]);

        let hits = ticks
            .iter()
            .flatten()
            .filter(|fb| {
                matches!(
                    fb,
                    Feedback::Hit {
                        target: 0,
                        part: BodyPart::Torso,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(hits, 7, "{ticks:?}");
        let body = sim.dummy_body(0).expect("dummy 0 exists");
        assert_eq!(body.health(), 0);
        assert!(body.is_dead());
    }

    #[test]
    fn a_second_reload_request_neither_restarts_nor_extends_the_reload() {
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(10), vec![mag(30, 30), mag(30, 20)]),
        );
        let mut script = vec![reload(), idle(), idle(), idle(), reload()];
        script.extend([reload(), idle()].iter().cycle().take(5).copied());
        script.extend([idle(); 6]);

        let ticks = run(&mut sim, &script);

        let done: Vec<usize> = ticks
            .iter()
            .enumerate()
            .filter(|(_, fb)| fb.contains(&Feedback::ReloadDone))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(done, [10], "{ticks:?}");
        assert_eq!(sim.rifle().magazine(), &mag(30, 30));
        assert_eq!(sim.spares().len(), 2);
    }
}

#[cfg(test)]
mod hit_volume_and_cooldown_tests {
    use moho_physics::HitTag;

    use super::*;
    use crate::test_support::{DT, flat_floor, mag, test_config};

    fn offset_of(volumes: &[HitVolume; 6], part: BodyPart) -> Vec3 {
        let tag = HitTag::from(SurfaceTag::Part { target: 4, part });
        volumes
            .iter()
            .find(|v| v.tag == tag)
            .expect("every part has a volume")
            .offset
    }

    #[test]
    fn dummy_volumes_put_left_parts_on_plus_x_right_on_minus_x_and_legs_below_the_torso() {
        let volumes = dummy_hit_volumes(4);

        let offsets = [
            BodyPart::Head,
            BodyPart::Torso,
            BodyPart::LeftArm,
            BodyPart::RightArm,
            BodyPart::LeftLeg,
            BodyPart::RightLeg,
        ]
        .map(|part| offset_of(&volumes, part).to_array());

        assert_eq!(
            offsets,
            [
                [0.0, 0.72, 0.0],
                [0.0, 0.25, 0.0],
                [0.3, 0.25, 0.0],
                [-0.3, 0.25, 0.0],
                [0.12, -0.625, 0.0],
                [-0.12, -0.625, 0.0],
            ]
        );
    }

    /// Ticks that fired when the trigger is pulled on each of `ticks` consecutive ticks.
    fn fired_ticks(cooldown_ticks: u32, ticks: usize) -> Vec<usize> {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.cooldown_ticks = cooldown_ticks;
        let mut sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let pull = FpsCommand {
            fire: true,
            ..FpsCommand::default()
        };
        (0..ticks)
            .filter(|_| {
                sim.tick(&pull, DT);
                sim.drain_feedback().contains(&Feedback::Fired)
            })
            .collect()
    }

    #[test]
    fn the_next_pull_is_accepted_exactly_cooldown_ticks_after_a_shot() {
        assert_eq!(fired_ticks(0, 4), [0, 1, 2, 3]);
        assert_eq!(fired_ticks(1, 4), [0, 1, 2, 3]);
        assert_eq!(fired_ticks(2, 6), [0, 2, 4]);
        assert_eq!(fired_ticks(3, 7), [0, 3, 6]);
    }

    /// Whether a level shot at a dummy `distance` ahead, from `eye_height` and sideways
    /// offset `x`, reports a hit.
    fn level_shot_hits(eye_height: f32, x: f32, distance: f32) -> bool {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.eye_height = eye_height;
        let mut sim = FpsSim::new(
            flat_floor(),
            Vec3::new(x, 0.0, 0.0),
            &[Vec3::new(0.0, 0.0, distance)],
            config,
        );
        let pull = FpsCommand {
            fire: true,
            ..FpsCommand::default()
        };

        sim.tick(&pull, DT);

        sim.drain_feedback()
            .iter()
            .any(|fb| matches!(fb, Feedback::Hit { target: 0, .. }))
    }

    #[test]
    fn a_level_shot_from_the_shipped_eye_height_hits_the_dummy() {
        let hit = level_shot_hits(0.7, 0.0, 4.0);

        assert!(hit);
    }

    #[test]
    fn a_level_shot_hits_at_every_height_between_the_dummys_feet_and_crown() {
        let volumes = dummy_hit_volumes(0);
        let bottom = volumes
            .iter()
            .map(|v| v.offset.y - v.half_extents.y)
            .fold(f32::INFINITY, f32::min);
        let top = volumes
            .iter()
            .map(|v| v.offset.y + v.half_extents.y)
            .fold(f32::NEG_INFINITY, f32::max);
        // Sideways 0.1 is inside the head, torso and both legs, so only vertical gaps can miss.
        let x = 0.1;
        // The exact bottom and top planes are grazing rays, so start and stop one step inside.
        let steps = ((top - bottom) / 0.01).floor() as i32;

        let misses: Vec<f32> = (1..steps)
            .map(|step| bottom + step as f32 * 0.01)
            .filter(|&eye| !level_shot_hits(eye, x, 5.0))
            .collect();

        assert!(
            misses.is_empty(),
            "level shots passed through at {misses:?}"
        );
    }

    #[test]
    fn eye_height_reports_the_configured_height() {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.eye_height = 1.6;

        let sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);

        assert_eq!(sim.eye_height(), 1.6);
    }
}

#[cfg(test)]
mod enemy_tests {
    use std::f32::consts::FRAC_PI_2;
    use std::path::PathBuf;

    use moho_physics::CharacterCommand;

    use super::*;
    use crate::arena;
    use crate::test_support::{DT, arena_with, mag, test_config};

    /// Height of the head volume's centre above the capsule centre: a level shot from here
    /// strikes a standing body's head.
    const HEAD_HEIGHT: f32 = 0.72;
    /// Head status of every body in these tests (see `test_profile`): two 30-damage hits kill.
    const PLAYER_ONE_SHOT: u32 = 60;

    fn config() -> SimConfig {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.eye_height = HEAD_HEIGHT;
        config
    }

    fn sturdy(mut config: SimConfig) -> SimConfig {
        config.body = BodyProfile {
            max_health: 1_000_000,
            head_status: 1_000_000,
            arm_status: 1_000_000,
            leg_status: 1_000_000,
        };
        config
    }

    fn idle() -> FpsCommand {
        FpsCommand::default()
    }

    fn pull() -> FpsCommand {
        FpsCommand {
            fire: true,
            ..FpsCommand::default()
        }
    }

    fn restart() -> FpsCommand {
        FpsCommand {
            restart: true,
            ..FpsCommand::default()
        }
    }

    /// `len` ticks of nothing, with a trigger pull on each tick in `pulls`.
    fn script(len: usize, pulls: &[usize]) -> Vec<FpsCommand> {
        (0..len)
            .map(|tick| {
                if pulls.contains(&tick) {
                    pull()
                } else {
                    idle()
                }
            })
            .collect()
    }

    fn run(sim: &mut FpsSim, script: &[FpsCommand]) -> Vec<Vec<Feedback>> {
        script
            .iter()
            .map(|command| {
                sim.tick(command, DT);
                sim.drain_feedback()
            })
            .collect()
    }

    fn ticks_where(ticks: &[Vec<Feedback>], matches: impl Fn(&Feedback) -> bool) -> Vec<usize> {
        ticks
            .iter()
            .enumerate()
            .filter(|(_, feedback)| feedback.iter().any(&matches))
            .map(|(tick, _)| tick)
            .collect()
    }

    fn count(ticks: &[Vec<Feedback>], matches: impl Fn(&Feedback) -> bool) -> usize {
        ticks.iter().flatten().filter(|fb| matches(fb)).count()
    }

    fn enemy_fired(feedback: &Feedback) -> bool {
        matches!(feedback, Feedback::EnemyFired { .. })
    }

    fn player_hit(feedback: &Feedback) -> bool {
        matches!(feedback, Feedback::PlayerHit { .. })
    }

    fn player_died(feedback: &Feedback) -> bool {
        matches!(feedback, Feedback::PlayerDied)
    }

    fn hit_of(feedback: &Feedback) -> Option<(u16, BodyPart)> {
        match feedback {
            Feedback::Hit { target, part, .. } => Some((*target, *part)),
            _ => None,
        }
    }

    fn horizontal(a: Vec3, b: Vec3) -> f32 {
        Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
    }

    const ENEMY_SPAWN: Vec3 = Vec3::new(0.0, 0.0, 10.0);
    /// Between the enemy and the player, so the enemy spawns facing the player.
    const TOWARD_PLAYER: Vec3 = Vec3::new(0.0, 0.0, 6.0);
    /// Far behind the enemy as seen from the player, so the enemy spawns facing away.
    const AWAY_FROM_PLAYER: Vec3 = Vec3::new(0.0, 0.0, 40.0);

    /// The player at the origin facing +Z and one enemy ten metres ahead, facing the player.
    fn duel(boxes: &[(Vec3, Vec3)]) -> Arc<Arena> {
        arena_with(boxes, Vec3::ZERO, &[ENEMY_SPAWN], &[TOWARD_PLAYER])
    }

    #[test]
    fn an_enemy_walks_a_full_lap_of_the_shipped_arena() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb");
        let (level, markers) = arena::load(&path).expect("the shipped arena loads");
        let mut arena = Arena::from_level(&level, &markers);
        arena.enemy_spawns.truncate(1);
        let route = arena.route.clone();
        let mut config = config();
        // Too short to ever notice the player, so only the route moves the enemy.
        config.enemy.sight_range = 0.5;
        let speed = config.move_settings.max_speed * config.enemy.patrol_speed;
        let lap: f32 = route
            .iter()
            .zip(route.iter().cycle().skip(1))
            .map(|(a, b)| horizontal(*a, *b))
            .sum();
        let budget = (lap / speed * 60.0 * 2.0) as usize + 600;
        let mut sim = FpsSim::start(Arc::new(arena), config);
        let mut closest = vec![f32::MAX; route.len()];

        for _ in 0..budget {
            sim.tick(&idle(), DT);
            let at = sim
                .enemy_state(0)
                .expect("nothing hurts the enemy")
                .position;
            for (best, waypoint) in closest.iter_mut().zip(&route) {
                *best = best.min(horizontal(at, *waypoint));
            }
        }

        for (index, best) in closest.iter().enumerate() {
            assert!(
                *best < 1.0,
                "waypoint {} was never reached, closest {best} m",
                index + 1
            );
        }
    }

    #[test]
    fn an_enemy_spawns_facing_its_nearest_waypoint_and_walks_to_it() {
        let arena = arena_with(
            &[],
            Vec3::new(0.0, 0.0, -10.0),
            &[ENEMY_SPAWN],
            &[Vec3::new(10.0, 0.0, 10.0), AWAY_FROM_PLAYER],
        );
        let mut sim = FpsSim::start(arena, config());

        let spawned = sim.enemy_state(0).expect("the enemy spawned");
        run(&mut sim, &script(30, &[]));

        let walked = sim.enemy_state(0).expect("the enemy is alive");
        assert!((spawned.yaw - FRAC_PI_2).abs() < 1e-3, "{spawned:?}");
        assert!(walked.position.x > 0.5, "{walked:?}");
        assert!((walked.position.z - 10.0).abs() < 0.2, "{walked:?}");
    }

    #[test]
    fn an_enemy_with_sight_fires_at_the_player() {
        let mut sim = FpsSim::start(duel(&[]), config());

        let ticks = run(&mut sim, &script(12, &[]));

        assert_eq!(ticks_where(&ticks, enemy_fired), [2, 6, 10]);
        assert_eq!(ticks_where(&ticks, player_hit), [2, 6, 10]);
        let Some(Feedback::PlayerHit { part, from }) =
            ticks[2].iter().find(|fb| player_hit(fb)).cloned()
        else {
            panic!("no hit on tick 2: {ticks:?}");
        };
        assert_eq!(part, BodyPart::Torso);
        assert!(horizontal(from, ENEMY_SPAWN) < 1.0, "{from:?}");
        assert_eq!(sim.player_body().health(), 70);
    }

    #[test]
    fn an_enemy_beyond_its_sight_range_does_not_fire() {
        let fired_with_range = |sight_range: f32| {
            let mut config = config();
            config.enemy.sight_range = sight_range;
            let mut sim = FpsSim::start(duel(&[]), config);
            let ticks = run(&mut sim, &script(30, &[]));
            count(&ticks, enemy_fired)
        };

        let short = fired_with_range(9.0);
        let long = fired_with_range(11.0);

        assert_eq!(short, 0);
        assert!(long > 0);
    }

    #[test]
    fn an_enemy_behind_a_wall_does_not_damage_the_player() {
        let wall = (Vec3::new(-8.0, 0.0, 5.0), Vec3::new(8.0, 4.0, 5.5));
        let mut sim = FpsSim::start(duel(&[wall]), config());

        let ticks = run(&mut sim, &script(180, &[]));

        assert_eq!(count(&ticks, player_hit), 0);
        assert_eq!(count(&ticks, enemy_fired), 0);
        assert_eq!(sim.player_body().health(), 100);
    }

    #[test]
    fn an_enemy_hit_drains_the_players_health_and_the_struck_part() {
        let cases = [
            // part, head status, left leg status
            (BodyPart::Head, 40, 60),
            (BodyPart::Torso, 50, 60),
            (BodyPart::LeftLeg, 50, 50),
        ];
        for (part, head, left_leg) in cases {
            let mut config = config();
            config.enemy_aim_at = part;
            let mut sim = FpsSim::start(duel(&[]), config);

            let ticks = run(&mut sim, &script(3, &[]));

            let hits: Vec<_> = ticks.iter().flatten().filter(|fb| player_hit(fb)).collect();
            assert!(
                matches!(hits[..], [Feedback::PlayerHit { part: struck, .. }] if *struck == part),
                "{part:?}: {ticks:?}"
            );
            let body = sim.player_body();
            assert_eq!(body.health(), 90, "{part:?}");
            assert_eq!(body.status(BodyPart::Head), Some(head), "{part:?}");
            assert_eq!(body.status(BodyPart::LeftLeg), Some(left_leg), "{part:?}");
            assert_eq!(body.status(BodyPart::RightLeg), Some(60), "{part:?}");
            assert_eq!(body.status(BodyPart::LeftArm), Some(60), "{part:?}");
            assert_eq!(body.status(BodyPart::RightArm), Some(60), "{part:?}");
            assert_eq!(body.status(BodyPart::Torso), None, "{part:?}");
        }
    }

    #[test]
    fn enemy_shots_to_the_head_kill_a_player_with_health_left() {
        let mut config = config();
        config.enemy_aim_at = BodyPart::Head;
        let mut sim = FpsSim::start(duel(&[]), config);

        let ticks = run(&mut sim, &script(60, &[]));

        assert_eq!(ticks_where(&ticks, player_died), [18]);
        let body = sim.player_body();
        assert!(body.is_dead());
        assert_eq!(body.status(BodyPart::Head), Some(0));
        assert_eq!(body.health(), 50);
        assert_eq!(count(&ticks, player_hit), 5);
        assert_eq!(count(&ticks, enemy_fired), 5);
    }

    #[test]
    fn enemy_shots_to_the_torso_kill_when_health_is_gone() {
        let mut sim = FpsSim::start(duel(&[]), config());

        let ticks = run(&mut sim, &script(80, &[]));

        assert_eq!(ticks_where(&ticks, player_died), [38]);
        let body = sim.player_body();
        assert!(body.is_dead());
        assert_eq!(body.health(), 0);
        assert_eq!(body.status(BodyPart::Head), Some(50));
        assert_eq!(count(&ticks, player_hit), 10);
        assert_eq!(count(&ticks, enemy_fired), 10);
    }

    #[test]
    fn spread_is_deterministic_for_a_seed() {
        let hit_pattern = |seed: u64| {
            let mut config = sturdy(config());
            config.enemy.aim_error = 0.05;
            config.enemy.damage = 1;
            config.enemy.seed = seed;
            let mut sim = FpsSim::start(duel(&[]), config);
            let ticks = run(&mut sim, &script(300, &[]));
            (
                ticks_where(&ticks, enemy_fired),
                ticks_where(&ticks, player_hit),
            )
        };

        let first = hit_pattern(1);
        let again = hit_pattern(1);
        let other = hit_pattern(2);

        assert_eq!(first, again);
        assert_ne!(first.1, other.1);
        assert!(first.1.len() < first.0.len(), "no shot was off target");
        assert!(!first.1.is_empty(), "no shot was on target");
    }

    #[test]
    fn a_shot_that_clips_another_enemy_is_absorbed() {
        let shooter = Vec3::new(0.0, 0.0, 12.0);
        // Beside the line from the shooter to the player, close enough that a wide spread
        // sometimes strikes it, and turned away so it never shoots.
        let bystander = Vec3::new(0.6, 0.0, 6.0);
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[shooter, bystander],
            &[Vec3::new(0.0, 0.0, 8.0), Vec3::new(0.6, 0.0, 40.0)],
        );
        let mut config = sturdy(config());
        config.enemy.aim_error = 0.2;
        config.enemy.patrol_speed = 0.0;
        let untouched = Body::new(config.body);
        let mut probe = FpsSim::start(arena.clone(), config.clone());
        let mut sim = FpsSim::start(arena, config);

        let ticks = run(&mut sim, &script(600, &[]));

        // Replay the shooter's spread on a twin sim: the same seed gives the same shots.
        let shooter = probe.targets[0].handle.expect("the shooter spawned");
        let eye =
            probe.world.character(shooter).expect("alive").position + Vec3::Y * probe.eye_height;
        let aim = probe.player_aim_point();
        let clipped = (0..count(&ticks, enemy_fired))
            .filter(|_| {
                let direction = probe.with_spread(aim - eye);
                matches!(
                    resolve_shot(
                        &probe.world,
                        ShotRay {
                            origin: eye,
                            direction,
                            range: probe.enemy.sight_range,
                            ignore: Some(shooter),
                        },
                    ),
                    Some(Strike {
                        tag: SurfaceTag::Part { target: 1, .. },
                        ..
                    })
                )
            })
            .count();
        assert!(clipped > 0, "no shot struck the bystander");

        assert!(
            count(&ticks, enemy_fired) > 100,
            "{} shots",
            count(&ticks, enemy_fired)
        );
        assert_eq!(sim.enemy_body(1), Some(&untouched));
    }

    #[test]
    fn a_killed_enemy_stops_moving_and_firing() {
        // Walking away from the player, who shoots it in the back of the head.
        let away = arena_with(&[], Vec3::ZERO, &[ENEMY_SPAWN], &[AWAY_FROM_PLAYER]);
        let mut config_a = config();
        config_a.damage = 30;
        let mut walking = FpsSim::start(away, config_a);
        // Facing the player, who answers its first shot with a killing one.
        let mut config_b = config();
        config_b.damage = PLAYER_ONE_SHOT;
        let mut firing = FpsSim::start(duel(&[]), config_b);

        let walked = run(&mut walking, &script(3, &[0]));
        let before = walking
            .enemy_state(0)
            .expect("alive after one hit")
            .position;
        let killed = run(&mut walking, &script(1, &[0]));
        let after_death = run(&mut walking, &script(60, &[]));
        let shooting = run(&mut firing, &script(90, &[3]));

        assert!(before.z > ENEMY_SPAWN.z + 0.02, "never walked: {before:?}");
        assert!(
            walked
                .iter()
                .flatten()
                .any(|fb| hit_of(fb) == Some((0, BodyPart::Head)))
        );
        assert!(
            killed[0].contains(&Feedback::EnemyDied { target: 0 }),
            "{killed:?}"
        );
        assert!(walking.enemy_state(0).is_none());
        assert!(walking.enemy_body(0).is_some_and(Body::is_dead));
        assert!(after_death.iter().all(Vec::is_empty), "{after_death:?}");
        assert_eq!(ticks_where(&shooting, enemy_fired), [2]);
        assert_eq!(
            ticks_where(&shooting, |fb| *fb == Feedback::EnemyDied { target: 0 }),
            [3]
        );
        assert_eq!(count(&shooting, player_hit), 1);
        assert_eq!(firing.player_body().health(), 90);
        assert!(firing.enemy_state(0).is_none());
    }

    #[test]
    fn shots_pass_through_where_a_dead_enemy_stood() {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[ENEMY_SPAWN, Vec3::new(0.0, 0.0, 16.0)],
            &[AWAY_FROM_PLAYER],
        );
        let mut config = config();
        config.damage = PLAYER_ONE_SHOT;
        let mut sim = FpsSim::start(arena, config);

        let ticks = run(&mut sim, &script(4, &[0, 3]));

        assert!(
            ticks[0].contains(&Feedback::EnemyDied { target: 0 }),
            "{ticks:?}"
        );
        let second: Vec<_> = ticks[3].iter().filter_map(hit_of).collect();
        assert_eq!(second, [(1, BodyPart::Head)], "{ticks:?}");
    }

    #[test]
    fn an_enemy_killed_this_tick_does_not_fire() {
        let mut config = config();
        config.damage = PLAYER_ONE_SHOT;
        let mut control = FpsSim::start(duel(&[]), config.clone());
        let mut contested = FpsSim::start(duel(&[]), config);

        let undisturbed = run(&mut control, &script(60, &[]));
        let shot_down = run(&mut contested, &script(60, &[2]));

        assert_eq!(ticks_where(&undisturbed, enemy_fired)[0], 2);
        assert!(
            shot_down[2].contains(&Feedback::EnemyDied { target: 0 }),
            "{shot_down:?}"
        );
        assert_eq!(count(&shot_down, enemy_fired), 0);
        assert_eq!(count(&shot_down, player_hit), 0);
        assert_eq!(contested.player_body().health(), 100);
    }

    #[test]
    fn an_enemy_does_not_fire_at_a_player_behind_it() {
        let arena = arena_with(&[], Vec3::ZERO, &[ENEMY_SPAWN], &[AWAY_FROM_PLAYER]);
        let mut sim = FpsSim::start(arena, config());

        let ticks = run(&mut sim, &script(180, &[]));

        assert_eq!(count(&ticks, enemy_fired), 0);
        assert_eq!(count(&ticks, player_hit), 0);
        let at = sim.enemy_state(0).expect("alive").position;
        assert!(at.z > ENEMY_SPAWN.z + 3.0, "stopped patrolling: {at:?}");
    }

    #[test]
    fn an_enemy_stops_firing_when_the_player_ducks_behind_a_wall() {
        let wall = (Vec3::new(1.0, 0.0, 5.0), Vec3::new(9.0, 4.0, 5.5));
        let arena = arena_with(
            &[wall],
            Vec3::ZERO,
            &[ENEMY_SPAWN],
            &[TOWARD_PLAYER, Vec3::new(0.0, 0.0, 25.0)],
        );
        let mut sim = FpsSim::start(arena, sturdy(config()));
        let turn_and_walk = |first: bool| FpsCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { FRAC_PI_2 } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..FpsCommand::default()
        };

        let mut exposed = Vec::new();
        let mut first = true;
        while sim.player().position.x < 4.5 {
            sim.tick(&turn_and_walk(first), DT);
            exposed.push(sim.drain_feedback());
            first = false;
            assert!(exposed.len() < 300, "the player never got behind the wall");
        }
        let enemy_when_hidden = sim.enemy_state(0).expect("alive").position;
        let hidden = run(&mut sim, &script(90, &[]));

        assert!(
            count(&exposed, enemy_fired) > 0,
            "it never fired in the open"
        );
        assert_eq!(count(&hidden, enemy_fired), 0);
        assert_eq!(count(&hidden, player_hit), 0);
        let enemy_now = sim.enemy_state(0).expect("alive").position;
        assert!(
            horizontal(enemy_now, enemy_when_hidden) > 0.5,
            "it did not resume its patrol: {enemy_when_hidden:?} -> {enemy_now:?}"
        );
    }

    #[test]
    fn a_dead_player_freezes_the_arena() {
        // The first enemy kills the player; the second walks away from them unaware.
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[ENEMY_SPAWN, Vec3::new(25.0, 0.0, 5.0)],
            &[TOWARD_PLAYER, Vec3::new(40.0, 0.0, 5.0)],
        );
        let mut config = config();
        config.enemy.damage = 100;
        config.enemy.reaction_ticks = 30;
        let mut sim = FpsSim::start(arena, config);
        let walker_start = sim.enemy_state(1).expect("spawned").position;

        let mut ticks = Vec::new();
        while !ticks.iter().flatten().any(player_died) {
            ticks.extend(run(&mut sim, &script(1, &[])));
            assert!(ticks.len() < 300, "the player never died");
        }
        let walker_at_death = sim.enemy_state(1).expect("alive").position;
        let player_at_death = sim.player();
        let rounds = sim.rifle().magazine().rounds();
        let restless = FpsCommand {
            movement: CharacterCommand {
                forward: 1.0,
                jump: true,
                ..CharacterCommand::default()
            },
            fire: true,
            reload: true,
            check: true,
            ..FpsCommand::default()
        };
        let frozen = run(&mut sim, &[restless; 90]);

        assert!(walker_at_death.x > walker_start.x + 0.5, "it never walked");
        assert!(frozen.iter().all(Vec::is_empty), "{frozen:?}");
        let walker_after = sim.enemy_state(1).expect("alive").position;
        assert!(horizontal(walker_after, walker_at_death) < 1e-4);
        assert!((sim.player().position - player_at_death.position).length() < 1e-4);
        assert_eq!(sim.rifle().magazine().rounds(), rounds);
    }

    #[test]
    fn no_enemy_fires_once_the_player_is_dead() {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[Vec3::new(-3.0, 0.0, 10.0), Vec3::new(3.0, 0.0, 10.0)],
            &[TOWARD_PLAYER],
        );
        let mut config = config();
        config.enemy.damage = 100;
        let mut sim = FpsSim::start(arena, config);

        let ticks = run(&mut sim, &script(120, &[]));

        let death = ticks_where(&ticks, player_died);
        assert_eq!(death.len(), 1, "{ticks:?}");
        let fired_that_tick = ticks[death[0]].iter().filter(|fb| enemy_fired(fb)).count();
        assert_eq!(fired_that_tick, 1, "{:?}", ticks[death[0]]);
        assert_eq!(count(&ticks, enemy_fired), 1);
        assert_eq!(count(&ticks, player_hit), 1);
    }

    /// The player killed by one of two enemies after dropping the other, then restarted.
    fn dead_player_in_a_busy_arena() -> (FpsSim, Arc<Arena>, SimConfig) {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[ENEMY_SPAWN, Vec3::new(3.0, 0.0, 10.0)],
            &[TOWARD_PLAYER],
        );
        let mut config = config();
        config.damage = PLAYER_ONE_SHOT;
        config.enemy.damage = 100;
        config.enemy.aim_error = 0.05;
        let mut sim = FpsSim::start(arena.clone(), config.clone());
        let mut dead = false;
        for tick in 0..2000 {
            sim.tick(&if tick == 0 { pull() } else { idle() }, DT);
            dead |= sim.drain_feedback().iter().any(player_died);
            if dead {
                break;
            }
        }
        assert!(dead, "the player never died");
        (sim, arena, config)
    }

    #[test]
    fn restart_after_death_matches_a_fresh_start() {
        let (mut restarted, arena, config) = dead_player_in_a_busy_arena();
        let mut fresh = FpsSim::start(arena, config);
        assert!(
            restarted.enemy_state(0).is_none(),
            "the setup killed the first enemy"
        );

        restarted.tick(&restart(), DT);

        assert_eq!(restarted.player(), fresh.player());
        assert_eq!(restarted.player_body(), fresh.player_body());
        assert!(!restarted.player_body().is_dead());
        assert_eq!(restarted.rifle(), fresh.rifle());
        assert_eq!(restarted.spares(), fresh.spares());
        assert_eq!(restarted.enemy_count(), fresh.enemy_count());
        for index in 0..fresh.enemy_count() {
            assert_eq!(
                restarted.enemy_state(index),
                fresh.enemy_state(index),
                "{index}"
            );
            assert_eq!(
                restarted.enemy_body(index),
                fresh.enemy_body(index),
                "{index}"
            );
        }
        assert_eq!(restarted.pending_feedback(), fresh.pending_feedback());
        let commands = script(300, &[0]);
        assert_eq!(run(&mut restarted, &commands), run(&mut fresh, &commands));
        assert_eq!(restarted.player(), fresh.player());
    }

    #[test]
    fn a_dead_player_in_an_arena_sim_restarts_to_a_living_one() {
        let mut sim = FpsSim::start(arena_with(&[], Vec3::ZERO, &[], &[]), config());
        sim.player_body.hit(BodyPart::Torso, 1_000);
        assert!(sim.player_body().is_dead());

        sim.tick(&restart(), DT);

        assert!(!sim.player_body().is_dead());
    }

    #[test]
    fn a_dead_player_in_a_sim_built_without_an_arena_restarts_to_a_living_one() {
        let mut sim = FpsSim::new(crate::test_support::flat_floor(), Vec3::ZERO, &[], config());
        sim.player_body.hit(BodyPart::Torso, 1_000);
        assert!(sim.player_body().is_dead());

        sim.tick(&restart(), DT);

        assert!(!sim.player_body().is_dead());
    }

    #[test]
    fn restart_while_alive_does_nothing() {
        let arena = duel(&[]);
        let mut config = config();
        config.damage = PLAYER_ONE_SHOT;
        let mut pressed = FpsSim::start(arena.clone(), config.clone());
        let mut control = FpsSim::start(arena, config);
        let walk = FpsCommand {
            movement: CharacterCommand {
                forward: 1.0,
                ..CharacterCommand::default()
            },
            ..FpsCommand::default()
        };
        let lead_in = [walk; 5];
        let with_restart = FpsCommand {
            restart: true,
            ..walk
        };
        run(&mut pressed, &lead_in);
        run(&mut control, &lead_in);

        let after_restart = run(&mut pressed, &[with_restart, pull(), idle(), idle()]);
        let after_nothing = run(&mut control, &[walk, pull(), idle(), idle()]);

        assert_eq!(after_restart, after_nothing);
        assert_eq!(pressed.player(), control.player());
        assert_eq!(pressed.player_body(), control.player_body());
        assert_eq!(pressed.rifle(), control.rifle());
        assert_eq!(pressed.enemy_state(0), control.enemy_state(0));
        assert_eq!(pressed.enemy_body(0), control.enemy_body(0));
        assert!(pressed.player().position.z > 0.05, "the walk was undone");
    }

    #[test]
    fn the_players_hit_volumes_are_a_dummys_tagged_as_the_player() {
        let player = player_hit_volumes();
        let dummy = dummy_hit_volumes(0);

        for (own, theirs) in player.iter().zip(&dummy) {
            assert_eq!(own.offset, theirs.offset);
            assert_eq!(own.half_extents, theirs.half_extents);
            let tag = SurfaceTag::try_from(own.tag).expect("a surface tag");
            let expected = SurfaceTag::try_from(theirs.tag).expect("a surface tag");
            let (SurfaceTag::Player(part), SurfaceTag::Part { part: same, .. }) = (tag, expected)
            else {
                panic!("{tag:?} / {expected:?}");
            };
            assert_eq!(part, same);
        }
    }

    /// Low enough that the sight and shot rays at head height pass over it, high enough to
    /// stop a ray to the hide height: behind it, as seen from the player, is cover.
    const LOW_WALL: (Vec3, Vec3) = (Vec3::new(0.5, 0.0, 9.0), Vec3::new(9.0, 1.2, 9.2));
    /// Behind `LOW_WALL`, on the side away from the player.
    const SHIELDED: Vec3 = Vec3::new(4.0, 0.0, 10.0);
    /// Open ground nearer to the enemy than `SHIELDED`.
    const DECOY: Vec3 = Vec3::new(-2.0, 0.0, 10.0);
    /// Two shielded points near the enemies spawned at `ENEMY_SPAWN` and `NEIGHBOUR_SPAWN`:
    /// `NEAR_COVER` is the nearer to both.
    const NEAR_COVER: Vec3 = Vec3::new(1.2, 0.0, 10.0);
    const FAR_COVER: Vec3 = Vec3::new(5.5, 0.0, 10.0);
    const NEIGHBOUR_SPAWN: Vec3 = Vec3::new(3.0, 0.0, 10.0);

    fn with_covers(arena: Arc<Arena>, covers: &[Vec3]) -> Arc<Arena> {
        let mut arena = (*arena).clone();
        arena.covers = covers.to_vec();
        Arc::new(arena)
    }

    /// Behind the enemy at `ENEMY_SPAWN`, off its straight walk to `SHIELDED`.
    const BYSTANDER_SPAWN: Vec3 = Vec3::new(-3.0, 0.0, 10.0);

    /// A player sturdy enough to outlive these scenarios however long the enemies fire.
    fn cover_config() -> SimConfig {
        let mut config = config();
        config.enemy.damage = 1;
        config.body.max_health = 1_000_000;
        config
    }

    /// A trigger pull at enemy `index`, turning the player to face it first.
    fn shoot_at(sim: &FpsSim, index: usize) -> FpsCommand {
        let at = sim
            .enemy_state(index)
            .expect("the enemy to shoot at is alive")
            .position;
        let player = sim.player();
        let bearing = (at.x - player.position.x).atan2(at.z - player.position.z);
        FpsCommand {
            movement: CharacterCommand {
                yaw_delta: bearing - player.yaw,
                ..CharacterCommand::default()
            },
            fire: true,
            ..FpsCommand::default()
        }
    }

    fn enemy_at(sim: &FpsSim, index: usize) -> Vec3 {
        sim.enemy_state(index).expect("the enemy is alive").position
    }

    fn fired_by(target: u16) -> impl Fn(&Feedback) -> bool {
        move |feedback| *feedback == Feedback::EnemyFired { target }
    }

    #[test]
    fn an_enemy_hit_by_the_player_runs_to_cover() {
        let arena = with_covers(duel(&[LOW_WALL]), &[DECOY, SHIELDED]);
        let mut sim = FpsSim::start(arena, cover_config());

        let ticks = run(&mut sim, &script(300, &[0]));

        let hits: Vec<_> = ticks[0].iter().filter_map(hit_of).collect();
        assert_eq!(hits, [(0, BodyPart::Head)], "{:?}", ticks[0]);
        let at = enemy_at(&sim, 0);
        assert!(
            horizontal(at, SHIELDED) < 1.0,
            "not at the shielded point: {at:?}"
        );
    }

    #[test]
    fn an_enemy_in_the_shipped_arena_takes_cover_when_shot() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb");
        let (level, markers) = arena::load(&path).expect("the shipped arena loads");
        let mut arena = Arena::from_level(&level, &markers);
        arena.enemy_spawns.retain(|spawn| spawn.x.abs() < 0.1);
        assert_eq!(
            arena.enemy_spawns.len(),
            1,
            "the test needs the enemy on x = 0"
        );
        arena.player_spawn = Vec3::new(0.0, 0.0, -5.0);
        let spawn = arena.enemy_spawns[0];
        let nearest = arena
            .covers
            .iter()
            .copied()
            .min_by(|a, b| horizontal(*a, spawn).total_cmp(&horizontal(*b, spawn)))
            .expect("the shipped arena has cover points");
        assert!(
            horizontal(nearest, spawn) < 15.0,
            "no cover point within range of the enemy"
        );
        let first_enemy =
            u16::try_from(arena.dummy_spawns.len()).expect("a few dummies fit a target number");
        let mut sim = FpsSim::start(Arc::new(arena), cover_config());

        let ticks = run(&mut sim, &script(400, &[0]));

        let hits: Vec<_> = ticks[0].iter().filter_map(hit_of).collect();
        assert_eq!(hits, [(first_enemy, BodyPart::Head)], "{:?}", ticks[0]);
        let at = enemy_at(&sim, 0);
        assert!(
            horizontal(at, nearest) < 1.0,
            "not at the cover point {nearest:?}: {at:?}"
        );
    }

    #[test]
    fn a_hit_that_kills_sends_nobody_to_cover() {
        let arena = with_covers(
            arena_with(
                &[LOW_WALL],
                Vec3::ZERO,
                &[ENEMY_SPAWN, BYSTANDER_SPAWN],
                &[TOWARD_PLAYER],
            ),
            &[SHIELDED],
        );
        let mut lethal = cover_config();
        lethal.damage = PLAYER_ONE_SHOT;
        let mut wounding = cover_config();
        wounding.damage = 30;
        let mut killer = FpsSim::start(arena.clone(), lethal);
        let mut control = FpsSim::start(arena, wounding);

        let killed = run(&mut killer, &script(300, &[0]));
        run(&mut control, &script(300, &[0]));

        assert_eq!(
            count(&killed, |fb| *fb == Feedback::EnemyDied { target: 0 }),
            1,
            "{:?}",
            killed[0]
        );
        assert!(killer.enemy_state(0).is_none());
        assert!(
            horizontal(enemy_at(&control, 0), SHIELDED) < 1.0,
            "the control enemy did not take cover: {:?}",
            enemy_at(&control, 0)
        );
        assert!(
            horizontal(enemy_at(&killer, 1), BYSTANDER_SPAWN) < 1.0,
            "the bystander ran for cover: {:?}",
            enemy_at(&killer, 1)
        );
    }

    fn two_enemies_and_two_shielded_points() -> Arc<Arena> {
        with_covers(
            arena_with(
                &[LOW_WALL],
                Vec3::ZERO,
                &[ENEMY_SPAWN, NEIGHBOUR_SPAWN],
                &[TOWARD_PLAYER],
            ),
            &[NEAR_COVER, FAR_COVER],
        )
    }

    #[test]
    fn two_enemies_hit_together_take_different_cover_points() {
        let mut config = cover_config();
        config.cooldown_ticks = 0;
        let mut sim = FpsSim::start(two_enemies_and_two_shielded_points(), config);

        let first = shoot_at(&sim, 0);
        sim.tick(&first, DT);
        let second = shoot_at(&sim, 1);
        sim.tick(&second, DT);
        let mut ticks = vec![sim.drain_feedback()];
        ticks.extend(run(&mut sim, &script(300, &[])));

        let mut struck: Vec<u16> = ticks
            .iter()
            .flatten()
            .filter_map(hit_of)
            .map(|(target, _)| target)
            .collect();
        struck.sort_unstable();
        assert_eq!(struck, [0, 1], "both enemies must be hit");
        assert!(
            horizontal(enemy_at(&sim, 0), NEAR_COVER) < 1.0,
            "the first enemy is at {:?}",
            enemy_at(&sim, 0)
        );
        assert!(
            horizontal(enemy_at(&sim, 1), FAR_COVER) < 1.0,
            "the second enemy is at {:?}",
            enemy_at(&sim, 1)
        );
    }

    #[test]
    fn a_dead_enemys_cover_point_is_free_again() {
        let mut config = cover_config();
        config.cooldown_ticks = 0;
        let mut sim = FpsSim::start(two_enemies_and_two_shielded_points(), config);
        let first = shoot_at(&sim, 0);
        sim.tick(&first, DT);
        run(&mut sim, &script(120, &[]));
        assert!(
            horizontal(enemy_at(&sim, 0), NEAR_COVER) < 1.0,
            "the first enemy never reached the near point: {:?}",
            enemy_at(&sim, 0)
        );

        let finisher = shoot_at(&sim, 0);
        sim.tick(&finisher, DT);
        let killed = sim.drain_feedback();
        let second = shoot_at(&sim, 1);
        sim.tick(&second, DT);
        run(&mut sim, &script(300, &[]));

        assert!(
            killed.contains(&Feedback::EnemyDied { target: 0 }),
            "{killed:?}"
        );
        assert!(
            horizontal(enemy_at(&sim, 1), NEAR_COVER) < 1.0,
            "the second enemy is at {:?}, not at the freed point",
            enemy_at(&sim, 1)
        );
    }

    #[test]
    fn an_enemy_at_cover_fires_at_the_player() {
        let arena = with_covers(duel(&[LOW_WALL]), &[SHIELDED]);
        let mut sim = FpsSim::start(arena, cover_config());
        sim.tick(&pull(), DT);
        sim.drain_feedback();
        let mut waited = 0;
        while horizontal(enemy_at(&sim, 0), SHIELDED) > 0.6 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
            waited += 1;
            assert!(waited < 300, "never reached cover: {:?}", enemy_at(&sim, 0));
        }

        let ticks = run(&mut sim, &script(120, &[]));

        assert!(
            count(&ticks, enemy_fired) >= 5,
            "{} shots",
            count(&ticks, enemy_fired)
        );
        assert!(
            count(&ticks, player_hit) >= 5,
            "{} hits",
            count(&ticks, player_hit)
        );
        assert!(horizontal(enemy_at(&sim, 0), SHIELDED) < 1.0);
    }

    /// A wall near the player: walking east past it, the player leaves the sight of an enemy
    /// ten metres ahead at x = 1.1 or so, with the wall far from the enemy's way there.
    const SIDE_WALL: (Vec3, Vec3) = (Vec3::new(1.0, 0.0, 0.8), Vec3::new(9.0, 4.0, 1.0));
    /// Roughly where the player was when last seen, on the ground plane.
    const LAST_SEEN: Vec3 = Vec3::new(1.1, 0.0, 0.0);

    /// The enemy ten metres ahead of the player, a waypoint beside it and a far one behind,
    /// and the player walking east behind `SIDE_WALL`.
    fn player_ducks_behind_the_side_wall() -> FpsSim {
        let arena = arena_with(
            &[SIDE_WALL],
            Vec3::ZERO,
            &[ENEMY_SPAWN],
            &[TOWARD_PLAYER, Vec3::new(0.0, 0.0, 25.0)],
        );
        let mut config = cover_config();
        config.enemy.arrive_radius = 2.5;
        config.enemy.patrol_speed = 1.0;
        config.enemy.chase_ticks = 600;
        let mut sim = FpsSim::start(arena, config);
        let turn_and_walk = |first: bool| FpsCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { FRAC_PI_2 } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..FpsCommand::default()
        };
        let mut first = true;
        let mut walked = 0;
        while sim.player().position.x < 4.0 {
            sim.tick(&turn_and_walk(first), DT);
            sim.drain_feedback();
            first = false;
            walked += 1;
            assert!(walked < 300, "the player never got behind the wall");
        }
        sim
    }

    #[test]
    fn an_enemy_chases_a_player_who_ducks_behind_a_wall() {
        let mut sim = player_ducks_behind_the_side_wall();

        let mut closest = f32::MAX;
        for _ in 0..500 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
            closest = closest.min(horizontal(enemy_at(&sim, 0), LAST_SEEN));
        }

        assert!(
            closest < 3.0,
            "the enemy never came within 3 m of the last seen position, closest {closest} m"
        );
    }

    #[test]
    fn a_chasing_enemy_that_finds_nothing_patrols_again() {
        let mut sim = player_ducks_behind_the_side_wall();

        let mut arrived = false;
        let mut patrolling_again = false;
        for _ in 0..1500 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
            let at = enemy_at(&sim, 0);
            arrived |= horizontal(at, LAST_SEEN) < 3.0;
            patrolling_again |= arrived && at.z > 12.0;
        }

        assert!(arrived, "the enemy never went to the last seen position");
        assert!(
            patrolling_again,
            "the enemy never walked its route again: {:?}",
            enemy_at(&sim, 0)
        );
    }

    #[test]
    fn a_hurt_enemy_with_no_free_cover_keeps_firing() {
        let arena = with_covers(
            arena_with(
                &[LOW_WALL],
                Vec3::ZERO,
                &[ENEMY_SPAWN, NEIGHBOUR_SPAWN],
                &[TOWARD_PLAYER],
            ),
            &[NEAR_COVER],
        );
        let mut config = cover_config();
        config.cooldown_ticks = 0;
        let mut sim = FpsSim::start(arena, config);
        let first = shoot_at(&sim, 0);
        sim.tick(&first, DT);
        run(&mut sim, &script(120, &[]));
        assert!(
            horizontal(enemy_at(&sim, 0), NEAR_COVER) < 1.0,
            "the first enemy never took the only cover point: {:?}",
            enemy_at(&sim, 0)
        );

        let second = shoot_at(&sim, 1);
        sim.tick(&second, DT);
        let hit = sim.drain_feedback();
        let ticks = run(&mut sim, &script(400, &[]));

        assert!(hit.iter().filter_map(hit_of).any(|h| h.0 == 1), "{hit:?}");
        assert!(
            count(&ticks, fired_by(1)) >= 30,
            "the second enemy fired {} times",
            count(&ticks, fired_by(1))
        );
        assert!(
            horizontal(enemy_at(&sim, 1), NEIGHBOUR_SPAWN) < 1.0,
            "the second enemy left its place: {:?}",
            enemy_at(&sim, 1)
        );
    }

    #[test]
    fn a_hurt_enemy_in_an_arena_without_cover_points_keeps_firing() {
        let mut sim = FpsSim::start(duel(&[LOW_WALL]), cover_config());

        let ticks = run(&mut sim, &script(400, &[0]));

        assert!(
            count(&ticks[1..], enemy_fired) >= 30,
            "{} shots",
            count(&ticks[1..], enemy_fired)
        );
        assert!(horizontal(enemy_at(&sim, 0), ENEMY_SPAWN) < 1.0);
    }

    /// A wall halfway between the player and `SHIELDED`'s row of points, `top` metres high.
    /// The line from the player's eye (1.15 + `HEAD_HEIGHT` up) to the hide height behind it
    /// crosses the wall at about 1.38 m; level enemy shots at the player's head pass over it.
    fn midfield_wall(top: f32) -> (Vec3, Vec3) {
        (Vec3::new(-10.0, 0.0, 5.0), Vec3::new(10.0, top, 5.1))
    }

    fn head_aiming_cover_config() -> SimConfig {
        let mut config = cover_config();
        config.enemy_aim_at = BodyPart::Head;
        config
    }

    #[test]
    fn a_wall_above_the_line_from_the_players_eye_hides_a_cover_point() {
        let arena = with_covers(duel(&[midfield_wall(1.55)]), &[SHIELDED]);
        let mut sim = FpsSim::start(arena, head_aiming_cover_config());

        run(&mut sim, &script(300, &[0]));

        let at = enemy_at(&sim, 0);
        assert!(
            horizontal(at, SHIELDED) < 1.0,
            "the enemy did not take the hidden point: {at:?}"
        );
    }

    #[test]
    fn a_wall_below_the_line_from_the_players_eye_leaves_a_cover_point_in_view() {
        let arena = with_covers(duel(&[midfield_wall(1.2)]), &[SHIELDED]);
        let mut sim = FpsSim::start(arena, head_aiming_cover_config());

        let ticks = run(&mut sim, &script(300, &[0]));

        let at = enemy_at(&sim, 0);
        assert!(
            horizontal(at, ENEMY_SPAWN) < 1.0,
            "the enemy ran to a point the player can see: {at:?}"
        );
        assert!(count(&ticks[1..], enemy_fired) >= 30);
    }
}

#[cfg(test)]
mod pinned_behaviour_tests {
    use super::*;
    use crate::test_support::{DT, arena_with, flat_floor, mag, test_config};

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn splitmix64_from_seed_zero_matches_the_published_reference_outputs() {
        let mut rng = SplitMix64(0);

        let outputs = [rng.next_u64(), rng.next_u64(), rng.next_u64()];

        assert_eq!(
            outputs,
            [
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F
            ]
        );
    }

    #[test]
    fn splitmix64_from_another_seed_matches_its_reference_outputs() {
        let mut rng = SplitMix64(7);

        let outputs = [rng.next_u64(), rng.next_u64(), rng.next_u64()];

        assert_eq!(
            outputs,
            [
                0x63CB_E1E4_5932_0DD7,
                0x044C_3CD7_F43C_661C,
                0xE698_4080_BAB1_2A02
            ]
        );
    }

    #[test]
    fn next_signed_maps_the_top_24_bits_onto_minus_one_to_one() {
        let mut rng = SplitMix64(0);

        let values = [rng.next_signed(), rng.next_signed(), rng.next_signed()];

        assert_close(values[0], 0.766_622);
        assert_close(values[1], -0.136_944);
        assert_close(values[2], -0.947_132);
    }

    #[test]
    fn nearest_waypoint_picks_the_closest_on_the_ground_and_faces_it() {
        let spawn = Vec3::new(2.0, 0.0, 10.0);
        let route = [Vec3::new(5.0, 7.0, 14.0), Vec3::new(2.0, 0.0, -10.0)];

        let (index, yaw) = nearest_waypoint(spawn, &route);

        assert_eq!(index, 0);
        assert_close(yaw, 3.0_f32.atan2(4.0));
    }

    #[test]
    fn nearest_waypoint_prefers_a_later_waypoint_when_it_is_closer() {
        let spawn = Vec3::new(-4.0, 0.0, -6.0);
        let route = [Vec3::new(20.0, 0.0, 20.0), Vec3::new(-4.0, 0.0, -9.0)];

        let (index, yaw) = nearest_waypoint(spawn, &route);

        assert_eq!(index, 1);
        assert_close(yaw, std::f32::consts::PI);
    }

    #[test]
    fn spread_turns_the_aim_by_the_next_two_draws_across_and_up() {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.enemy.aim_error = 0.5;
        config.enemy.seed = 7;
        let mut sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let mut reference = SplitMix64(7);
        let across = reference.next_signed() * 0.5;
        let rise = reference.next_signed() * 0.5;

        let aimed = sim.with_spread(Vec3::new(0.0, 0.0, 5.0));

        // Facing +Z the right-hand axis is -X and up is +Y.
        assert!(
            aimed.abs_diff_eq(Vec3::new(-across, rise, 1.0), 1e-5),
            "{aimed:?}"
        );
    }

    #[test]
    fn the_sim_reports_the_configured_maximum_health() {
        let config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        let sim = FpsSim::new(flat_floor(), Vec3::ZERO, &[], config);

        assert_eq!(sim.player_max_health(), 100);
    }

    #[test]
    fn enemies_are_counted_and_indexed_after_the_dummies() {
        let mut arena = (*arena_with(
            &[],
            Vec3::new(0.0, 0.0, -10.0),
            &[Vec3::new(5.0, 0.0, 5.0), Vec3::new(8.0, 0.0, 5.0)],
            &[Vec3::new(10.0, 0.0, 10.0)],
        ))
        .clone();
        arena.dummy_spawns = vec![Vec3::new(-10.0, 0.0, -10.0)];
        let config = test_config(Rifle::loaded(mag(30, 30)), vec![]);

        let sim = FpsSim::start(Arc::new(arena), config);

        assert_eq!(sim.enemy_count(), 2);
        assert!(sim.dummy_body(0).is_some());
        assert!(sim.enemy_body(0).is_some());
        assert!(sim.enemy_body(1).is_some());
        assert!(sim.enemy_body(2).is_none());
        assert!(sim.enemy_state(1).is_some());
        assert!(sim.enemy_state(2).is_none());
    }

    #[test]
    fn a_sim_without_enemies_has_none() {
        let config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        let sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[Vec3::new(3.0, 0.0, 3.0), Vec3::new(6.0, 0.0, 3.0)],
            config,
        );

        assert_eq!(sim.enemy_count(), 0);
    }

    #[test]
    fn an_enemy_shoots_from_its_eye_through_a_gap_only_that_height_clears() {
        // A low wall under a lintel leaves a slot from 1.2 m to 1.8 m; the eye ray crosses it
        // at about 1.65 m, while eyes lower or higher than the configured one are blocked.
        let low = (Vec3::new(-8.0, 0.0, 5.0), Vec3::new(8.0, 1.2, 5.5));
        let lintel = (Vec3::new(-8.0, 1.8, 5.0), Vec3::new(8.0, 4.0, 5.5));
        let arena = arena_with(
            &[low, lintel],
            Vec3::ZERO,
            &[Vec3::new(0.0, 0.0, 10.0)],
            &[Vec3::new(0.0, 0.0, 6.0)],
        );
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.eye_height = 0.72;
        let mut sim = FpsSim::start(arena, config);

        let mut feedback = Vec::new();
        for _ in 0..3 {
            sim.tick(&FpsCommand::default(), DT);
            feedback.extend(sim.drain_feedback());
        }

        let hits: Vec<Vec3> = feedback
            .iter()
            .filter_map(|fb| match fb {
                Feedback::PlayerHit { from, .. } => Some(*from),
                _ => None,
            })
            .collect();
        let eye = sim.enemy_state(0).expect("alive").position + Vec3::Y * 0.72;
        assert_eq!(hits.len(), 1, "{feedback:?}");
        assert!(hits[0].abs_diff_eq(eye, 1e-4), "{:?} vs {eye:?}", hits[0]);
    }
}
