//! The fixed-tick simulation: player, rifle, dummies and the feedback they produce.
//! It holds no renderer, audio or UI handle.

use std::sync::Arc;

use glam::{Quat, Vec3};
use moho_core::rng::{SimRng, StreamId};
use moho_level::NavMesh;
use moho_physics::{
    CharacterBody, CharacterCommand, CharacterHandle, CharacterSpawn, CharacterState, HitVolume,
    MoveSettings, PhysicsWorld, first_person_view,
};
use searchlight_rules::{
    Body, BodyPart, BodyProfile, Grade, HitReport, Lean, LeanReach, LeanSide, LeanTuning, Loadout,
    MagazineLevel, Rifle, Rig, Shot,
};

use crate::arena::{Arena, SETTLE_DT};
use crate::brain::{Brain, EnemyTuning, Percept};
use crate::cover::{self, Threat};
use crate::input::SearchlightCommand;
use crate::lean::lean_offset;
use crate::shot::{ShotRay, Strike, passes_within, resolve_shot};
use crate::tag::SurfaceTag;
use crate::tuning::SHIPPED_BODY;

/// How far past a full lean the reach rays go, in metres, so the eye and the near plane stay
/// off a wall.
const LEAN_CLEARANCE: f32 = 0.15;
/// How far from the eye a control can be used, in metres.
pub const USE_REACH: f32 = 2.0;

/// Height of the shipped body's capsule centre above its feet, in metres: half its hull. For
/// code with no [`SimConfig`] at hand; the sim itself spawns the configured body.
pub const STAND_HEIGHT: f32 = SHIPPED_BODY.standing.hull.length / 2.0;

/// The capsule centre of `body` standing on the floor point `floor`.
fn standing_centre(body: &CharacterBody, floor: Vec3) -> Vec3 {
    floor + Vec3::Y * (body.standing.hull.length / 2.0)
}

/// The target number of the dummy at `index` in a spawn list.
///
/// # Panics
/// If `index` is more than a `u16` target number can name.
fn target_number(index: usize) -> u16 {
    u16::try_from(index).expect("an arena has fewer than 65536 dummies")
}

/// The target number and capsule centre of the `body` dummy at `index` in a spawn list,
/// for a dummy standing on the floor point `spawn`.
///
/// # Panics
/// If `index` is more than a `u16` target number can name.
pub(crate) fn dummy_at(body: &CharacterBody, index: usize, spawn: Vec3) -> (u16, Vec3) {
    (target_number(index), standing_centre(body, spawn))
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

/// Radius of the shipped body's capsule, in metres; two shipped bodies touch at twice this.
pub const CAPSULE_RADIUS: f32 = SHIPPED_BODY.standing.hull.radius;

/// Whether a sim starts with an enemy at each enemy spawn marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    AtMarkers,
    /// No enemies; the markers are remembered for [`SearchlightSim::spawn_enemy`].
    Empty,
}

/// Why [`SearchlightSim::spawn_enemy`] added no enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SpawnError {
    #[error("the arena has no enemy spawn marker {marker}")]
    UnknownMarker { marker: usize },
    #[error("enemy spawn marker {marker} is occupied")]
    OccupiedMarker { marker: usize },
    #[error("no more targets can be numbered")]
    TooManyTargets,
}

/// Tuning the simulation is built with.
#[derive(Clone, Debug)]
pub struct SimConfig {
    pub move_settings: MoveSettings,
    /// Downward acceleration of the world, m/s², positive down: it pulls the player (whose
    /// `move_settings.gravity` is `None`) and every rigid body.
    pub gravity: f32,
    /// The hull and eye of the player and every target. Named apart from `body`, the pools.
    pub character_body: CharacterBody,
    /// How far a shot reaches, in metres.
    pub range: f32,
    /// Damage of one hit.
    pub damage: u32,
    /// Ticks between one shot and the next allowed pull: shots on ticks `t` and
    /// `t + cooldown_ticks` are both allowed, one on `t + cooldown_ticks - 1` is not.
    pub cooldown_ticks: u32,
    /// Ticks a tactical reload (round in the chamber) takes: pressed on tick `t`, the
    /// rifle can fire again on tick `t + reload_ticks`.
    pub reload_ticks: u32,
    /// Ticks added to `reload_ticks` when the chamber is empty at the press.
    pub charge_ticks: u32,
    /// Pools of the dummies', enemies' and the player's bodies.
    pub body: BodyProfile,
    /// The rifle in hand and the rig it reloads from.
    pub loadout: Loadout,
    /// How every enemy notices, aims and fires. `damage` and `cooldown_ticks` here are the
    /// enemies'; the fields above are the player's.
    pub enemy: EnemyTuning,
    /// The player's part the enemies aim at.
    pub enemy_aim_at: BodyPart,
    /// How far and how fast the player leans, and how much it slows walking.
    pub lean: LeanTuning,
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

/// The six hit volumes of the shipped-body player, the same boxes as a dummy's but tagged
/// [`SurfaceTag::Player`].
pub fn player_hit_volumes() -> [HitVolume; 6] {
    body_volumes(&SHIPPED_BODY, SurfaceTag::Player)
}

/// The six hit volumes of a shipped-body dummy numbered `target`, one per body part, in the
/// body frame around the capsule centre. Each is tagged `SurfaceTag::Part` for its part.
///
/// The body faces +Z at yaw 0, so its left side is +X. The head sits directly on the torso
/// and the legs under it, reaching the feet of the hull, so a level ray from the player's eye
/// height meets a part rather than passing through a gap at the neck.
pub fn dummy_hit_volumes(target: u16) -> [HitVolume; 6] {
    dummy_hit_volumes_of(&SHIPPED_BODY, target)
}

/// [`dummy_hit_volumes`] for a dummy built on `body`, whose legs reach the feet of its hull.
pub(crate) fn dummy_hit_volumes_of(body: &CharacterBody, target: u16) -> [HitVolume; 6] {
    body_volumes(body, |part| SurfaceTag::Part { target, part })
}

/// Torso box centre height above the capsule centre, in metres.
const TORSO_CENTRE_Y: f32 = 0.25;

/// Torso box half height, in metres.
const TORSO_HALF_Y: f32 = 0.35;

/// The six hit volumes every body shares, each tagged by `tag_of` for its part. The legs run
/// from the torso's bottom to the feet of `body`'s hull.
fn body_volumes(body: &CharacterBody, tag_of: impl Fn(BodyPart) -> SurfaceTag) -> [HitVolume; 6] {
    let volume = |part, offset: Vec3, half_extents: Vec3| HitVolume {
        offset,
        rotation: Quat::IDENTITY,
        half_extents,
        tag: tag_of(part).into(),
    };
    let arm = Vec3::new(0.1, 0.3, 0.1);
    let torso_bottom = TORSO_CENTRE_Y - TORSO_HALF_Y;
    let feet = -body.standing.hull.length / 2.0;
    let leg_centre_y = (torso_bottom + feet) / 2.0;
    let leg = Vec3::new(0.1, (torso_bottom - feet) / 2.0, 0.1);
    [
        volume(
            BodyPart::Head,
            Vec3::new(0.0, 0.72, 0.0),
            Vec3::new(0.12, 0.12, 0.12),
        ),
        volume(
            BodyPart::Torso,
            Vec3::new(0.0, TORSO_CENTRE_Y, 0.0),
            Vec3::new(0.2, TORSO_HALF_Y, 0.12),
        ),
        volume(BodyPart::LeftArm, Vec3::new(0.3, 0.25, 0.0), arm),
        volume(BodyPart::RightArm, Vec3::new(-0.3, 0.25, 0.0), arm),
        volume(BodyPart::LeftLeg, Vec3::new(0.12, leg_centre_y, 0.0), leg),
        volume(BodyPart::RightLeg, Vec3::new(-0.12, leg_centre_y, 0.0), leg),
    ]
}

/// What the rifle is doing; one thing at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weapon {
    Ready,
    /// A shot was just fired; ready again when the count of ticks left runs out.
    Cooling(u32),
    /// A reload is under way; done when the count of ticks left runs out.
    Reloading(u32),
}

/// A dummy or an enemy: a character with hit volumes and a body. Enemies also think.
#[derive(Debug)]
struct Target {
    /// `None` once an enemy died and left the world.
    handle: Option<CharacterHandle>,
    body: Body,
    /// `None` for a dummy.
    enemy: Option<Enemy>,
    /// Where the player stood when it put this enemy under fire, by a non-lethal hit or a near
    /// miss that reached the grade's count; read by the next percept step.
    under_fire: Option<Vec3>,
    /// Near misses taken since the last time this enemy was put under fire.
    near_misses: u32,
}

#[derive(Clone, Copy, Debug)]
struct Enemy {
    brain: Brain,
    grade: Grade,
}

/// The engine stream enemy aim error draws from; later draws add streams of their own.
const SPREAD_STREAM: StreamId = StreamId::named("searchlight.enemy_spread");

/// The next draw of `rng`, uniform in `[-1, 1)`.
fn next_signed(rng: &mut SimRng) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// The world, the player, the rifle, the dummies and the enemies.
#[derive(Debug)]
pub struct SearchlightSim {
    world: PhysicsWorld,
    player: CharacterHandle,
    move_settings: MoveSettings,
    /// The hull and eye of the player and every target.
    body: CharacterBody,
    range: f32,
    damage: u32,
    cooldown_ticks: u32,
    reload_ticks: u32,
    charge_ticks: u32,
    loadout: Loadout,
    /// Indexed by target number: the dummies, then the enemies.
    targets: Vec<Target>,
    dummy_count: usize,
    player_body: Body,
    player_max_health: u32,
    route: Vec<Vec3>,
    covers: Vec<Vec3>,
    /// The arena's walkable area; empty for a sim without enemies.
    nav: Arc<NavMesh>,
    enemy: EnemyTuning,
    /// Offset from the player's capsule centre to the part the enemies aim at, before the
    /// player's yaw is applied.
    aim_offset: Vec3,
    rng: SimRng,
    weapon: Weapon,
    lean: Lean,
    lean_tuning: LeanTuning,
    outbox: Vec<Feedback>,
    /// Lethal player shots on enemies since the sim was built or last restarted.
    kills: u32,
    /// What a restart rebuilds the characters from, in the same world.
    origin: (Spawns, SimConfig),
}

/// Where a sim's characters start.
#[derive(Clone, Debug)]
struct Spawns {
    player: Vec3,
    dummies: Vec<Vec3>,
    /// Enemies standing from the start, a prefix of the targets after the dummies.
    enemies: Vec<Vec3>,
    /// Every enemy spawn marker the arena has, whether or not an enemy starts on it.
    markers: Vec<Vec3>,
    route: Vec<Vec3>,
    covers: Vec<Vec3>,
    nav: Arc<NavMesh>,
}

/// A target standing on the floor point `spawn` as target number `index`. With a route and a
/// grade it is an enemy of that grade, facing the nearest waypoint; without, a dummy.
fn new_target(
    world: &mut PhysicsWorld,
    index: usize,
    spawn: Vec3,
    enemy: Option<(&[Vec3], Grade)>,
    profile: BodyProfile,
    body: CharacterBody,
) -> Target {
    let target = target_number(index);
    let centre = standing_centre(&body, spawn);
    let (enemy, yaw) = match enemy {
        Some((route, grade)) => {
            let (nearest, yaw) = nearest_waypoint(spawn, route);
            let brain = Brain::Patrol { next: nearest };
            (Some(Enemy { brain, grade }), yaw)
        }
        None => (None, 0.0),
    };
    let handle = world.add_character(CharacterSpawn {
        position: centre,
        yaw,
        pitch: 0.0,
        body,
    });
    world
        .set_hit_volumes(
            handle,
            &body_volumes(&body, |part| SurfaceTag::Part { target, part }),
        )
        .expect("the target was added a moment ago");
    Target {
        handle: Some(handle),
        body: Body::new(profile),
        enemy,
        under_fire: None,
        near_misses: 0,
    }
}

impl SearchlightSim {
    /// A sim over `world` (static geometry already in), the player standing at
    /// `player_spawn` and one dummy at each of `dummy_spawns`, numbered by position in the
    /// slice. Spawns are floor points: capsule centres sit half the configured hull above them.
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
            markers: Vec::new(),
            route: Vec::new(),
            covers: Vec::new(),
            nav: Arc::default(),
        };
        Self::assemble(world, spawns, config)
    }

    fn assemble(mut world: PhysicsWorld, spawns: Spawns, config: SimConfig) -> Self {
        world.set_gravity(config.gravity);
        let player = world.add_character(CharacterSpawn {
            body: config.character_body,
            ..CharacterSpawn::from(standing_centre(&config.character_body, spawns.player))
        });
        let player_volumes = body_volumes(&config.character_body, SurfaceTag::Player);
        world
            .set_hit_volumes(player, &player_volumes)
            .expect("the player was added a moment ago");
        let aim_offset = player_volumes
            .iter()
            .find(|volume| volume.tag == SurfaceTag::Player(config.enemy_aim_at).into())
            .expect("the player has a hit volume for every part")
            .offset;
        let mut targets = Vec::new();
        for (index, &spawn) in spawns.dummies.iter().chain(&spawns.enemies).enumerate() {
            let enemy = (index >= spawns.dummies.len())
                .then_some((spawns.route.as_slice(), Grade::default()));
            targets.push(new_target(
                &mut world,
                index,
                spawn,
                enemy,
                config.body,
                config.character_body,
            ));
        }
        // Index the new hit volumes so the first tick's rays can strike them.
        world.step(SETTLE_DT);
        Self {
            world,
            player,
            move_settings: config.move_settings,
            body: config.character_body,
            range: config.range,
            damage: config.damage,
            cooldown_ticks: config.cooldown_ticks,
            reload_ticks: config.reload_ticks,
            charge_ticks: config.charge_ticks,
            loadout: config.loadout.clone(),
            targets,
            dummy_count: spawns.dummies.len(),
            player_body: Body::new(config.body),
            player_max_health: config.body.max_health,
            route: spawns.route.clone(),
            covers: spawns.covers.clone(),
            nav: spawns.nav.clone(),
            enemy: config.enemy,
            aim_offset,
            rng: SimRng::new(config.enemy.seed, SPREAD_STREAM),
            weapon: Weapon::Ready,
            lean: Lean::default(),
            lean_tuning: config.lean,
            outbox: Vec::new(),
            kills: 0,
            origin: (spawns, config),
        }
    }

    /// Runs one tick of `dt` seconds.
    ///
    /// While the player is dead only `command.restart` is read, and it puts every character
    /// back at its spawn, as a fresh sim over the same world would have them. Otherwise the enemies perceive and move, the player moves (slower while a lean is held), the
    /// world steps, the player's lean blends within the room beside it, the player's weapon actions apply, then the enemies that decided to fire
    /// do, and enemies that died leave the world.
    pub fn tick(&mut self, command: &SearchlightCommand, dt: f32) {
        if self.player_body.is_dead() {
            if command.restart {
                self.restart();
            }
            return;
        }
        let firing = self.move_enemies(dt);
        let factor = if command.lean.is_some() {
            self.lean_tuning.speed_factor
        } else {
            1.0
        };
        let settings = MoveSettings {
            max_speed: self.move_settings.max_speed * factor,
            ..self.move_settings
        };
        self.world
            .move_character(self.player, &command.movement, &settings, dt)
            .expect("the sim never removes the player");
        self.world.step(dt);
        self.step_lean(command.lean, dt);

        self.advance_weapon();
        if command.fire {
            self.pull_trigger();
        }
        if command.reload {
            self.begin_reload();
        }
        if command.check {
            self.outbox
                .push(Feedback::MagazineLevel(self.rifle().magazine().level()));
        }

        self.fire_enemies(&firing, command.god_mode);
        self.remove_dead_enemies();
        if self.player_body.is_dead() {
            self.outbox.push(Feedback::PlayerDied);
        }
    }

    /// Blends the lean toward `want`, held short of any wall beside the player.
    fn step_lean(&mut self, want: Option<LeanSide>, dt: f32) {
        let reach = if want.is_none() && self.lean.amount() == 0.0 {
            LeanReach::FULL
        } else {
            self.lean_reach()
        };
        self.lean = self.lean.step(want, reach, dt, &self.lean_tuning);
    }

    /// How much of a full lean each side has room for, from one level ray per side out of the
    /// upright eye.
    fn lean_reach(&self) -> LeanReach {
        let state = self.player();
        let eye = state.eye_position();
        let shift = self.lean_tuning.shift;
        let room = |direction: Vec3| {
            let strike = resolve_shot(
                &self.world,
                ShotRay {
                    origin: eye,
                    direction,
                    range: shift + LEAN_CLEARANCE,
                    ignore: Some(self.player),
                },
            );
            match strike {
                Some(strike) if shift > 0.0 => {
                    ((strike.distance - LEAN_CLEARANCE) / shift).clamp(0.0, 1.0)
                }
                _ => 1.0,
            }
        };
        let right = lean_offset(state.yaw, 1.0);
        LeanReach {
            left: room(-right),
            right: room(right),
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
            eye: self.player().eye_position(),
            aim,
        };
        let mut claims: Vec<Option<usize>> = self
            .targets
            .iter()
            .map(|target| {
                target
                    .handle
                    .and(target.enemy)
                    .and_then(|enemy| enemy.brain.claim())
            })
            .collect();
        let mut firing = Vec::new();
        let nav = &self.nav;
        let eye_height = self.eye_height();
        for (index, target) in self.targets.iter_mut().enumerate().skip(self.dummy_count) {
            let (Some(handle), Some(enemy)) = (target.handle, target.enemy.as_mut()) else {
                continue;
            };
            let tuning = self.enemy.graded(enemy.grade);
            let state = self
                .world
                .character(handle)
                .expect("a living enemy is in the world");
            let eye = state.eye_position();
            let sees_player = resolve_shot(
                &self.world,
                ShotRay {
                    origin: eye,
                    direction: aim - eye,
                    range: tuning.sight_range,
                    ignore: Some(handle),
                },
            )
            .is_some_and(|strike| matches!(strike.tag, SurfaceTag::Player(_)));
            let under_fire = target.under_fire.take();
            let offer = under_fire.and_then(|_| {
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
                    eye_height,
                    &tuning,
                )
            });
            let percept = Percept {
                position: state.position,
                yaw: state.yaw,
                target: sees_player.then_some(aim),
                under_fire,
                offer,
            };
            let position = state.position;
            let world = &self.world;
            let steer = |goal| {
                nav.path(position, cover::floor_below(world, goal))
                    .and_then(|path| path.get(1).copied())
            };
            let intent = enemy
                .brain
                .think(&self.route, &percept, &tuning, &steer, dt);
            claims[index] = enemy.brain.claim();
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
    fn fire_enemies(&mut self, firing: &[usize], god_mode: bool) {
        for &index in firing {
            if self.player_body.is_dead() {
                break;
            }
            let target = &self.targets[index];
            let Some(handle) = target.handle.filter(|_| !target.body.is_dead()) else {
                continue;
            };
            let enemy = target.enemy.expect("only enemies fire");
            let tuning = self.enemy.graded(enemy.grade);
            let state = self
                .world
                .character(handle)
                .expect("a living enemy is in the world");
            let eye = state.eye_position();
            let direction = self.with_spread(self.player_aim_point() - eye, tuning.aim_error);
            self.outbox.push(Feedback::EnemyFired {
                target: u16::try_from(index).expect("target numbers fit a u16"),
            });
            let strike = resolve_shot(
                &self.world,
                ShotRay {
                    origin: eye,
                    direction,
                    range: tuning.sight_range,
                    ignore: Some(handle),
                },
            );
            if let Some(Strike {
                tag: SurfaceTag::Player(part),
                ..
            }) = strike
            {
                if !god_mode {
                    self.player_body.hit(part, tuning.damage);
                }
                self.outbox.push(Feedback::PlayerHit { part, from: eye });
            }
        }
    }

    /// `direction` turned off by up to `aim_error` radians either way across and up.
    fn with_spread(&mut self, direction: Vec3, aim_error: f32) -> Vec3 {
        let forward = direction.normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let across = next_signed(&mut self.rng) * aim_error;
        let rise = next_signed(&mut self.rng) * aim_error;
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
        match self.loadout.fire() {
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
        let view = first_person_view(&state, &state, 1.0, (0.0, 0.0));
        let origin = self.leaned_eye(&state);
        let strike = resolve_shot(
            &self.world,
            ShotRay {
                origin,
                direction: view.forward,
                range: self.range,
                ignore: Some(self.player),
            },
        );
        let end = strike.map_or(origin + view.forward * self.range, |strike| strike.point);
        let hit = strike.and_then(|strike| match strike.tag {
            SurfaceTag::Part { target, part } => Some((target, part)),
            _ => None,
        });
        let struck = hit.map(|(target, _)| usize::from(target));
        self.record_near_misses(origin, end, struck, state.position);
        if let Some((target, part)) = hit {
            self.apply_player_hit(target, part, self.damage, state.position);
        }
    }

    /// Counts a near miss for every living enemy but `struck` that the shot from `origin` to
    /// `end` passed within the near-miss radius of, and puts those whose count reached their
    /// grade's under fire from `from`.
    fn record_near_misses(&mut self, origin: Vec3, end: Vec3, struck: Option<usize>, from: Vec3) {
        let radius = self.enemy.near_miss_radius;
        for (index, target) in self.targets.iter_mut().enumerate().skip(self.dummy_count) {
            let (Some(handle), Some(enemy)) = (target.handle, target.enemy) else {
                continue;
            };
            if struck == Some(index) {
                continue;
            }
            let Some(needed) = enemy.grade.near_misses_to_cover() else {
                continue;
            };
            let centre = self
                .world
                .character(handle)
                .expect("a living enemy is in the world")
                .position;
            if !passes_within(origin, end, centre, radius) {
                continue;
            }
            target.near_misses += 1;
            if target.near_misses >= needed {
                target.near_misses = 0;
                target.under_fire = Some(from);
            }
        }
    }

    /// Lands a player's hit of `damage` on `part` of target `target`, shot from `from`: wounds
    /// the body, alerts a surviving enemy, counts a kill and reports the hit.
    fn apply_player_hit(&mut self, target: u16, part: BodyPart, damage: u32, from: Vec3) {
        let Some(struck) = self.targets.get_mut(usize::from(target)) else {
            tracing::warn!(target, "shot struck a target with no body, dropped");
            return;
        };
        let was_dead = struck.body.is_dead();
        let report = struck.body.hit(part, damage);
        let is_dead = struck.body.is_dead();
        if struck.enemy.is_some() {
            if !is_dead {
                struck.under_fire = Some(from);
            } else if !was_dead {
                self.kills += 1;
            }
        }
        self.outbox.push(Feedback::Hit {
            target,
            part,
            report,
        });
    }

    fn begin_reload(&mut self) {
        if matches!(self.weapon, Weapon::Reloading(_)) || !self.loadout.can_reload() {
            return;
        }
        let total = if self.loadout.rifle().is_chambered() {
            self.reload_ticks
        } else {
            self.reload_ticks.saturating_add(self.charge_ticks)
        };
        if total == 0 {
            self.finish_reload();
        } else {
            self.weapon = Weapon::Reloading(total);
        }
    }

    /// Swaps in the next magazine from the rig; the ejected one takes its slot.
    fn finish_reload(&mut self) {
        if self.loadout.reload() {
            self.outbox.push(Feedback::ReloadDone);
        }
    }

    /// The player's state after the last tick.
    pub fn player(&self) -> CharacterState {
        self.world
            .character(self.player)
            .expect("the sim never removes the player")
    }

    /// Height of the standing eye above the feet, in metres.
    pub fn eye_height(&self) -> f32 {
        self.body.standing.eye_height
    }

    /// Where the player's eye is after the last tick: the upright eye shifted sideways by the
    /// lean.
    pub fn player_eye(&self) -> Vec3 {
        self.leaned_eye(&self.player())
    }

    fn leaned_eye(&self, state: &CharacterState) -> Vec3 {
        state.eye_position() + lean_offset(state.yaw, self.lean.shift(&self.lean_tuning))
    }

    /// The control the player is looking at within [`USE_REACH`], or `None` when none is, a wall
    /// or body is in front of it, or the player is dead.
    pub fn control_in_reach(&self) -> Option<u16> {
        None
    }

    /// How far the player leans after the last tick.
    pub fn player_lean(&self) -> Lean {
        self.lean
    }

    /// What a full lean looks like and how it blends.
    pub fn lean_tuning(&self) -> &LeanTuning {
        &self.lean_tuning
    }

    pub fn rifle(&self) -> &Rifle {
        self.loadout.rifle()
    }

    /// The rig the player reloads from.
    pub fn rig(&self) -> &Rig {
        self.loadout.rig()
    }

    /// The body of dummy `target`, or `None` if there is no such dummy.
    pub fn dummy_body(&self, target: u16) -> Option<&Body> {
        self.targets[..self.dummy_count]
            .get(usize::from(target))
            .map(|dummy| &dummy.body)
    }

    /// A sim started from `arena` as [`Self::start`] does, except that with [`Start::Empty`]
    /// no enemy stands at the markers; a restart rebuilds through the same `start`.
    pub fn with_start(arena: Arc<Arena>, config: SimConfig, start: Start) -> Self {
        let spawns = Spawns {
            player: arena.player_spawn,
            dummies: arena.dummy_spawns.clone(),
            enemies: match start {
                Start::AtMarkers => arena.enemy_spawns.clone(),
                Start::Empty => Vec::new(),
            },
            markers: arena.enemy_spawns.clone(),
            route: arena.route.clone(),
            covers: arena.covers.clone(),
            nav: Arc::clone(&arena.nav),
        };
        Self::assemble(arena.physics_world(), spawns, config)
    }

    /// Adds one enemy of `grade` at enemy spawn marker `marker`, set up like a marker-start
    /// enemy.
    ///
    /// # Errors
    /// If there is no such marker, a character stands on it, or the new enemy's target number
    /// does not fit a `u16`.
    pub fn spawn_enemy(&mut self, marker: usize, grade: Grade) -> Result<(), SpawnError> {
        let spawn = *self
            .origin
            .0
            .markers
            .get(marker)
            .ok_or(SpawnError::UnknownMarker { marker })?;
        if !self.marker_is_free(spawn, &self.occupant_positions()) {
            return Err(SpawnError::OccupiedMarker { marker });
        }
        let index = self.targets.len();
        u16::try_from(index).map_err(|_| SpawnError::TooManyTargets)?;
        let target = new_target(
            &mut self.world,
            index,
            spawn,
            Some((&self.route, grade)),
            self.origin.1.body,
            self.body,
        );
        self.targets.push(target);
        // Index the new hit volumes so the next tick's rays can strike them.
        self.world.step(SETTLE_DT);
        Ok(())
    }

    /// Enemy spawn markers, ascending, with no living character within two capsule radii.
    pub fn free_enemy_spawns(&self) -> Vec<usize> {
        let occupants = self.occupant_positions();
        self.origin
            .0
            .markers
            .iter()
            .enumerate()
            .filter(|(_, marker)| self.marker_is_free(**marker, &occupants))
            .map(|(index, _)| index)
            .collect()
    }

    /// Positions of the living characters, the player included.
    fn occupant_positions(&self) -> Vec<Vec3> {
        self.targets
            .iter()
            .filter_map(|target| target.handle)
            .chain([self.player])
            .filter_map(|handle| self.world.character(handle))
            .map(|state| state.position)
            .collect()
    }

    /// Whether no occupant stands within two hull radii of `marker`, measured on the ground plane.
    fn marker_is_free(&self, marker: Vec3, occupants: &[Vec3]) -> bool {
        let clearance = 2.0 * self.body.standing.hull.radius;
        occupants
            .iter()
            .all(|position| (position.x - marker.x).hypot(position.z - marker.z) >= clearance)
    }

    /// The hull and eye the player and every target were built with.
    pub fn character_body(&self) -> &CharacterBody {
        &self.body
    }

    /// Enemies still in the world.
    pub fn enemies_alive(&self) -> usize {
        self.targets[self.dummy_count..]
            .iter()
            .filter(|enemy| enemy.handle.is_some())
            .count()
    }

    /// Enemies the player's shots have killed since the sim was built or last restarted.
    pub fn kills(&self) -> u32 {
        self.kills
    }

    /// Applies a lethal hit to enemy `index` (enemy order) as a player's shot would.
    #[cfg(test)]
    pub(crate) fn kill_enemy(&mut self, index: usize) {
        let target = u16::try_from(self.dummy_count + index).expect("target numbers fit a u16");
        let from = self.player().position;
        self.apply_player_hit(target, BodyPart::Torso, 1_000_000, from);
    }

    /// Kills the player outright.
    #[cfg(test)]
    pub(crate) fn kill_player(&mut self) {
        self.player_body.hit(BodyPart::Torso, 1_000_000);
    }

    /// A sim started from `arena`: the player, one dummy per dummy spawn and one enemy per enemy
    /// spawn, numbered dummies first. Every enemy starts facing its nearest waypoint.
    ///
    /// # Panics
    /// If the arena has an enemy spawn but no route, or more targets than a `u16` can number.
    pub fn start(arena: Arc<Arena>, config: SimConfig) -> Self {
        Self::with_start(arena, config, Start::AtMarkers)
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

    /// The grade enemy `index` (enemy order) was spawned with, dead or alive, or `None` if
    /// there is no such enemy.
    pub fn enemy_grade(&self, index: usize) -> Option<Grade> {
        self.targets
            .get(self.dummy_count + index)?
            .enemy
            .map(|enemy| enemy.grade)
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
    use searchlight_rules::{ItemId, Loadout, Magazine, MagazineLevel, Rig, RigSpec};

    use super::*;
    use crate::tag::SurfaceTag;
    use crate::test_support::{
        DT, arena_with_controls, flat_floor, foreign_mag, mag, rifle_from, room, test_config,
    };

    fn rifle_with(rounds: u32) -> Rifle {
        rifle_from(mag(30, rounds))
    }

    fn part_of(volume: &HitVolume) -> BodyPart {
        match SurfaceTag::try_from(volume.tag).expect("a body volume carries a body tag") {
            SurfaceTag::Part { part, .. } | SurfaceTag::Player(part) => part,
            other @ (SurfaceTag::World | SurfaceTag::Control(_)) => {
                panic!("not a body part: {other:?}")
            }
        }
    }

    #[test]
    fn body_volumes_fit_inside_the_hull() {
        const HALF_LENGTH: f32 = 0.915;
        const RADIUS: f32 = 0.405;
        const TOLERANCE: f32 = 1e-4;
        let sets = [
            ("player", player_hit_volumes()),
            ("dummy", dummy_hit_volumes(0)),
        ];

        for (who, volumes) in sets {
            let mut legs = 0;
            let mut head_bottom = f32::MAX;
            let mut torso_top = f32::MIN;
            for volume in &volumes {
                assert_eq!(
                    volume.rotation,
                    Quat::IDENTITY,
                    "{who}: boxes are axis aligned"
                );
                let (centre, half) = (volume.offset, volume.half_extents);
                assert!(
                    centre.x.abs() + half.x <= RADIUS + TOLERANCE
                        && centre.z.abs() + half.z <= RADIUS + TOLERANCE,
                    "{who}: {volume:?} sticks out of the {RADIUS} m radius"
                );
                assert!(
                    centre.y + half.y <= HALF_LENGTH + TOLERANCE,
                    "{who}: {volume:?} rises above the {HALF_LENGTH} m half length"
                );
                assert!(
                    centre.y - half.y >= -HALF_LENGTH - TOLERANCE,
                    "{who}: {volume:?} sinks below the feet"
                );
                match part_of(volume) {
                    BodyPart::Head => head_bottom = centre.y - half.y,
                    BodyPart::Torso => torso_top = centre.y + half.y,
                    BodyPart::LeftLeg | BodyPart::RightLeg => {
                        legs += 1;
                        let bottom = centre.y - half.y;
                        assert!(
                            (bottom + HALF_LENGTH).abs() <= TOLERANCE,
                            "{who}: a leg ends at {bottom}, not at the feet (-{HALF_LENGTH})"
                        );
                    }
                    BodyPart::LeftArm | BodyPart::RightArm => {}
                }
            }
            assert_eq!(legs, 2, "{who}: both legs have a volume");
            assert!(
                (head_bottom - torso_top).abs() <= TOLERANCE,
                "{who}: the head ({head_bottom}) sits directly on the torso ({torso_top})"
            );
        }
    }

    fn empty_rig(slots: u32) -> Rig {
        Rig::new(
            ItemId::new("test_rig"),
            &RigSpec {
                magazine_slots: slots,
            },
        )
    }

    fn idle() -> SearchlightCommand {
        SearchlightCommand::default()
    }

    fn pull() -> SearchlightCommand {
        SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn reload() -> SearchlightCommand {
        SearchlightCommand {
            reload: true,
            ..SearchlightCommand::default()
        }
    }

    fn check() -> SearchlightCommand {
        SearchlightCommand {
            check: true,
            ..SearchlightCommand::default()
        }
    }

    fn arena_sim(rifle: Rifle, spares: Vec<Magazine>) -> SearchlightSim {
        SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], test_config(rifle, spares))
    }

    /// Ticks once per command in `script`, draining feedback after each tick.
    fn run(sim: &mut SearchlightSim, script: &[SearchlightCommand]) -> Vec<Vec<Feedback>> {
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

        let sim = SearchlightSim::new(flat_floor(), spawn, &[], test_config(rifle_with(5), vec![]));

        let at = sim.player().position;
        assert!(
            (at.x - spawn.x).abs() < 1e-3 && (at.z - spawn.z).abs() < 1e-3,
            "{at:?}"
        );
        assert!((at.y - (spawn.y + STAND_HEIGHT)).abs() < 1e-3, "{at:?}");
    }

    #[test]
    fn the_configured_gravity_reaches_the_world() {
        let mut config = test_config(rifle_with(5), vec![]);
        config.gravity = 20.32;

        let sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);

        assert_eq!(sim.world.gravity(), 20.32);
    }

    #[test]
    fn player_cannot_walk_out_of_the_arena() {
        for turns in 0..4 {
            let yaw = FRAC_PI_2 * turns as f32;
            let mut sim =
                SearchlightSim::new(room(), Vec3::ZERO, &[], test_config(rifle_with(5), vec![]));
            let walk = |yaw_delta| SearchlightCommand {
                movement: CharacterCommand {
                    forward: 1.0,
                    yaw_delta,
                    ..CharacterCommand::default()
                },
                ..SearchlightCommand::default()
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

    /// A sim over a flat floor with `boxes` as walls and `controls` as next-wave posts, the
    /// player at the origin facing +Z and no enemies.
    fn control_sim(boxes: &[(Vec3, Vec3)], controls: &[(Vec3, Vec3)]) -> SearchlightSim {
        let arena = arena_with_controls(boxes, Vec3::ZERO, &[], &[], controls);
        SearchlightSim::with_start(arena, test_config(rifle_with(30), vec![]), Start::Empty)
    }

    /// Like [`control_sim`] with a dummy standing on `dummy`.
    fn control_sim_with_dummy(controls: &[(Vec3, Vec3)], dummy: Vec3) -> SearchlightSim {
        let arena = arena_with_controls(&[], Vec3::ZERO, &[], &[], controls);
        let arena = Arc::new(Arena {
            dummy_spawns: vec![dummy],
            ..(*arena).clone()
        });
        SearchlightSim::with_start(arena, test_config(rifle_with(30), vec![]), Start::Empty)
    }

    /// A post 0.6 m wide and 2 m tall whose near face is `distance` m in front of the origin.
    fn post_at(distance: f32) -> (Vec3, Vec3) {
        (
            Vec3::new(-0.3, 0.0, distance),
            Vec3::new(0.3, 2.0, distance + 0.4),
        )
    }

    /// A post just behind the player at the origin.
    fn post_behind() -> (Vec3, Vec3) {
        (Vec3::new(-0.3, 0.0, -1.4), Vec3::new(0.3, 2.0, -1.0))
    }

    fn turning(yaw_delta: f32, pitch_delta: f32) -> SearchlightCommand {
        SearchlightCommand {
            movement: CharacterCommand {
                yaw_delta,
                pitch_delta,
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        }
    }

    #[test]
    fn a_control_in_view_and_reach_is_found() {
        let mut sim = control_sim(&[], &[post_behind(), post_at(1.0)]);

        sim.tick(&idle(), DT);

        assert_eq!(sim.control_in_reach(), Some(1));
    }

    #[test]
    fn a_control_out_of_view_is_not_found() {
        let mut sim = control_sim(&[], &[post_at(1.0)]);
        let mut looking_up = control_sim(&[], &[post_at(1.0)]);
        sim.tick(&idle(), DT);
        let in_view = sim.control_in_reach();

        sim.tick(&turning(std::f32::consts::PI, 0.0), DT);
        looking_up.tick(&turning(0.0, 1.0), DT);

        assert_eq!(in_view, Some(0));
        assert_eq!(sim.control_in_reach(), None, "facing away");
        assert_eq!(looking_up.control_in_reach(), None, "looking over it");
    }

    #[test]
    fn a_control_beyond_reach_is_not_found() {
        let mut near = control_sim(&[], &[post_at(1.9)]);
        let mut far = control_sim(&[], &[post_at(2.1)]);

        near.tick(&idle(), DT);
        far.tick(&idle(), DT);

        assert_eq!(near.control_in_reach(), Some(0));
        assert_eq!(far.control_in_reach(), None);
    }

    #[test]
    fn a_control_behind_a_wall_is_not_found() {
        let wall = (Vec3::new(-2.0, 0.0, 0.8), Vec3::new(2.0, 4.0, 0.9));
        let mut open = control_sim(&[], &[post_at(1.2)]);
        let mut walled = control_sim(&[wall], &[post_at(1.2)]);

        open.tick(&idle(), DT);
        walled.tick(&idle(), DT);

        assert_eq!(open.control_in_reach(), Some(0));
        assert_eq!(walled.control_in_reach(), None);
    }

    #[test]
    fn a_body_in_front_of_a_control_blocks_it() {
        let mut open = control_sim(&[], &[post_at(1.6)]);
        let mut blocked = control_sim_with_dummy(&[post_at(1.6)], Vec3::new(0.0, 0.0, 1.0));
        open.tick(&idle(), DT);
        blocked.tick(&idle(), DT);

        assert_eq!(open.control_in_reach(), Some(0));
        assert_eq!(blocked.control_in_reach(), None);
    }

    #[test]
    fn a_dead_player_finds_no_control() {
        let mut sim = control_sim(&[], &[post_at(1.0)]);
        sim.tick(&idle(), DT);
        let alive = sim.control_in_reach();

        sim.kill_player();
        sim.tick(&idle(), DT);

        assert_eq!(alive, Some(0));
        assert_eq!(sim.control_in_reach(), None);
    }

    #[test]
    fn a_shot_at_a_control_damages_nothing() {
        let behind_the_post = Vec3::new(0.0, 0.0, 4.0);
        let mut open = control_sim_with_dummy(&[], behind_the_post);
        let mut shielded = control_sim_with_dummy(&[post_at(1.0)], behind_the_post);
        let health = shielded.dummy_body(0).expect("a dummy stands").health();
        let player_health = shielded.player_body().health();

        let open_ticks = run(&mut open, &[pull()]);
        let shielded_ticks = run(&mut shielded, &[pull()]);

        let is_hit = |feedback: &Feedback| matches!(feedback, Feedback::Hit { .. });
        assert!(
            open_ticks[0].iter().any(is_hit),
            "the dummy is shootable without the post: {open_ticks:?}"
        );
        assert!(shielded_ticks[0].contains(&Feedback::Fired));
        assert!(!shielded_ticks[0].iter().any(is_hit), "{shielded_ticks:?}");
        assert_eq!(
            shielded.dummy_body(0).expect("a dummy stands").health(),
            health
        );
        assert_eq!(shielded.player_body().health(), player_health);
        assert_eq!(shielded.kills(), 0);
    }

    #[test]
    fn trigger_pulls_spend_rounds_and_report_dry() {
        let mut sim = arena_sim(rifle_with(3), vec![]);
        let spaced = [pull(), idle(), idle()];
        let script: Vec<SearchlightCommand> = spaced.iter().cycle().take(10).copied().collect();

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
        let mut rifle = rifle_from(mag(30, 1));
        rifle.reload(mag(30, 30)).expect("the test magazine fits");
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

    fn charging_sim(
        rifle: Rifle,
        spares: Vec<Magazine>,
        reload_ticks: u32,
        charge_ticks: u32,
    ) -> SearchlightSim {
        let mut config = test_config(rifle, spares);
        config.reload_ticks = reload_ticks;
        config.charge_ticks = charge_ticks;
        SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config)
    }

    fn reload_wait_pull(idles: usize) -> Vec<SearchlightCommand> {
        let mut script = vec![reload()];
        script.extend(std::iter::repeat_n(idle(), idles));
        script.push(pull());
        script
    }

    #[test]
    fn the_rifle_cannot_fire_while_reloading() {
        let cases = [
            (rifle_with(10), 10, 30, mag(30, 9)),
            (rifle_with(0), 14, 29, mag(30, 0)),
        ];
        for (rifle, duration, rounds, ejected) in cases {
            let chambered = rifle.is_chambered();
            let mut sim = charging_sim(rifle, vec![mag(30, 30)], 10, 4);
            let mut script = vec![reload()];
            script.extend(std::iter::repeat_n(pull(), duration - 1));
            script.push(idle());

            let ticks = run(&mut sim, &script);

            assert!(
                fired_on(&ticks).is_empty(),
                "chambered {chambered}: fired during the reload"
            );
            assert!(
                ticks.iter().flatten().all(|fb| *fb != Feedback::Dry),
                "chambered {chambered}: dry feedback during the reload"
            );
            assert_eq!(
                ticks[duration],
                [Feedback::ReloadDone],
                "chambered {chambered}"
            );
            assert_eq!(
                sim.rifle().magazine().rounds(),
                rounds,
                "chambered {chambered}"
            );
            assert_eq!(sim.rig().slots(), [Some(ejected)], "chambered {chambered}");
            assert!(sim.rifle().is_chambered());
        }
    }

    #[test]
    fn a_tactical_reload_ends_on_exactly_the_reload_tick() {
        let mut early = charging_sim(rifle_with(10), vec![mag(30, 30)], 10, 4);
        let mut on_time = charging_sim(rifle_with(10), vec![mag(30, 30)], 10, 4);

        let before = run(&mut early, &reload_wait_pull(8));
        let after = run(&mut on_time, &reload_wait_pull(9));

        assert!(fired_on(&before).is_empty());
        assert_eq!(before[8], []);
        assert_eq!(after[10], [Feedback::ReloadDone, Feedback::Fired]);
        assert_eq!(sim_rounds(&on_time), (true, 29));
    }

    fn sim_rounds(sim: &SearchlightSim) -> (bool, u32) {
        (sim.rifle().is_chambered(), sim.rifle().magazine().rounds())
    }

    #[test]
    fn an_empty_chamber_reload_ends_after_reload_plus_charge_ticks() {
        let mut early = charging_sim(rifle_with(0), vec![mag(30, 30)], 10, 4);
        let mut on_time = charging_sim(rifle_with(0), vec![mag(30, 30)], 10, 4);

        let before = run(&mut early, &reload_wait_pull(12));
        let after = run(&mut on_time, &reload_wait_pull(13));

        assert!(before.iter().flatten().next().is_none(), "{before:?}");
        assert_eq!(after[..14], vec![vec![]; 14][..]);
        assert_eq!(after[14], [Feedback::ReloadDone, Feedback::Fired]);
        assert_eq!(sim_rounds(&on_time), (true, 28));
    }

    #[test]
    fn a_chambered_round_with_an_empty_magazine_reloads_tactically() {
        let mut early = charging_sim(rifle_with(1), vec![mag(30, 30)], 10, 4);
        let mut on_time = charging_sim(rifle_with(1), vec![mag(30, 30)], 10, 4);
        assert_eq!(sim_rounds(&early), (true, 0));

        let before = run(&mut early, &reload_wait_pull(8));
        let after = run(&mut on_time, &reload_wait_pull(9));

        assert!(fired_on(&before).is_empty());
        assert_eq!(before[8], []);
        assert_eq!(after[10], [Feedback::ReloadDone, Feedback::Fired]);
    }

    #[test]
    fn an_empty_chamber_pays_the_charge_even_with_an_empty_spare() {
        let mut sim = charging_sim(rifle_with(0), vec![mag(30, 0)], 10, 4);
        let mut script = vec![reload()];
        script.extend([idle(); 14]);

        let ticks = run(&mut sim, &script);

        assert_eq!(ticks[..14], vec![vec![]; 14][..]);
        assert_eq!(ticks[14], [Feedback::ReloadDone]);
        assert_eq!(sim_rounds(&sim), (false, 0));
    }

    #[test]
    fn an_instant_reload_still_charges_on_an_empty_chamber() {
        let mut sim = charging_sim(rifle_with(0), vec![mag(30, 30)], 0, 4);
        let mut script = vec![reload()];
        script.extend([idle(); 3]);
        script.push(pull());

        let ticks = run(&mut sim, &script);

        assert_eq!(ticks[..4], vec![vec![]; 4][..]);
        assert_eq!(ticks[4], [Feedback::ReloadDone, Feedback::Fired]);
    }

    #[test]
    fn an_overlong_reload_plus_charge_saturates_instead_of_wrapping() {
        let mut sim = charging_sim(rifle_with(0), vec![mag(30, 30)], u32::MAX, 1);

        let ticks = run(&mut sim, &[reload(), idle(), pull()]);

        assert!(ticks.iter().all(Vec::is_empty), "{ticks:?}");
    }

    #[test]
    fn an_instant_reload_completes_at_once_on_a_chambered_round() {
        let mut sim = charging_sim(rifle_with(10), vec![mag(30, 30)], 0, 4);

        let ticks = run(&mut sim, &[reload()]);

        assert_eq!(ticks, [vec![Feedback::ReloadDone]]);
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
    fn reloading_with_an_empty_rig_starts_no_reload() {
        let cases = [
            (rifle_with(10), Feedback::Fired, (true, 8)),
            (rifle_with(0), Feedback::Dry, (false, 0)),
        ];
        for (rifle, pulled, rounds) in cases {
            for free_slots in [0, 3] {
                let chambered = rifle.is_chambered();
                let mut config = test_config(rifle.clone(), vec![]);
                config.reload_ticks = 10;
                config.charge_ticks = 4;
                config.loadout = Loadout::new(rifle.clone(), empty_rig(free_slots));
                let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);
                let mut script = vec![reload(), pull()];
                script.extend([idle(); 14]);

                let ticks = run(&mut sim, &script);

                let label = format!("chambered {chambered}, {free_slots} free slots");
                assert_eq!(ticks[0], [], "{label}");
                assert_eq!(ticks[1], std::slice::from_ref(&pulled), "{label}");
                assert!(ticks[2..].iter().all(Vec::is_empty), "{label}: {ticks:?}");
                assert_eq!(sim_rounds(&sim), rounds, "{label}");
                assert_eq!(sim.rig().magazines().count(), 0, "{label}");
            }
        }
    }

    #[test]
    fn a_rig_holding_only_magazines_of_another_well_starts_no_reload() {
        let mut sim = charging_sim(rifle_with(10), vec![foreign_mag(30, 30)], 10, 4);
        let mut script = vec![reload(), pull()];
        script.extend([idle(); 14]);

        let ticks = run(&mut sim, &script);

        assert_eq!(ticks[0], []);
        assert_eq!(ticks[1], [Feedback::Fired]);
        assert!(ticks[2..].iter().all(Vec::is_empty), "{ticks:?}");
        assert_eq!(sim_rounds(&sim), (true, 8));
        assert_eq!(sim.rig().slots(), [Some(foreign_mag(30, 30))]);
    }

    #[test]
    fn a_finished_reload_leaves_the_empty_magazine_on_the_rig() {
        let mut sim = arena_sim(rifle_with(1), vec![mag(30, 30)]);

        let mid = run(&mut sim, &[reload(), idle(), idle(), idle(), idle()]);
        let held_mid_reload = (sim.rifle().magazine().clone(), sim.rig().slots().to_vec());
        let rest = run(&mut sim, &[idle(); 6]);

        assert!(mid.iter().all(Vec::is_empty), "{mid:?}");
        assert_eq!(held_mid_reload.0, mag(30, 0));
        assert_eq!(held_mid_reload.1, [Some(mag(30, 30))]);
        assert!(rest[..5].iter().all(Vec::is_empty), "{rest:?}");
        assert_eq!(rest[5], [Feedback::ReloadDone]);
        assert_eq!(sim.rifle().magazine(), &mag(30, 30));
        assert_eq!(sim.rig().slots(), [Some(mag(30, 0))]);
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
            crate::test_support::set_eye_above_centre(&mut config, volume.offset.y);
            let mut sim = SearchlightSim::new(
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
        crate::test_support::set_eye_above_centre(&mut config, volume.offset.y + 5.0);
        let mut sim = SearchlightSim::new(
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
            crate::test_support::set_eye_above_centre(&mut config, volume.offset.y);
            config.range = range;
            let mut sim = SearchlightSim::new(
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

    use crate::test_support::world_with;
    use searchlight_rules::LeanSide;

    /// Ticks that settle any lean: twice the blend time.
    const SETTLE_TICKS: usize = 30;

    fn leaning(side: Option<LeanSide>) -> SearchlightCommand {
        SearchlightCommand {
            lean: side,
            ..SearchlightCommand::default()
        }
    }

    fn walking(side: Option<LeanSide>) -> SearchlightCommand {
        SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                ..CharacterCommand::default()
            },
            lean: side,
            ..SearchlightCommand::default()
        }
    }

    fn hold(sim: &mut SearchlightSim, command: &SearchlightCommand, ticks: usize) {
        for _ in 0..ticks {
            sim.tick(command, DT);
        }
    }

    fn standing_eye(sim: &SearchlightSim) -> Vec3 {
        let player = sim.player();
        player.eye_position()
    }

    fn open_sim() -> SearchlightSim {
        arena_sim(rifle_with(5), vec![])
    }

    #[test]
    fn holding_lean_left_settles_the_eye_0_35_m_left() {
        let mut sim = open_sim();

        hold(&mut sim, &leaning(Some(LeanSide::Left)), SETTLE_TICKS);

        let offset = sim.player_eye() - standing_eye(&sim);
        assert!(
            offset.distance(Vec3::new(0.35, 0.0, 0.0)) < 1e-3,
            "left is +X at yaw 0: {offset:?}"
        );
        assert!((sim.player_lean().amount() + 1.0).abs() < 1e-4);
        let roll = sim.player_lean().roll(sim.lean_tuning());
        assert!(
            (roll + 12.0_f32.to_radians()).abs() < 1e-4,
            "rolled toward the left: {roll}"
        );
    }

    #[test]
    fn holding_lean_right_settles_the_eye_0_35_m_right() {
        let mut sim = open_sim();

        hold(&mut sim, &leaning(Some(LeanSide::Right)), SETTLE_TICKS);

        let offset = sim.player_eye() - standing_eye(&sim);
        assert!(
            offset.distance(Vec3::new(-0.35, 0.0, 0.0)) < 1e-3,
            "right is -X at yaw 0: {offset:?}"
        );
        assert!((sim.player_lean().amount() - 1.0).abs() < 1e-4);
        let roll = sim.player_lean().roll(sim.lean_tuning());
        assert!(
            (roll - 12.0_f32.to_radians()).abs() < 1e-4,
            "rolled toward the right: {roll}"
        );
    }

    #[test]
    fn a_lean_follows_the_players_facing() {
        let mut sim = open_sim();
        let turn_left = SearchlightCommand {
            movement: CharacterCommand {
                yaw_delta: FRAC_PI_2,
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        sim.tick(&turn_left, DT);

        hold(&mut sim, &leaning(Some(LeanSide::Right)), SETTLE_TICKS);

        let offset = sim.player_eye() - standing_eye(&sim);
        assert!(
            offset.distance(Vec3::new(0.0, 0.0, 0.35)) < 1e-3,
            "facing +X, right is +Z: {offset:?}"
        );
    }

    #[test]
    fn releasing_the_lean_returns_the_eye_to_standing() {
        let mut sim = open_sim();
        hold(&mut sim, &leaning(Some(LeanSide::Left)), SETTLE_TICKS);
        assert!(
            (sim.player_lean().amount() + 1.0).abs() < 1e-4,
            "the lean never settled"
        );

        hold(&mut sim, &leaning(None), SETTLE_TICKS);

        assert!(sim.player_eye().distance(standing_eye(&sim)) < 1e-4);
        assert_eq!(sim.player_lean().amount(), 0.0);
        assert_eq!(sim.player_lean().roll(sim.lean_tuning()), 0.0);
    }

    #[test]
    fn the_first_lean_tick_moves_the_eye_less_than_a_full_lean() {
        let mut sim = open_sim();

        sim.tick(&leaning(Some(LeanSide::Left)), DT);

        let sideways = sim.player_eye().x - standing_eye(&sim).x;
        assert!(sideways > 0.0, "the eye did not move left: {sideways}");
        assert!(sideways < 0.35 - 0.05, "the lean snapped: {sideways}");
        let amount = sim.player_lean().amount();
        assert!(amount < 0.0 && amount > -1.0, "{amount}");
    }

    #[test]
    fn a_wall_0_15_m_beside_the_capsule_stops_the_lean_short_of_it() {
        const FACE: f32 = 0.45;
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let along_x = |near: f32, far: f32| (Vec3::new(near, 0.0, -5.0), Vec3::new(far, 4.0, 5.0));
        let along_z = |near: f32, far: f32| (Vec3::new(-5.0, 0.0, near), Vec3::new(5.0, 4.0, far));
        // Facing +X (yaw a quarter turn) the player's right is +Z and its left is -Z.
        let cases = [
            (LeanSide::Left, 0.0, along_x(FACE, FACE + 1.0)),
            (LeanSide::Right, 0.0, along_x(-FACE - 1.0, -FACE)),
            (LeanSide::Left, FRAC_PI_2, along_z(-FACE - 1.0, -FACE)),
            (LeanSide::Right, FRAC_PI_2, along_z(FACE, FACE + 1.0)),
        ];
        for (side, yaw, wall) in cases {
            let mut sim = SearchlightSim::new(
                world_with(&[floor, wall]),
                Vec3::ZERO,
                &[],
                test_config(rifle_with(5), vec![]),
            );
            let turn = SearchlightCommand {
                movement: CharacterCommand {
                    yaw_delta: yaw,
                    ..CharacterCommand::default()
                },
                ..SearchlightCommand::default()
            };
            sim.tick(&turn, DT);

            hold(&mut sim, &leaning(Some(side)), SETTLE_TICKS);

            let out = sim.player_eye() - standing_eye(&sim);
            let sideways = out.length();
            assert!(
                sideways <= FACE - 0.15 + 1e-3,
                "{side:?} at yaw {yaw}: the eye is {sideways} m out, within 0.15 m of the wall at {FACE}"
            );
            let amount = sim.player_lean().amount().abs();
            assert!(
                amount < 1.0,
                "{side:?} at yaw {yaw}: a full lean against a wall"
            );
            assert!(
                (amount - (FACE - 0.15) / 0.35).abs() < 0.02,
                "{side:?} at yaw {yaw}: the lean should use all the room, got {amount}"
            );
        }
    }

    #[test]
    fn walking_while_leaning_covers_less_ground_than_walking_upright() {
        let travelled = |side: Option<LeanSide>| {
            let mut sim = open_sim();
            hold(&mut sim, &walking(side), 60);
            sim.player().position.z
        };

        let upright = travelled(None);
        let leaning = travelled(Some(LeanSide::Left));

        assert!(upright > 1.0, "upright barely moved: {upright}");
        assert!(leaning > 0.5, "a lean should slow, not stop: {leaning}");
        assert!(
            leaning < upright * 0.75,
            "leaning {leaning} m vs upright {upright} m"
        );
    }

    /// A dummy on a 0.3 m plinth 8 m ahead whose torso, 0.4 m wide, spans both the upright
    /// eye's line (x = 0) and the line of an eye leaned fully right (x = -0.35) at eye
    /// height; with `corner`, a wall across the upright line that leaves the right-leaned
    /// line open.
    fn corner_sim(corner: bool) -> SearchlightSim {
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let plinth = (Vec3::new(-1.0, 0.0, 7.0), Vec3::new(1.0, 0.3, 9.0));
        let wall = (Vec3::new(-0.2, 0.0, 3.0), Vec3::new(2.0, 4.0, 4.0));
        let boxes: Vec<_> = [Some(floor), Some(plinth), corner.then_some(wall)]
            .into_iter()
            .flatten()
            .collect();
        SearchlightSim::new(
            world_with(&boxes),
            Vec3::ZERO,
            &[Vec3::new(-0.175, 0.3, 8.0)],
            test_config(rifle_with(5), vec![]),
        )
    }

    fn fire_while(sim: &mut SearchlightSim, side: Option<LeanSide>) {
        hold(sim, &leaning(side), SETTLE_TICKS);
        let shot = SearchlightCommand {
            fire: true,
            lean: side,
            ..SearchlightCommand::default()
        };
        sim.tick(&shot, DT);
    }

    fn hit(sim: &mut SearchlightSim) -> bool {
        sim.drain_feedback()
            .iter()
            .any(|fb| matches!(fb, Feedback::Hit { target: 0, .. }))
    }

    #[test]
    fn without_the_corner_the_same_upright_shot_hits_the_dummy() {
        let mut upright = corner_sim(false);

        fire_while(&mut upright, None);

        assert!(hit(&mut upright), "the dummy should stand in the open line");
        assert!(upright.dummy_body(0).expect("a dummy").health() < 100);
    }

    #[test]
    fn a_leaned_shot_hits_a_dummy_visible_only_past_the_corner() {
        let mut upright = corner_sim(true);
        let mut leaned = corner_sim(true);

        fire_while(&mut upright, None);
        fire_while(&mut leaned, Some(LeanSide::Right));

        assert!(
            !hit(&mut upright),
            "the corner should cover the upright eye"
        );
        assert_eq!(upright.dummy_body(0).expect("a dummy").health(), 100);
        assert!(hit(&mut leaned), "the leaned eye sees past the corner");
        assert!(leaned.dummy_body(0).expect("a dummy").health() < 100);
    }

    #[test]
    fn restart_stands_the_player_upright() {
        let mut sim = open_sim();
        hold(&mut sim, &leaning(Some(LeanSide::Left)), SETTLE_TICKS);
        assert!(
            sim.player_lean().amount() < -0.99,
            "the lean never settled before the restart"
        );
        sim.kill_player();

        sim.tick(
            &SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
            DT,
        );

        assert_eq!(sim.player_lean().amount(), 0.0);
        assert!(sim.player_eye().distance(standing_eye(&sim)) < 1e-4);
    }

    #[test]
    fn sidestepping_into_a_wall_while_leaned_pulls_the_lean_in() {
        const FACE: f32 = -1.5;
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let wall = (Vec3::new(FACE - 1.0, 0.0, -5.0), Vec3::new(FACE, 4.0, 5.0));
        let mut sim = SearchlightSim::new(
            world_with(&[floor, wall]),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(5), vec![]),
        );
        hold(&mut sim, &leaning(Some(LeanSide::Right)), SETTLE_TICKS);
        assert!((sim.player_lean().amount() - 1.0).abs() < 1e-4);
        let sidestep = SearchlightCommand {
            movement: CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            },
            lean: Some(LeanSide::Right),
            ..SearchlightCommand::default()
        };

        for tick in 0..400 {
            sim.tick(&sidestep, DT);

            let gap = sim.player_eye().x - FACE;
            assert!(
                gap >= 0.15 - 1e-3,
                "tick {tick}: the eye is {gap} m from the wall"
            );
        }

        assert!(
            sim.player().position.x < -0.8,
            "the player never reached the wall: {:?}",
            sim.player().position
        );
        assert!(
            sim.player_lean().amount() < 1.0,
            "still a full lean against the wall"
        );
    }

    #[test]
    fn a_shot_on_the_first_lean_tick_leaves_from_the_part_leaned_eye() {
        let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
        let plinth = (Vec3::new(-1.0, 0.0, 7.0), Vec3::new(1.0, 0.3, 9.0));
        // Its edge at x = -0.01 blocks the upright line (x = 0) but not an eye already 0.023 m
        // to the right, which is where the first lean tick puts it.
        let wall = (Vec3::new(-0.01, 0.0, 3.0), Vec3::new(2.0, 4.0, 4.0));
        let mut sim = SearchlightSim::new(
            world_with(&[floor, plinth, wall]),
            Vec3::ZERO,
            &[Vec3::new(-0.175, 0.3, 8.0)],
            test_config(rifle_with(5), vec![]),
        );
        let shot = SearchlightCommand {
            fire: true,
            lean: Some(LeanSide::Right),
            ..SearchlightCommand::default()
        };

        sim.tick(&shot, DT);

        let sideways = standing_eye(&sim).x - sim.player_eye().x;
        assert!(
            sideways > 0.01 + 1e-3 && sideways < 0.05,
            "the first tick should lean a little: {sideways}"
        );
        assert!(
            hit(&mut sim),
            "the shot should leave from the eye as leaned that tick"
        );
    }

    mod waves {
        use super::*;
        use crate::test_support::arena_with;

        fn config() -> SimConfig {
            test_config(rifle_from(mag(30, 30)), vec![])
        }

        fn idle() -> SearchlightCommand {
            SearchlightCommand::default()
        }

        fn pull() -> SearchlightCommand {
            SearchlightCommand {
                fire: true,
                ..SearchlightCommand::default()
            }
        }

        fn restart() -> SearchlightCommand {
            SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            }
        }

        /// A far waypoint straight ahead of the markers, so enemies face away from the player.
        const AHEAD: Vec3 = Vec3::new(0.0, 0.0, 40.0);

        fn empty_sim(markers: &[Vec3]) -> SearchlightSim {
            SearchlightSim::with_start(
                arena_with(&[], Vec3::ZERO, markers, &[AHEAD]),
                config(),
                Start::Empty,
            )
        }

        fn hits(sim: &mut SearchlightSim) -> Vec<u16> {
            sim.drain_feedback()
                .into_iter()
                .filter_map(|feedback| match feedback {
                    Feedback::Hit { target, .. } => Some(target),
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn spawn_enemy_places_an_enemy_at_the_marker() {
            let mut sim = empty_sim(&[Vec3::new(5.0, 0.0, 10.0), Vec3::new(-5.0, 0.0, 10.0)]);
            assert_eq!(sim.enemy_count(), 0);

            let spawned = sim.spawn_enemy(1, Grade::Experienced);

            assert_eq!(spawned, Ok(()));
            assert_eq!(sim.enemy_count(), 1);
            assert_eq!(sim.enemies_alive(), 1);
            let state = sim.enemy_state(0).expect("the enemy spawned");
            assert!((state.position.x + 5.0).abs() < 1e-3, "{state:?}");
            assert!((state.position.z - 10.0).abs() < 1e-3, "{state:?}");
            assert!((state.position.y - STAND_HEIGHT).abs() < 1e-3, "{state:?}");
        }

        #[test]
        fn a_spawned_enemy_faces_its_nearest_waypoint_and_walks_it() {
            let arena = arena_with(
                &[],
                Vec3::new(0.0, 0.0, -10.0),
                &[Vec3::new(0.0, 0.0, 10.0)],
                &[Vec3::new(10.0, 0.0, 10.0), AHEAD],
            );
            let mut sim = SearchlightSim::with_start(arena, config(), Start::Empty);
            sim.spawn_enemy(0, Grade::Experienced)
                .expect("the marker is free");

            let spawned = sim.enemy_state(0).expect("the enemy spawned");
            for _ in 0..30 {
                sim.tick(&idle(), DT);
            }

            let walked = sim.enemy_state(0).expect("the enemy is alive");
            assert!((spawned.yaw - FRAC_PI_2).abs() < 1e-3, "{spawned:?}");
            assert!(walked.position.x > 0.5, "{walked:?}");
        }

        #[test]
        fn a_spawned_enemy_takes_the_next_target_number_after_the_dummies() {
            let mut arena =
                (*arena_with(&[], Vec3::ZERO, &[Vec3::new(0.0, 0.0, 10.0)], &[AHEAD])).clone();
            arena.dummy_spawns = vec![Vec3::new(-8.0, 0.0, 4.0), Vec3::new(-4.0, 0.0, 4.0)];
            let mut sim = SearchlightSim::with_start(Arc::new(arena), config(), Start::Empty);
            sim.spawn_enemy(0, Grade::Experienced)
                .expect("the marker is free");

            sim.tick(&pull(), DT);

            assert_eq!(hits(&mut sim), [2]);
        }

        #[test]
        fn a_respawned_enemy_takes_a_new_target_number_and_the_dead_keep_theirs() {
            let mut sim = empty_sim(&[Vec3::new(0.0, 0.0, 10.0)]);
            sim.spawn_enemy(0, Grade::Experienced)
                .expect("the marker is free");
            sim.kill_enemy(0);
            sim.tick(&idle(), DT);
            assert_eq!(sim.enemies_alive(), 0);
            sim.drain_feedback();

            let respawned = sim.spawn_enemy(0, Grade::Experienced);
            sim.tick(&pull(), DT);

            assert_eq!(respawned, Ok(()));
            assert_eq!(sim.enemy_count(), 2);
            assert!(sim.enemy_state(0).is_none());
            assert!(sim.enemy_state(1).is_some());
            assert_eq!(hits(&mut sim), [1]);
        }

        #[test]
        fn an_occupied_marker_is_not_free() {
            let mut sim = empty_sim(&[Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 20.0)]);
            let before = sim.free_enemy_spawns();

            sim.spawn_enemy(0, Grade::Experienced)
                .expect("the marker is free");
            let while_occupied = sim.free_enemy_spawns();
            sim.kill_enemy(0);
            sim.tick(&idle(), DT);
            let after_death = sim.free_enemy_spawns();

            assert_eq!(before, [0, 1]);
            assert_eq!(while_occupied, [1]);
            assert_eq!(after_death, [0, 1]);
        }

        #[test]
        fn a_marker_within_a_capsule_width_of_a_character_is_not_free() {
            let mut arena = (*arena_with(
                &[],
                Vec3::ZERO,
                &[
                    Vec3::new(0.55, 0.0, 0.0),
                    Vec3::new(0.9, 0.0, 0.0),
                    Vec3::new(-8.0, 0.0, 4.0),
                    Vec3::new(-8.5, 0.0, 4.0),
                    Vec3::new(-8.0, 5.0, 4.0),
                ],
                &[AHEAD],
            ))
            .clone();
            arena.dummy_spawns = vec![Vec3::new(-8.0, 0.0, 4.0)];
            let sim = SearchlightSim::with_start(Arc::new(arena), config(), Start::Empty);

            let free = sim.free_enemy_spawns();

            // 0.55 m from the player and 0.0 and 0.5 m from the dummy are touching; 0.9 m is not,
            // and a marker high above the dummy still counts as on it.
            assert_eq!(free, [1]);
        }

        #[test]
        fn spawn_enemy_refuses_an_occupied_or_unknown_marker() {
            let mut sim = empty_sim(&[Vec3::new(0.0, 0.0, 10.0)]);

            let unknown = sim.spawn_enemy(1, Grade::Experienced);
            let first = sim.spawn_enemy(0, Grade::Experienced);
            let occupied = sim.spawn_enemy(0, Grade::Experienced);

            assert_eq!(unknown, Err(SpawnError::UnknownMarker { marker: 1 }));
            assert_eq!(first, Ok(()));
            assert_eq!(occupied, Err(SpawnError::OccupiedMarker { marker: 0 }));
            assert_eq!(sim.enemy_count(), 1);
        }

        #[test]
        fn with_start_at_markers_matches_start() {
            let arena = arena_with(
                &[],
                Vec3::ZERO,
                &[Vec3::new(0.0, 0.0, 10.0), Vec3::new(3.0, 0.0, 10.0)],
                &[AHEAD],
            );

            let with = SearchlightSim::with_start(arena.clone(), config(), Start::AtMarkers);
            let plain = SearchlightSim::start(arena, config());

            assert_eq!(with.enemy_count(), 2);
            assert_eq!(with.enemies_alive(), 2);
            assert_eq!(with.enemy_state(1), plain.enemy_state(1));
            assert_eq!(with.free_enemy_spawns(), plain.free_enemy_spawns());
            assert_eq!(with.kills(), 0);
        }

        #[test]
        fn kills_counts_player_kills_and_resets_on_restart() {
            let mut sim = empty_sim(&[Vec3::new(-3.0, 0.0, 10.0), Vec3::new(3.0, 0.0, 10.0)]);
            sim.spawn_enemy(0, Grade::Experienced).expect("free");
            sim.spawn_enemy(1, Grade::Experienced).expect("free");
            let none = sim.kills();

            sim.kill_enemy(0);
            let one = sim.kills();
            sim.kill_enemy(1);
            let two = sim.kills();
            sim.tick(&idle(), DT);
            let after_removal = sim.kills();
            sim.kill_player();
            sim.tick(&restart(), DT);

            assert_eq!((none, one, two, after_removal), (0, 1, 2, 2));
            assert_eq!(sim.kills(), 0);
            assert_eq!(sim.enemy_count(), 0);
            assert_eq!(sim.enemies_alive(), 0);
        }

        #[test]
        fn an_empty_start_survives_restart() {
            let mut sim = empty_sim(&[Vec3::new(-3.0, 0.0, 10.0), Vec3::new(3.0, 0.0, 10.0)]);
            sim.spawn_enemy(0, Grade::Experienced).expect("free");
            sim.kill_player();

            sim.tick(&restart(), DT);

            assert!(!sim.player_body().is_dead());
            assert_eq!(sim.enemy_count(), 0);
            assert_eq!(sim.free_enemy_spawns(), [0, 1]);
            assert_eq!(sim.spawn_enemy(0, Grade::Experienced), Ok(()));
            assert_eq!(sim.enemy_count(), 1);
        }

        #[test]
        fn a_restart_of_an_at_markers_sim_brings_the_enemies_back() {
            let arena = arena_with(&[], Vec3::ZERO, &[Vec3::new(0.0, 0.0, 10.0)], &[AHEAD]);
            let mut sim = SearchlightSim::with_start(arena, config(), Start::AtMarkers);
            sim.kill_enemy(0);
            sim.tick(&idle(), DT);
            assert_eq!(sim.enemies_alive(), 0);
            sim.kill_player();

            sim.tick(&restart(), DT);

            assert_eq!(sim.enemies_alive(), 1);
            assert_eq!(sim.kills(), 0);
        }

        #[test]
        fn a_hit_on_a_dummy_is_not_a_kill() {
            let mut arena = (*arena_with(&[], Vec3::ZERO, &[], &[])).clone();
            arena.dummy_spawns = vec![Vec3::new(0.0, 0.0, 10.0)];
            let mut config = config();
            config.damage = 1_000;
            let mut sim = SearchlightSim::with_start(Arc::new(arena), config, Start::AtMarkers);

            sim.tick(&pull(), DT);

            assert_eq!(hits(&mut sim), [0]);
            assert!(sim.dummy_body(0).expect("a dummy").is_dead());
            assert_eq!(sim.kills(), 0);
        }

        #[test]
        fn a_non_lethal_hit_on_an_enemy_is_not_a_kill_and_the_lethal_one_is() {
            let mut sim = empty_sim(&[Vec3::new(0.0, 0.0, 10.0)]);
            sim.spawn_enemy(0, Grade::Experienced).expect("free");
            // The 3-tick cooldown allows the second pull on tick 3; two 30-damage head hits kill.
            let mut kills_after = Vec::new();
            for tick in 0..4 {
                sim.tick(
                    &if tick == 0 || tick == 3 {
                        pull()
                    } else {
                        idle()
                    },
                    DT,
                );
                kills_after.push(sim.kills());
                if tick == 0 {
                    assert!(!sim.enemy_body(0).expect("an enemy").is_dead());
                    assert_eq!(sim.enemies_alive(), 1);
                }
            }

            assert_eq!(kills_after, [0, 0, 0, 1]);
            assert_eq!(sim.enemies_alive(), 0);
            assert_eq!(sim.enemy_count(), 1);
        }

        #[test]
        fn a_dead_enemy_is_counted_once_however_long_it_lies() {
            let mut sim = empty_sim(&[Vec3::new(0.0, 0.0, 10.0)]);
            sim.spawn_enemy(0, Grade::Experienced).expect("free");

            sim.kill_enemy(0);
            for _ in 0..5 {
                sim.tick(&idle(), DT);
            }

            assert_eq!(sim.kills(), 1);
        }
    }
}

#[cfg(test)]
mod dummy_pose_tests {
    use searchlight_rules::BodyPart;

    use super::*;
    use crate::test_support::{DT, flat_floor, mag, rifle_from, test_config};

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
        let mut config = test_config(rifle_from(mag(30, 5)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, volume.offset.y);
        let mut sim = SearchlightSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[Vec3::new(0.0, 0.0, 8.0)],
            config,
        );
        for _ in 0..600 {
            sim.tick(&SearchlightCommand::default(), DT);
        }

        sim.tick(
            &SearchlightCommand {
                fire: true,
                ..SearchlightCommand::default()
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
    use crate::test_support::{DT, flat_floor, mag, rifle_from, test_config};

    fn rifle_with(rounds: u32) -> Rifle {
        rifle_from(mag(30, rounds))
    }

    fn idle() -> SearchlightCommand {
        SearchlightCommand::default()
    }

    fn pull() -> SearchlightCommand {
        SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn reload() -> SearchlightCommand {
        SearchlightCommand {
            reload: true,
            ..SearchlightCommand::default()
        }
    }

    fn volume_of(target: u16, part: BodyPart) -> HitVolume {
        let tag = SurfaceTag::Part { target, part };
        dummy_hit_volumes(target)
            .into_iter()
            .find(|v| v.tag == tag.into())
            .expect("a volume per part")
    }

    fn run(sim: &mut SearchlightSim, script: &[SearchlightCommand]) -> Vec<Vec<Feedback>> {
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
        crate::test_support::set_eye_above_centre(&mut config, head.offset.y);
        let mut sim = SearchlightSim::new(
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
            let mut sim = SearchlightSim::new(
                flat_floor(),
                Vec3::ZERO,
                &[],
                test_config(rifle_with(5), vec![]),
            );
            let look = SearchlightCommand {
                movement: CharacterCommand {
                    pitch_delta,
                    ..CharacterCommand::default()
                },
                ..SearchlightCommand::default()
            };

            sim.tick(&look, DT);

            let state = sim.player();
            assert_eq!(state.pitch.abs(), PITCH_LIMIT);
            let view = first_person_view(&state, &state, 1.0, (0.0, 0.0));
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
            let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);
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
        let mut reference = SearchlightSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(10), vec![mag(30, 30)]),
        );
        reference.tick(&pull(), DT);
        reference.drain_feedback();
        reference.tick(
            &SearchlightCommand {
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
        let mut sim = SearchlightSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(rifle_with(10), vec![mag(30, 30)]),
        );
        let all = SearchlightCommand {
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
        let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let all = SearchlightCommand {
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
        crate::test_support::set_eye_above_centre(&mut config, torso.offset.y);
        config.cooldown_ticks = 0;
        let mut sim = SearchlightSim::new(
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
        let mut sim = SearchlightSim::new(
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
        assert_eq!(sim.rig().magazines().count(), 2);
        assert_eq!(sim.rig().free_slots(), 0);
    }
}

#[cfg(test)]
mod hit_volume_and_cooldown_tests {
    use moho_physics::HitTag;

    use super::*;
    use crate::test_support::{DT, flat_floor, mag, rifle_from, test_config};

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
                [0.12, -0.5075, 0.0],
                [-0.12, -0.5075, 0.0],
            ]
        );
    }

    /// Ticks that fired when the trigger is pulled on each of `ticks` consecutive ticks.
    fn fired_ticks(cooldown_ticks: u32, ticks: usize) -> Vec<usize> {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.cooldown_ticks = cooldown_ticks;
        let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let pull = SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
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
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, eye_height);
        let mut sim = SearchlightSim::new(
            flat_floor(),
            Vec3::new(x, 0.0, 0.0),
            &[Vec3::new(0.0, 0.0, distance)],
            config,
        );
        let pull = SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
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
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.character_body.standing.eye_height = 1.6;

        let sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);

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
    use crate::test_support::{DT, arena_with, mag, rifle_from, test_config};

    /// Height of the head volume's centre above the capsule centre: a level shot from here
    /// strikes a standing body's head.
    const HEAD_HEIGHT: f32 = 0.72;
    /// Head status of every body in these tests (see `test_profile`): two 30-damage hits kill.
    const PLAYER_ONE_SHOT: u32 = 60;

    fn config() -> SimConfig {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, HEAD_HEIGHT);
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

    fn idle() -> SearchlightCommand {
        SearchlightCommand::default()
    }

    fn pull() -> SearchlightCommand {
        SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn restart() -> SearchlightCommand {
        SearchlightCommand {
            restart: true,
            ..SearchlightCommand::default()
        }
    }

    /// `len` ticks of nothing, with a trigger pull on each tick in `pulls`.
    fn script(len: usize, pulls: &[usize]) -> Vec<SearchlightCommand> {
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

    fn run(sim: &mut SearchlightSim, script: &[SearchlightCommand]) -> Vec<Vec<Feedback>> {
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
        let mut arena = Arena::from_level(&level, &markers).expect("the shipped arena builds");
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
        let mut sim = SearchlightSim::start(Arc::new(arena), config);
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
    fn a_patrolling_enemy_walks_around_cover_to_the_next_waypoint() {
        let first = Vec3::new(-10.0, 0.0, 0.0);
        let second = Vec3::new(10.0, 0.0, 0.0);
        let cover = (Vec3::new(-1.0, 0.0, -6.0), Vec3::new(1.0, 3.0, 6.0));
        let arena = arena_with(
            &[cover],
            Vec3::new(0.0, 0.0, 25.0),
            &[first],
            &[first, second],
        );
        let mut config = config();
        // Too short to ever notice the player, so only the route moves the enemy.
        config.enemy.sight_range = 0.5;
        let mut sim = SearchlightSim::start(arena, config);
        let mut closest = f32::MAX;

        for _ in 0..2400 {
            sim.tick(&idle(), DT);
            let at = sim
                .enemy_state(0)
                .expect("nothing hurts the enemy")
                .position;
            closest = closest.min(horizontal(at, second));
        }

        assert!(
            closest < 1.0,
            "the second waypoint was never reached, closest {closest} m"
        );
    }

    #[test]
    fn an_enemy_spawns_facing_its_nearest_waypoint_and_walks_to_it() {
        let arena = arena_with(
            &[],
            Vec3::new(0.0, 0.0, -10.0),
            &[ENEMY_SPAWN],
            &[Vec3::new(10.0, 0.0, 10.0), AWAY_FROM_PLAYER],
        );
        let mut sim = SearchlightSim::start(arena, config());

        let spawned = sim.enemy_state(0).expect("the enemy spawned");
        run(&mut sim, &script(30, &[]));

        let walked = sim.enemy_state(0).expect("the enemy is alive");
        assert!((spawned.yaw - FRAC_PI_2).abs() < 1e-3, "{spawned:?}");
        assert!(walked.position.x > 0.5, "{walked:?}");
        assert!((walked.position.z - 10.0).abs() < 0.2, "{walked:?}");
    }

    #[test]
    fn an_enemy_with_sight_fires_at_the_player() {
        let mut sim = SearchlightSim::start(duel(&[]), config());

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
            let mut sim = SearchlightSim::start(duel(&[]), config);
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
        let mut sim = SearchlightSim::start(duel(&[wall]), config());

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
            let mut sim = SearchlightSim::start(duel(&[]), config);

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
        let mut sim = SearchlightSim::start(duel(&[]), config);

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
        let mut sim = SearchlightSim::start(duel(&[]), config());

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
            let mut sim = SearchlightSim::start(duel(&[]), config);
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
        let mut probe = SearchlightSim::start(arena.clone(), config.clone());
        let mut sim = SearchlightSim::start(arena, config);

        let ticks = run(&mut sim, &script(600, &[]));

        // Replay the shooter's spread on a twin sim: the same seed gives the same shots.
        let shooter = probe.targets[0].handle.expect("the shooter spawned");
        let eye = probe
            .world
            .character(shooter)
            .expect("alive")
            .eye_position();
        let aim = probe.player_aim_point();
        let clipped = (0..count(&ticks, enemy_fired))
            .filter(|_| {
                let direction = probe.with_spread(aim - eye, probe.enemy.aim_error);
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
        let mut walking = SearchlightSim::start(away, config_a);
        // Facing the player, who answers its first shot with a killing one.
        let mut config_b = config();
        config_b.damage = PLAYER_ONE_SHOT;
        let mut firing = SearchlightSim::start(duel(&[]), config_b);

        run(&mut walking, &script(5, &[]));
        let before = walking.enemy_state(0).expect("alive unshot").position;
        let walked = run(&mut walking, &script(3, &[0]));
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
        let mut sim = SearchlightSim::start(arena, config);

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
        let mut control = SearchlightSim::start(duel(&[]), config.clone());
        let mut contested = SearchlightSim::start(duel(&[]), config);

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
        let mut sim = SearchlightSim::start(arena, config());

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
        let mut sim = SearchlightSim::start(arena, sturdy(config()));
        let turn_and_walk = |first: bool| SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { FRAC_PI_2 } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
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
        let mut sim = SearchlightSim::start(arena, config);
        let walker_start = sim.enemy_state(1).expect("spawned").position;

        let mut ticks: Vec<Vec<Feedback>> = Vec::new();
        while !ticks.iter().flatten().any(player_died) {
            ticks.extend(run(&mut sim, &script(1, &[])));
            assert!(ticks.len() < 300, "the player never died");
        }
        let walker_at_death = sim.enemy_state(1).expect("alive").position;
        let player_at_death = sim.player();
        let rounds = sim.rifle().magazine().rounds();
        let restless = SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                jump: true,
                ..CharacterCommand::default()
            },
            fire: true,
            reload: true,
            check: true,
            ..SearchlightCommand::default()
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
        let mut sim = SearchlightSim::start(arena, config);

        let ticks = run(&mut sim, &script(120, &[]));

        let death = ticks_where(&ticks, player_died);
        assert_eq!(death.len(), 1, "{ticks:?}");
        let fired_that_tick = ticks[death[0]].iter().filter(|fb| enemy_fired(fb)).count();
        assert_eq!(fired_that_tick, 1, "{:?}", ticks[death[0]]);
        assert_eq!(count(&ticks, enemy_fired), 1);
        assert_eq!(count(&ticks, player_hit), 1);
    }

    /// The player killed by one of two enemies after dropping the other, then restarted.
    fn dead_player_in_a_busy_arena() -> (SearchlightSim, Arc<Arena>, SimConfig) {
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
        let mut sim = SearchlightSim::start(arena.clone(), config.clone());
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
    fn a_restart_after_a_reload_restores_the_kit_from_the_config() {
        let arena = arena_with(&[], Vec3::ZERO, &[ENEMY_SPAWN], &[TOWARD_PLAYER]);
        let mut config = test_config(rifle_from(mag(30, 10)), vec![mag(30, 30)]);
        crate::test_support::set_eye_above_centre(&mut config, HEAD_HEIGHT);
        config.enemy.damage = 100;
        config.enemy.reaction_ticks = 40;
        let fresh = SearchlightSim::start(arena.clone(), config.clone());
        let mut sim = SearchlightSim::start(arena, config);
        let reload = SearchlightCommand {
            reload: true,
            ..idle()
        };
        let mut dead = false;
        for tick in 0..2000 {
            sim.tick(&if tick == 0 { reload } else { idle() }, DT);
            dead |= sim.drain_feedback().iter().any(player_died);
            if dead {
                break;
            }
        }
        assert!(dead, "the player never died");
        assert_ne!(
            sim.rig(),
            fresh.rig(),
            "the reload should have moved magazines"
        );

        sim.tick(&restart(), DT);

        assert_eq!(sim.rig(), fresh.rig());
        assert_eq!(sim.rifle(), fresh.rifle());
    }

    #[test]
    fn restart_after_death_matches_a_fresh_start() {
        let (mut restarted, arena, config) = dead_player_in_a_busy_arena();
        let mut fresh = SearchlightSim::start(arena, config);
        assert!(
            restarted.enemy_state(0).is_none(),
            "the setup killed the first enemy"
        );

        restarted.tick(&restart(), DT);

        assert_eq!(restarted.player(), fresh.player());
        assert_eq!(restarted.player_body(), fresh.player_body());
        assert!(!restarted.player_body().is_dead());
        assert_eq!(restarted.rifle(), fresh.rifle());
        assert_eq!(restarted.rig(), fresh.rig());
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
    fn restart_replays_the_spread_of_a_fresh_start() {
        let (mut restarted, arena, config) = dead_player_in_a_busy_arena();
        let mut fresh = SearchlightSim::start(arena, config);
        assert_ne!(restarted.rng, fresh.rng, "enemies fired before the restart");

        restarted.tick(&restart(), DT);

        let aim = Vec3::new(0.0, 0.0, 5.0);
        for _ in 0..4 {
            assert_eq!(
                restarted.with_spread(aim, restarted.enemy.aim_error),
                fresh.with_spread(aim, fresh.enemy.aim_error)
            );
        }
    }

    #[test]
    fn a_dead_player_in_an_arena_sim_restarts_to_a_living_one() {
        let mut sim = SearchlightSim::start(arena_with(&[], Vec3::ZERO, &[], &[]), config());
        sim.player_body.hit(BodyPart::Torso, 1_000);
        assert!(sim.player_body().is_dead());

        sim.tick(&restart(), DT);

        assert!(!sim.player_body().is_dead());
    }

    #[test]
    fn a_dead_player_in_a_sim_built_without_an_arena_restarts_to_a_living_one() {
        let mut sim =
            SearchlightSim::new(crate::test_support::flat_floor(), Vec3::ZERO, &[], config());
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
        let mut pressed = SearchlightSim::start(arena.clone(), config.clone());
        let mut control = SearchlightSim::start(arena, config);
        let walk = SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        let lead_in = [walk; 5];
        let with_restart = SearchlightCommand {
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
    const SHIELDED: Vec3 = Vec3::new(4.0, 0.0, 10.0);
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

    fn shoot_at(sim: &SearchlightSim, index: usize) -> SearchlightCommand {
        let at = sim
            .enemy_state(index)
            .expect("the enemy to shoot at is alive")
            .position;
        let player = sim.player();
        let bearing = (at.x - player.position.x).atan2(at.z - player.position.z);
        SearchlightCommand {
            movement: CharacterCommand {
                yaw_delta: bearing - player.yaw,
                ..CharacterCommand::default()
            },
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn enemy_at(sim: &SearchlightSim, index: usize) -> Vec3 {
        sim.enemy_state(index).expect("the enemy is alive").position
    }

    fn fired_by(target: u16) -> impl Fn(&Feedback) -> bool {
        move |feedback| *feedback == Feedback::EnemyFired { target }
    }

    #[test]
    fn an_enemy_hit_by_the_player_runs_to_cover() {
        let arena = with_covers(duel(&[LOW_WALL]), &[DECOY, SHIELDED]);
        let mut sim = SearchlightSim::start(arena, cover_config());

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
        let mut arena = Arena::from_level(&level, &markers).expect("the shipped arena builds");
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
        let mut sim = SearchlightSim::start(Arc::new(arena), cover_config());

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
        let mut killer = SearchlightSim::start(arena.clone(), lethal);
        let mut control = SearchlightSim::start(arena, wounding);

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
        let mut sim = SearchlightSim::start(two_enemies_and_two_shielded_points(), config);

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
        let mut sim = SearchlightSim::start(two_enemies_and_two_shielded_points(), config);
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
        let mut sim = SearchlightSim::start(arena, cover_config());
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
    const LAST_SEEN: Vec3 = Vec3::new(1.1, 0.0, 0.0);

    /// The enemy ten metres ahead of the player, a waypoint beside it and a far one behind,
    /// and the player walking east behind `SIDE_WALL`.
    fn player_ducks_behind_the_side_wall() -> SearchlightSim {
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
        let mut sim = SearchlightSim::start(arena, config);
        let turn_and_walk = |first: bool| SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { FRAC_PI_2 } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
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

    /// Waist high: sight and shot rays pass over it, a walking enemy cannot. Across the straight
    /// line between the enemy and the player, and ending well short of both sides' spawns.
    const LOW_BARRIER: (Vec3, Vec3) = (Vec3::new(-3.0, 0.0, 4.9), Vec3::new(3.0, 1.0, 5.1));

    /// How far the position the enemy remembers may sit from the player's centre where it lost
    /// sight: the aim offset to the torso, plus a tick of the player's running.
    const LAST_SEEN_SLACK: f32 = 0.5;

    #[test]
    fn a_chasing_enemy_walks_around_a_wall_to_the_last_seen_position() {
        let mut config = cover_config();
        // The player is lost by running out of range, not by a wall in the way.
        config.enemy.sight_range = 13.0;
        let mut sim = SearchlightSim::start(duel(&[LOW_BARRIER]), config);
        let flee = |first: bool| SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { std::f32::consts::PI } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        let first_shot = shoot_at(&sim, 0);
        sim.tick(&first_shot, DT);
        sim.drain_feedback();

        let mut closest_z = f32::MAX;
        let mut lost_at = None;
        let mut closest_to_lost = f32::MAX;
        for tick in 0..500 {
            sim.tick(&flee(tick == 0), DT);
            sim.drain_feedback();
            let enemy = enemy_at(&sim, 0);
            closest_z = closest_z.min(enemy.z);
            if lost_at.is_none()
                && matches!(
                    sim.targets[0].enemy.map(|enemy| enemy.brain),
                    Some(Brain::Chase { .. })
                )
            {
                lost_at = Some(sim.player().position);
            }
            if let Some(lost) = lost_at {
                closest_to_lost = closest_to_lost.min(horizontal(enemy, lost));
            }
        }

        assert!(lost_at.is_some(), "the enemy never lost the player");
        assert!(
            sim.player().position.z < -10.0,
            "the player never ran out of the enemy's range: {:?}",
            sim.player().position
        );
        assert!(
            closest_z < 0.0,
            "the enemy never got past the barrier at z = 5, closest z {closest_z}"
        );
        let reach = sim.enemy.arrive_radius + LAST_SEEN_SLACK;
        assert!(
            closest_to_lost <= reach,
            "the enemy never reached the place it lost the player {lost_at:?}, closest {closest_to_lost} m"
        );
    }

    #[test]
    fn an_enemy_that_loses_a_player_at_a_jump_apex_keeps_chasing_toward_them() {
        let mut sim = SearchlightSim::start(duel(&[]), cover_config());
        run(&mut sim, &script(10, &[]));
        let standing = sim.player().position.y;
        let jump = SearchlightCommand {
            movement: CharacterCommand {
                jump: true,
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        sim.tick(&jump, DT);
        sim.drain_feedback();
        let mut apex = sim.player().position;
        for _ in 0..120 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
            if sim.player().position.y < apex.y {
                break;
            }
            apex = sim.player().position;
        }
        assert!(
            apex.y - standing > 0.9,
            "the jump never rose well off the floor: {apex:?}, standing at {standing}"
        );
        // Nothing can be seen beyond a metre from now on; the player is still airborne.
        sim.enemy.sight_range = 1.0;
        let before = enemy_at(&sim, 0);
        let airborne = sim.player().position;

        sim.tick(&idle(), DT);
        sim.drain_feedback();
        let after_losing_sight = sim.targets[0].enemy.map(|enemy| enemy.brain);
        for _ in 0..5 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
        }

        assert!(
            airborne.y - standing > 0.9,
            "the player landed before sight was lost: {airborne:?}"
        );
        assert!(
            matches!(after_losing_sight, Some(Brain::Chase { .. })),
            "the enemy should keep chasing: {after_losing_sight:?}"
        );
        assert!(
            horizontal(enemy_at(&sim, 0), airborne) < horizontal(before, airborne) - 0.05,
            "the enemy did not walk toward the player: {before:?} -> {:?}",
            enemy_at(&sim, 0)
        );
    }

    /// Full height: neither sight nor a walking enemy passes it. It runs east from just beside
    /// the straight line between the enemy and the player to the edge of the floor, so the only
    /// way past is the west end.
    const FULL_WALL: (Vec3, Vec3) = (Vec3::new(-0.2, 0.0, 4.0), Vec3::new(30.0, 4.0, 4.4));

    #[test]
    fn a_chasing_enemy_rounds_a_full_height_wall_to_where_it_lost_the_player() {
        let mut config = cover_config();
        config.enemy.arrive_radius = 1.0;
        config.enemy.chase_ticks = 900;
        // The player is lost to the wall, and runs on out of range of the wall's west end.
        config.enemy.sight_range = 11.0;
        let arena = arena_with(
            &[FULL_WALL],
            Vec3::new(-3.0, 0.0, 0.0),
            &[ENEMY_SPAWN],
            &[TOWARD_PLAYER, Vec3::new(0.0, 0.0, 25.0)],
        );
        let mut sim = SearchlightSim::start(arena, config);
        let step_east = |first: bool| SearchlightCommand {
            movement: CharacterCommand {
                forward: 1.0,
                yaw_delta: if first { FRAC_PI_2 } else { 0.0 },
                ..CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        let mut lost_at = None;
        let mut closest_to_lost = f32::MAX;

        for tick in 0..900 {
            let command = if tick < 240 {
                step_east(tick == 0)
            } else {
                idle()
            };
            sim.tick(&command, DT);
            sim.drain_feedback();
            if lost_at.is_none()
                && matches!(
                    sim.targets[0].enemy.map(|enemy| enemy.brain),
                    Some(Brain::Chase { .. })
                )
            {
                lost_at = Some(sim.player().position);
            }
            if let Some(lost) = lost_at {
                closest_to_lost = closest_to_lost.min(horizontal(enemy_at(&sim, 0), lost));
            }
        }

        let lost = lost_at.expect("the enemy never lost the player");
        let reach = sim.enemy.arrive_radius + LAST_SEEN_SLACK;
        assert!(
            closest_to_lost <= reach,
            "the enemy never reached {lost:?}, closest {closest_to_lost} m"
        );
    }

    /// A box the player can stand on but no enemy can climb: taller than the 0.5 m autostep.
    const PLATEAU: (Vec3, Vec3) = (Vec3::new(-2.0, 0.0, -2.0), Vec3::new(2.0, 1.5, 2.0));

    #[test]
    fn an_enemy_that_loses_a_player_it_cannot_reach_resumes_its_patrol() {
        let top = Vec3::new(0.0, PLATEAU.1.y, 0.0);
        let arena = arena_with(&[PLATEAU], top, &[ENEMY_SPAWN], &[TOWARD_PLAYER]);
        let mut sim = SearchlightSim::start(arena, cover_config());
        run(&mut sim, &script(10, &[]));
        let engaged = matches!(
            sim.targets[0].enemy.map(|enemy| enemy.brain),
            Some(Brain::Engage { .. })
        );
        // Nothing can be seen beyond a metre from now on.
        sim.enemy.sight_range = 1.0;

        sim.tick(&idle(), DT);
        sim.drain_feedback();
        let after_losing_sight = sim.targets[0].enemy.map(|enemy| enemy.brain);
        let mut closest_z = f32::MAX;
        for _ in 0..600 {
            sim.tick(&idle(), DT);
            sim.drain_feedback();
            closest_z = closest_z.min(enemy_at(&sim, 0).z);
        }

        assert!(
            sim.player().position.y > 1.0,
            "the player is not on the box"
        );
        assert!(engaged, "the enemy never engaged the player on the box");
        assert!(
            matches!(after_losing_sight, Some(Brain::Patrol { .. })),
            "the enemy should give up at once: {after_losing_sight:?}"
        );
        assert!(
            closest_z > TOWARD_PLAYER.z - 1.0,
            "the enemy walked toward the box, closest z {closest_z}"
        );
        assert!(
            horizontal(enemy_at(&sim, 0), TOWARD_PLAYER) < 1.0,
            "the enemy is not at its waypoint: {:?}",
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
        let mut sim = SearchlightSim::start(arena, config);
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
    fn an_enemy_shot_from_behind_with_no_cover_turns_and_fires_without_walking() {
        let arena = arena_with(&[], Vec3::ZERO, &[ENEMY_SPAWN], &[AWAY_FROM_PLAYER]);
        let mut sim = SearchlightSim::start(arena, cover_config());
        let mut farthest: f32 = 0.0;

        let mut ticks = Vec::new();
        for command in script(180, &[0]) {
            ticks.extend(run(&mut sim, &[command]));
            farthest = farthest.max(horizontal(enemy_at(&sim, 0), ENEMY_SPAWN));
        }

        let hits: Vec<_> = ticks[0].iter().filter_map(hit_of).collect();
        assert_eq!(hits.len(), 1, "{:?}", ticks[0]);
        assert!(
            count(&ticks, enemy_fired) >= 5,
            "{} shots",
            count(&ticks, enemy_fired)
        );
        assert!(
            farthest < 0.3,
            "the enemy walked {farthest} m instead of standing to fight"
        );
    }

    #[test]
    fn a_hurt_enemy_in_an_arena_without_cover_points_keeps_firing() {
        let mut sim = SearchlightSim::start(duel(&[LOW_WALL]), cover_config());

        let ticks = run(&mut sim, &script(400, &[0]));

        assert!(
            count(&ticks[1..], enemy_fired) >= 30,
            "{} shots",
            count(&ticks[1..], enemy_fired)
        );
        assert!(horizontal(enemy_at(&sim, 0), ENEMY_SPAWN) < 1.0);
    }

    /// A wall halfway between the player and `SHIELDED`'s row of points, `top` metres high.
    /// The line from the player's eye (about 1.635 m up) to the hide height behind it
    /// crosses the wall at about 1.27 m; level enemy shots at the player's head pass over it.
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
        let mut sim = SearchlightSim::start(arena, head_aiming_cover_config());

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
        let mut sim = SearchlightSim::start(arena, head_aiming_cover_config());

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
    use crate::test_support::{DT, arena_with, flat_floor, mag, rifle_from, test_config};

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn draws(seed: u64) -> [u32; 4] {
        let mut rng = SimRng::new(seed, SPREAD_STREAM);
        std::array::from_fn(|_| (next_signed(&mut rng) * 2f32.powi(23) + 2f32.powi(23)) as u32)
    }

    #[test]
    fn spread_signed_draws_for_seed_7_are_pinned() {
        // Top 24 bits of the first four draws are 11041070, 1582717, 8475421, 1962916,
        // mapped to [-1, 1) as k / 2^24 * 2 - 1 (re-derived outside Rust).
        assert_eq!(draws(7), [11_041_070, 1_582_717, 8_475_421, 1_962_916]);
        let mut rng = SimRng::new(7, SPREAD_STREAM);
        assert_close(next_signed(&mut rng), 0.316_198);
        assert_close(next_signed(&mut rng), -0.811_325);
        assert_close(next_signed(&mut rng), 0.010_348);
        assert_close(next_signed(&mut rng), -0.766_002);
    }

    #[test]
    fn different_seeds_draw_different_spread() {
        assert_eq!(draws(0), [7_626_714, 13_892_238, 15_447_443, 11_324_399]);
        assert_ne!(draws(7), draws(0));
    }

    #[test]
    fn the_default_enemy_seed_is_zero() {
        assert_eq!(EnemyTuning::default().seed, 0);
    }

    #[test]
    fn a_zero_aim_error_never_moves_the_aim() {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.aim_error = 0.0;
        config.enemy.seed = 7;
        let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);
        let before = sim.rng.clone();

        let aimed = sim.with_spread(Vec3::new(0.0, 0.0, 5.0), sim.enemy.aim_error);

        assert!(aimed.abs_diff_eq(Vec3::Z, 1e-6), "{aimed:?}");
        assert_ne!(sim.rng, before, "the draws are still consumed");
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
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.aim_error = 0.5;
        config.enemy.seed = 7;
        let mut sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);

        let aimed = sim.with_spread(Vec3::new(0.0, 0.0, 5.0), sim.enemy.aim_error);

        // Facing +Z the right-hand axis is -X and up is +Y.
        assert!(
            aimed.abs_diff_eq(Vec3::new(-0.158_099, -0.405_663, 1.0), 1e-5),
            "{aimed:?}"
        );
    }

    #[test]
    fn the_sim_reports_the_configured_maximum_health() {
        let config = test_config(rifle_from(mag(30, 30)), vec![]);
        let sim = SearchlightSim::new(flat_floor(), Vec3::ZERO, &[], config);

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
        let config = test_config(rifle_from(mag(30, 30)), vec![]);

        let sim = SearchlightSim::start(Arc::new(arena), config);

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
        let config = test_config(rifle_from(mag(30, 30)), vec![]);
        let sim = SearchlightSim::new(
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
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, 0.72);
        let mut sim = SearchlightSim::start(arena, config);

        let mut feedback = Vec::new();
        for _ in 0..3 {
            sim.tick(&SearchlightCommand::default(), DT);
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

#[cfg(test)]
mod god_mode_tests {
    use super::*;
    use crate::test_support::{DT, arena_with, mag, rifle_from, test_config};

    fn duel_sim(aim_at: BodyPart) -> SearchlightSim {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[Vec3::new(0.0, 0.0, 10.0)],
            &[Vec3::new(0.0, 0.0, 6.0)],
        );
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, 0.72);
        config.enemy_aim_at = aim_at;
        SearchlightSim::start(arena, config)
    }

    fn command(god_mode: bool) -> SearchlightCommand {
        SearchlightCommand {
            god_mode,
            ..SearchlightCommand::default()
        }
    }

    /// Ticks `count` times under `god_mode`, returning all the feedback produced.
    fn run(sim: &mut SearchlightSim, count: usize, god_mode: bool) -> Vec<Feedback> {
        let mut feedback = Vec::new();
        for _ in 0..count {
            sim.tick(&command(god_mode), DT);
            feedback.extend(sim.drain_feedback());
        }
        feedback
    }

    fn hits_taken(feedback: &[Feedback]) -> usize {
        feedback
            .iter()
            .filter(|fb| matches!(fb, Feedback::PlayerHit { .. }))
            .count()
    }

    #[test]
    fn god_mode_enemy_hits_take_no_health() {
        let mut sim = duel_sim(BodyPart::Torso);

        let feedback = run(&mut sim, 60, true);

        assert!(hits_taken(&feedback) >= 5, "{feedback:?}");
        assert_eq!(sim.player_body().health(), 100);
        assert!(!sim.player_body().is_dead());
        assert!(!feedback.contains(&Feedback::PlayerDied));
    }

    #[test]
    fn god_mode_head_hits_never_kill() {
        let mut sim = duel_sim(BodyPart::Head);

        let feedback = run(&mut sim, 120, true);

        assert!(hits_taken(&feedback) >= 10, "{feedback:?}");
        assert!(!sim.player_body().is_dead());
        assert_eq!(sim.player_body().status(BodyPart::Head), Some(50));
        assert_eq!(sim.player_body().health(), 100);
        assert!(!feedback.contains(&Feedback::PlayerDied));
    }

    #[test]
    fn god_mode_limb_hits_never_disable_the_limb() {
        let mut sim = duel_sim(BodyPart::LeftLeg);

        let feedback = run(&mut sim, 120, true);

        assert!(hits_taken(&feedback) >= 10, "{feedback:?}");
        assert!(!sim.player_body().is_disabled(BodyPart::LeftLeg));
        assert_eq!(sim.player_body().status(BodyPart::LeftLeg), Some(60));
    }

    #[test]
    fn god_mode_hits_report_the_same_feedback_as_without_it() {
        let mut mortal = duel_sim(BodyPart::Torso);
        let mut immortal = duel_sim(BodyPart::Torso);

        let without = run(&mut mortal, 12, false);
        let with = run(&mut immortal, 12, true);

        assert_eq!(hits_taken(&without), 3);
        assert_eq!(with, without);
        assert_eq!(mortal.player_body().health(), 70);
        assert_eq!(immortal.player_body().health(), 100);
    }

    #[test]
    fn turning_god_mode_off_keeps_current_health() {
        let mut sim = duel_sim(BodyPart::Torso);

        run(&mut sim, 4, false);
        let before = sim.player_body().health();
        run(&mut sim, 8, true);
        let during = sim.player_body().health();
        run(&mut sim, 4, false);
        let after = sim.player_body().health();

        assert_eq!((before, during, after), (90, 90, 80));
    }

    #[test]
    fn god_mode_does_not_revive_a_dead_player() {
        let mut sim = duel_sim(BodyPart::Torso);
        sim.kill_player();

        run(&mut sim, 10, true);

        assert!(sim.player_body().is_dead());
    }
}

#[cfg(test)]
mod grade_tests {
    use moho_physics::CharacterCommand;

    use super::*;
    use crate::test_support::{DT, arena_with, mag, rifle_from, test_config};

    const ENEMY_SPAWN: Vec3 = Vec3::new(0.0, 0.0, 10.0);
    const TOWARD_PLAYER: Vec3 = Vec3::new(0.0, 0.0, 6.0);

    /// A 30-tick reaction and 12-tick cooldown, so grades separate.
    fn config() -> SimConfig {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, 0.72);
        config.enemy.reaction_ticks = 30;
        config.enemy.cooldown_ticks = 12;
        config.enemy.aim_error = 0.0;
        config
    }

    /// The player at the origin and one enemy of `grade` ten metres ahead, facing the player.
    fn duel(grade: Grade, config: SimConfig) -> SearchlightSim {
        let arena = arena_with(&[], Vec3::ZERO, &[ENEMY_SPAWN], &[TOWARD_PLAYER]);
        let mut sim = SearchlightSim::with_start(arena, config, Start::Empty);
        sim.spawn_enemy(0, grade).expect("the marker is free");
        sim
    }

    fn command(god_mode: bool) -> SearchlightCommand {
        SearchlightCommand {
            god_mode,
            ..SearchlightCommand::default()
        }
    }

    fn run(sim: &mut SearchlightSim, ticks: usize, god_mode: bool) -> Vec<Vec<Feedback>> {
        (0..ticks)
            .map(|_| {
                sim.tick(&command(god_mode), DT);
                sim.drain_feedback()
            })
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

    fn first_shot_tick(grade: Grade) -> Option<usize> {
        let mut sim = duel(grade, config());
        run(&mut sim, 150, true)
            .iter()
            .position(|tick| tick.iter().any(enemy_fired))
    }

    #[test]
    fn a_professional_fires_its_first_shot_before_a_rookie() {
        let rookie = first_shot_tick(Grade::Rookie);
        let experienced = first_shot_tick(Grade::Experienced);
        let professional = first_shot_tick(Grade::Professional);

        let (Some(rookie), Some(experienced), Some(professional)) =
            (rookie, experienced, professional)
        else {
            panic!(
                "all fire within 150 ticks: rookie {rookie:?}, experienced {experienced:?}, \
                 professional {professional:?}"
            );
        };
        assert!(
            professional < rookie,
            "professional {professional}, rookie {rookie}"
        );
        assert_eq!(rookie - experienced, 30, "base reaction 30 doubled to 60");
        assert_eq!(
            experienced - professional,
            15,
            "base reaction 30 halved to 15"
        );
    }

    fn shots_and_hits(grade: Grade) -> (usize, usize) {
        let mut config = config();
        // A rookie's 0.06 rad strays 0.6 m at this range, wider than the player's body.
        config.enemy.aim_error = 0.02;
        config.enemy.seed = 11;
        let mut sim = duel(grade, config);

        let ticks = run(&mut sim, 600, true);

        (count(&ticks, enemy_fired), count(&ticks, player_hit))
    }

    #[test]
    fn hits_on_the_player_never_fall_up_the_ladder() {
        let results: Vec<(Grade, (usize, usize))> = Grade::LADDER
            .into_iter()
            .map(|grade| (grade, shots_and_hits(grade)))
            .collect();

        for pair in results.windows(2) {
            let ((below, (_, below_hits)), (grade, (_, hits))) = (pair[0], pair[1]);
            assert!(
                hits >= below_hits,
                "{grade:?} hit {hits}, {below:?} hit {below_hits}: {results:?}"
            );
        }
        let (rookie_shots, rookie_hits) = results[0].1;
        let (professional_shots, professional_hits) = results[4].1;
        assert!(rookie_hits < rookie_shots, "the rookie never missed");
        assert_eq!(
            professional_hits, professional_shots,
            "the professional missed"
        );
        assert!(professional_hits > rookie_hits, "{results:?}");
    }

    #[test]
    fn a_professional_fires_more_shots_than_a_rookie_in_the_same_time() {
        // The ticks of each shot in the 120 ticks from the enemy's own first shot.
        let shot_ticks = |grade| -> Vec<usize> {
            let mut sim = duel(grade, config());
            let ticks = run(&mut sim, 300, true);
            let first = ticks
                .iter()
                .position(|tick| tick.iter().any(enemy_fired))
                .expect("the enemy fires within 300 ticks");
            ticks
                .iter()
                .enumerate()
                .skip(first)
                .take(120)
                .filter(|(_, tick)| tick.iter().any(enemy_fired))
                .map(|(at, _)| at)
                .collect()
        };
        let gaps = |ticks: &[usize]| -> Vec<usize> {
            ticks.windows(2).map(|pair| pair[1] - pair[0]).collect()
        };

        let rookie = shot_ticks(Grade::Rookie);
        let experienced = shot_ticks(Grade::Experienced);
        let professional = shot_ticks(Grade::Professional);

        assert!(rookie.len() > 1, "the rookie never fired twice");
        assert!(
            professional.len() > rookie.len(),
            "{professional:?} vs {rookie:?}"
        );
        assert!(gaps(&rookie).iter().all(|&gap| gap == 24), "{rookie:?}");
        assert!(
            gaps(&experienced).iter().all(|&gap| gap == 12),
            "{experienced:?}"
        );
        assert!(
            gaps(&professional).iter().all(|&gap| gap == 8),
            "{professional:?}"
        );
        assert!(experienced.len() > 1 && professional.len() > 1);
    }

    /// An enemy of `grade` out of sight of the player, as the only target.
    fn calm_enemy(grade: Grade) -> SearchlightSim {
        let mut config = config();
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        duel(grade, config)
    }

    /// The enemy of `grade`'s body after each hit of a fixed sequence, up to and including the
    /// killing one.
    fn bodies_until_dead(grade: Grade) -> Vec<Body> {
        let mut sim = calm_enemy(grade);
        let mut bodies = Vec::new();
        let sequence = [
            (BodyPart::Torso, 30),
            (BodyPart::LeftArm, 20),
            (BodyPart::Torso, 25),
            (BodyPart::Head, 10),
            (BodyPart::RightLeg, 30),
            (BodyPart::Torso, 30),
            (BodyPart::Torso, 30),
        ];
        let from = sim.player().position;
        for (part, damage) in sequence {
            sim.apply_player_hit(0, part, damage, from);
            let body = sim.enemy_body(0).expect("the enemy exists").clone();
            let dead = body.is_dead();
            bodies.push(body);
            if dead {
                return bodies;
            }
        }
        panic!("{grade:?} survived the whole sequence");
    }

    #[test]
    fn every_grade_dies_on_the_same_hit_as_an_experienced_enemy() {
        let experienced = bodies_until_dead(Grade::Experienced);

        assert!(experienced.len() > 1, "the sequence kills in one hit");
        for grade in [
            Grade::Rookie,
            Grade::Novice,
            Grade::Veteran,
            Grade::Professional,
        ] {
            let sim = calm_enemy(grade);
            assert_eq!(sim.enemy_grade(0), Some(grade));
            assert_eq!(bodies_until_dead(grade), experienced, "{grade:?}");
        }
    }

    fn health_after_first_hit(grade: Grade) -> u32 {
        let mut sim = duel(grade, config());
        assert_eq!(sim.enemy_grade(0), Some(grade));
        for _ in 0..150 {
            sim.tick(&command(false), DT);
            if sim.drain_feedback().iter().any(player_hit) {
                return sim.player_body().health();
            }
        }
        panic!("{grade:?} never hit the player");
    }

    #[test]
    fn one_hit_from_any_grade_costs_the_player_the_same_health() {
        let experienced = health_after_first_hit(Grade::Experienced);

        let rookie = health_after_first_hit(Grade::Rookie);
        let professional = health_after_first_hit(Grade::Professional);

        assert_eq!(experienced, 90);
        assert_eq!(rookie, experienced);
        assert_eq!(professional, experienced);
    }

    #[test]
    fn marker_start_and_restarted_enemies_are_experienced() {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[ENEMY_SPAWN, Vec3::new(3.0, 0.0, 10.0)],
            &[TOWARD_PLAYER],
        );
        let mut sim = SearchlightSim::start(arena, config());
        let at_start = (sim.enemy_grade(0), sim.enemy_grade(1), sim.enemy_grade(2));
        sim.kill_enemy(0);
        sim.tick(&command(true), DT);
        sim.spawn_enemy(0, Grade::Rookie)
            .expect("the dead enemy left marker 0");
        let spawned = sim.enemy_grade(2);
        sim.kill_player();

        sim.tick(
            &SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
            DT,
        );

        assert_eq!(
            at_start,
            (Some(Grade::Experienced), Some(Grade::Experienced), None)
        );
        assert_eq!(spawned, Some(Grade::Rookie));
        assert_eq!(sim.enemy_count(), 2);
        assert_eq!(sim.enemy_grade(0), Some(Grade::Experienced));
        assert_eq!(sim.enemy_grade(1), Some(Grade::Experienced));
        assert_eq!(sim.enemy_grade(2), None);
    }

    #[test]
    fn a_spawned_enemy_keeps_its_grade_after_it_dies() {
        let mut sim = duel(Grade::Veteran, config());
        assert_eq!(sim.enemy_grade(0), Some(Grade::Veteran));

        sim.kill_enemy(0);
        sim.tick(&command(false), DT);

        assert_eq!(sim.enemies_alive(), 0);
        assert_eq!(sim.enemy_grade(0), Some(Grade::Veteran));
    }

    /// Low enough that a shot at head height passes over it, high enough to hide an enemy
    /// standing behind it from the player.
    const LOW_WALL: (Vec3, Vec3) = (Vec3::new(0.5, 0.0, 9.0), Vec3::new(9.0, 1.2, 9.2));
    /// Behind `LOW_WALL` as seen from the player.
    const SHIELDED: Vec3 = Vec3::new(4.0, 0.0, 10.0);
    /// A second point behind `LOW_WALL`, for a second enemy to run to.
    const SECOND_SHIELDED: Vec3 = Vec3::new(6.0, 0.0, 10.0);
    /// Closer to the shot at `ENEMY_SPAWN` than the radius, and clear of its capsule.
    const BESIDE_SPAWN: Vec3 = Vec3::new(-1.0, 0.0, 10.0);
    /// Between the player and the enemy at the full width of the arena.
    const TALL_WALL: (Vec3, Vec3) = (Vec3::new(-6.0, 0.0, 5.0), Vec3::new(6.0, 4.0, 5.2));
    /// Metres off the enemy's centre of a shot that passes close to it.
    const NEAR: f32 = 1.0;
    /// Metres off the enemy's centre of a shot that lands far from it.
    const FAR: f32 = 4.0;
    /// Ticks between pulls, longer than the rifle's cooldown.
    const PULL_GAP: usize = 4;

    /// Shots into the open, a player that outlives the enemies' fire and enemies that stand
    /// where they are until something moves them.
    fn cover_config() -> SimConfig {
        let mut config = config();
        config.enemy.damage = 1;
        config.enemy.patrol_speed = 0.0;
        config.body.max_health = 1_000_000;
        config
    }

    /// The player at the origin and one enemy of `grade` ten metres ahead, with a shielded
    /// cover point in reach and `boxes` in the way.
    fn cover_duel(grade: Grade, boxes: &[(Vec3, Vec3)]) -> SearchlightSim {
        cover_duel_with(grade, boxes, cover_config())
    }

    fn cover_duel_with(grade: Grade, boxes: &[(Vec3, Vec3)], config: SimConfig) -> SearchlightSim {
        let mut arena = (*arena_with(boxes, Vec3::ZERO, &[ENEMY_SPAWN], &[TOWARD_PLAYER])).clone();
        arena.covers = vec![SHIELDED];
        let mut sim = SearchlightSim::with_start(Arc::new(arena), config, Start::Empty);
        sim.spawn_enemy(0, grade).expect("the marker is free");
        sim
    }

    /// A trigger pull with the player turned to pass `wide` metres to the side of the enemy
    /// at `ENEMY_SPAWN`; the player's rifle has no spread.
    fn fire_wide(sim: &SearchlightSim, wide: f32) -> SearchlightCommand {
        fire_past(sim, ENEMY_SPAWN, wide)
    }

    /// A trigger pull passing `wide` metres to the -x side of the floor point `at`.
    fn fire_past(sim: &SearchlightSim, at: Vec3, wide: f32) -> SearchlightCommand {
        let from = sim.player().position;
        let distance = horizontal(at, from);
        let bearing = (at.x - from.x).atan2(at.z - from.z) - (wide / distance).asin();
        SearchlightCommand {
            movement: CharacterCommand {
                yaw_delta: bearing - sim.player().yaw,
                ..CharacterCommand::default()
            },
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn horizontal(a: Vec3, b: Vec3) -> f32 {
        Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
    }

    fn enemy_at(sim: &SearchlightSim, index: usize) -> Vec3 {
        sim.enemy_state(index).expect("the enemy is alive").position
    }

    fn at_shielded_point(sim: &SearchlightSim) -> bool {
        horizontal(enemy_at(sim, 0), SHIELDED) < 1.0
    }

    /// Plays `ticks` ticks with a shot `wide` metres off the enemy on each tick in `pulls`
    /// (counted from now), and returns the first tick the enemy stood at the shielded point.
    fn play(sim: &mut SearchlightSim, ticks: usize, pulls: &[usize], wide: f32) -> Option<usize> {
        play_until(sim, ticks, pulls, wide, at_shielded_point)
    }

    /// As [`play`], returning the first tick `done` holds after.
    fn play_until(
        sim: &mut SearchlightSim,
        ticks: usize,
        pulls: &[usize],
        wide: f32,
        done: impl Fn(&SearchlightSim) -> bool,
    ) -> Option<usize> {
        for tick in 0..ticks {
            let command = if pulls.contains(&tick) {
                fire_wide(sim, wide)
            } else {
                SearchlightCommand::default()
            };
            sim.tick(&command, DT);
            sim.drain_feedback();
            if done(sim) {
                return Some(tick);
            }
        }
        None
    }

    fn tick_with(sim: &mut SearchlightSim, command: &SearchlightCommand) {
        sim.tick(command, DT);
        sim.drain_feedback();
    }

    fn idle_for(sim: &mut SearchlightSim, ticks: usize) {
        for _ in 0..ticks {
            tick_with(sim, &SearchlightCommand::default());
        }
    }

    /// `count` pulls, `PULL_GAP` ticks apart.
    fn pulls(count: usize) -> Vec<usize> {
        (0..count).map(|shot| shot * PULL_GAP).collect()
    }

    fn enemy_target(sim: &SearchlightSim, index: usize) -> &Target {
        &sim.targets[sim.dummy_count + index]
    }

    fn claim_of(sim: &SearchlightSim, index: usize) -> Option<usize> {
        enemy_target(sim, index)
            .enemy
            .expect("the target is an enemy")
            .brain
            .claim()
    }

    #[test]
    fn a_professional_reaches_cover_before_a_rookie_under_the_same_near_misses() {
        // Both enemies take the same near misses, then the same wound on the same tick.
        const WOUND_TICK: usize = 20;
        let arrival = |grade: Grade| {
            let mut sim = cover_duel(grade, &[LOW_WALL]);
            let from = sim.player().position;
            for tick in 0..400 {
                let command = if tick < WOUND_TICK && pulls(3).contains(&tick) {
                    fire_wide(&sim, NEAR)
                } else {
                    SearchlightCommand::default()
                };
                if tick == WOUND_TICK {
                    sim.apply_player_hit(0, BodyPart::Head, 30, from);
                }
                tick_with(&mut sim, &command);
                if at_shielded_point(&sim) {
                    return Some(tick);
                }
            }
            None
        };

        let professional = arrival(Grade::Professional);
        let rookie = arrival(Grade::Rookie);

        let (Some(professional), Some(rookie)) = (professional, rookie) else {
            panic!("both reach cover: professional {professional:?}, rookie {rookie:?}");
        };
        assert!(
            professional < rookie,
            "professional at tick {professional}, rookie at tick {rookie}"
        );
    }

    #[test]
    fn one_near_miss_sends_a_professional_to_cover() {
        let mut sim = cover_duel(Grade::Professional, &[LOW_WALL]);

        let reached = play(&mut sim, 300, &[0], NEAR);

        assert!(
            reached.is_some(),
            "the professional stayed at {:?}",
            enemy_at(&sim, 0)
        );
    }

    #[test]
    fn near_misses_alone_leave_a_rookie_where_it_stands() {
        let mut rookie = cover_duel(Grade::Rookie, &[LOW_WALL]);
        let mut professional = cover_duel(Grade::Professional, &[LOW_WALL]);

        let rookie_at = play(&mut rookie, 300, &pulls(6), NEAR);
        let professional_at = play(&mut professional, 300, &pulls(6), NEAR);

        assert!(
            professional_at.is_some(),
            "the same shots did not move a professional, so they were not near misses"
        );
        assert_eq!(rookie_at, None);
        assert!(
            horizontal(enemy_at(&rookie, 0), ENEMY_SPAWN) < 0.5,
            "the rookie moved to {:?}",
            enemy_at(&rookie, 0)
        );
        assert_eq!(
            claim_of(&rookie, 0),
            None,
            "the rookie claimed a cover point"
        );
    }

    #[test]
    fn a_hit_sends_a_rookie_to_cover() {
        let mut sim = cover_duel(Grade::Rookie, &[LOW_WALL]);

        let reached = play(&mut sim, 300, &[0], 0.0);

        assert!(
            reached.is_some(),
            "the rookie stayed at {:?}",
            enemy_at(&sim, 0)
        );
        assert_eq!(sim.enemy_body(0).map(Body::is_dead), Some(false));
    }

    #[test]
    fn a_shot_wide_of_the_radius_leaves_a_professional_in_the_open() {
        let mut near = cover_duel(Grade::Professional, &[LOW_WALL]);
        let mut far = cover_duel(Grade::Professional, &[LOW_WALL]);

        let near_at = play(&mut near, 300, &pulls(3), NEAR);
        let far_at = play(&mut far, 300, &pulls(3), FAR);

        assert!(
            near_at.is_some(),
            "the control shots 1 m wide did not move the professional"
        );
        assert_eq!(
            far_at, None,
            "shots 4 m wide sent the professional to cover"
        );
        assert!(
            horizontal(enemy_at(&far, 0), ENEMY_SPAWN) < 0.5,
            "the professional moved to {:?}",
            enemy_at(&far, 0)
        );
    }

    #[test]
    fn a_novice_runs_for_cover_on_its_third_near_miss() {
        let mut sim = cover_duel(Grade::Novice, &[LOW_WALL]);

        let after_two = play(&mut sim, 60, &pulls(2), NEAR);
        let held = horizontal(enemy_at(&sim, 0), ENEMY_SPAWN);
        let after_three = play(&mut sim, 300, &[0], NEAR);

        assert_eq!(after_two, None, "the novice ran after two near misses");
        assert!(held < 0.5, "the novice moved to {:?}", enemy_at(&sim, 0));
        assert!(
            after_three.is_some(),
            "the novice stayed at {:?} after its third near miss",
            enemy_at(&sim, 0)
        );
    }

    #[test]
    fn a_shot_stopped_short_by_a_wall_is_not_a_near_miss() {
        let mut open = cover_duel(Grade::Professional, &[LOW_WALL]);
        let mut walled = cover_duel(Grade::Professional, &[LOW_WALL, TALL_WALL]);

        let open_at = play(&mut open, 300, &pulls(3), NEAR);
        let walled_at = play(&mut walled, 300, &pulls(3), NEAR);

        assert!(
            open_at.is_some(),
            "the control shots without the wall did not move the professional"
        );
        assert_eq!(
            walled_at, None,
            "a shot the wall stopped scared the professional"
        );
        assert!(
            horizontal(enemy_at(&walled, 0), ENEMY_SPAWN) < 0.5,
            "the professional moved to {:?}",
            enemy_at(&walled, 0)
        );

        let mut open = cover_duel(Grade::Experienced, &[LOW_WALL]);
        let mut walled = cover_duel(Grade::Experienced, &[LOW_WALL, TALL_WALL]);
        play(&mut open, 30, &pulls(1), NEAR);
        play(&mut walled, 30, &pulls(1), NEAR);

        assert_eq!(enemy_target(&open, 0).near_misses, 1, "the control shot");
        assert_eq!(
            enemy_target(&walled, 0).near_misses,
            0,
            "a shot the wall stopped counted as a near miss"
        );
    }

    #[test]
    fn the_enemy_hit_does_not_also_take_a_near_miss() {
        let arena = arena_with(
            &[LOW_WALL],
            Vec3::ZERO,
            &[ENEMY_SPAWN, BESIDE_SPAWN],
            &[TOWARD_PLAYER],
        );
        let mut arena = (*arena).clone();
        arena.covers = vec![SHIELDED, SECOND_SHIELDED];
        let mut sim = SearchlightSim::with_start(Arc::new(arena), cover_config(), Start::Empty);
        sim.spawn_enemy(0, Grade::Experienced)
            .expect("the first marker is free");
        sim.spawn_enemy(1, Grade::Experienced)
            .expect("the second marker is free");
        let in_cover = |position: Vec3| {
            [SHIELDED, SECOND_SHIELDED]
                .iter()
                .any(|cover| horizontal(position, *cover) < 1.0)
        };

        let reached = play_until(&mut sim, 300, &[0], 0.0, |sim| in_cover(enemy_at(sim, 0)));
        let held = horizontal(enemy_at(&sim, 1), BESIDE_SPAWN);
        let first_shot_count = enemy_target(&sim, 0).near_misses;
        let second_shot = fire_past(&sim, BESIDE_SPAWN, NEAR);
        tick_with(&mut sim, &second_shot);
        idle_for(&mut sim, 300);

        assert!(
            reached.is_some(),
            "the struck enemy stayed at {:?}",
            enemy_at(&sim, 0)
        );
        assert_eq!(first_shot_count, 0, "the struck enemy took a near miss");
        assert!(
            held < 0.5,
            "the first shot, which hit the other enemy, moved the experienced bystander"
        );
        assert!(
            in_cover(enemy_at(&sim, 1)),
            "the bystander did not run for cover on its second near miss: {:?}",
            enemy_at(&sim, 1)
        );
    }

    #[test]
    fn a_near_miss_count_starts_again_after_it_sends_an_enemy_to_cover() {
        let mut sim = cover_duel(Grade::Experienced, &[LOW_WALL]);
        let mut near_misses_after_shot = Vec::new();

        for _ in 0..3 {
            let at = enemy_at(&sim, 0);
            let shot = fire_past(&sim, at, NEAR);
            tick_with(&mut sim, &shot);
            near_misses_after_shot.push(enemy_target(&sim, 0).near_misses);
            idle_for(&mut sim, 10);
        }

        assert_eq!(near_misses_after_shot, [1, 0, 1]);
    }

    #[test]
    fn a_shot_that_strikes_a_wall_beside_an_enemy_is_a_near_miss() {
        // The shot ends on this box's face at about (-1.0, 1.9, 10.4), 1.3 m from the enemy.
        let beside = (Vec3::new(-1.6, 0.0, 10.4), Vec3::new(-0.8, 3.0, 10.8));
        let mut sim = cover_duel(Grade::Professional, &[LOW_WALL, beside]);

        let shot = fire_wide(&sim, NEAR);
        tick_with(&mut sim, &shot);
        let player = sim.player();
        let strike = resolve_shot(
            &sim.world,
            ShotRay {
                origin: player.eye_position(),
                direction: Vec3::new(player.yaw.sin(), 0.0, player.yaw.cos()),
                range: sim.range,
                ignore: Some(sim.player),
            },
        )
        .expect("the shot strikes the box");
        let reached = play(&mut sim, 300, &[], NEAR);

        assert_eq!(strike.tag, SurfaceTag::World);
        assert!(
            (10.3..10.6).contains(&strike.point.z),
            "the shot did not end on the box: {strike:?}"
        );
        assert!(
            reached.is_some(),
            "the professional stayed at {:?}",
            enemy_at(&sim, 0)
        );
    }

    #[test]
    fn a_dead_enemy_and_a_dummy_take_no_near_miss() {
        let mut arena =
            (*arena_with(&[LOW_WALL], Vec3::ZERO, &[ENEMY_SPAWN], &[TOWARD_PLAYER])).clone();
        arena.covers = vec![SHIELDED];
        // Within the radius of the shot but clear of its path.
        arena.dummy_spawns = vec![Vec3::new(0.0, 0.0, 11.5)];
        let mut sim = SearchlightSim::with_start(Arc::new(arena), cover_config(), Start::Empty);
        sim.spawn_enemy(0, Grade::Experienced)
            .expect("the marker is free");
        sim.kill_enemy(0);
        tick_with(&mut sim, &SearchlightCommand::default());
        let shot = fire_wide(&sim, NEAR);

        tick_with(&mut sim, &shot);
        idle_for(&mut sim, 30);

        assert_eq!(sim.enemies_alive(), 0);
        assert_eq!(enemy_target(&sim, 0).near_misses, 0, "the dead enemy");
        assert_eq!(sim.targets[0].near_misses, 0, "the dummy");
    }

    #[test]
    fn the_near_miss_radius_sets_how_close_a_shot_must_pass() {
        let mut tight = cover_config();
        tight.enemy.near_miss_radius = 0.5;
        let mut wide = cover_config();
        wide.enemy.near_miss_radius = 5.0;
        let mut ignored = cover_duel_with(Grade::Professional, &[LOW_WALL], tight);
        let mut counted = cover_duel_with(Grade::Professional, &[LOW_WALL], wide);

        let ignored_at = play(&mut ignored, 300, &pulls(3), NEAR);
        let counted_at = play(&mut counted, 300, &pulls(3), FAR);

        assert!(
            counted_at.is_some(),
            "a 5 m radius did not count a shot 4 m wide"
        );
        assert_eq!(ignored_at, None, "a 0.5 m radius counted a shot 1 m wide");
    }
}
