//! The fixed-tick simulation: player, rifle, dummies and the feedback they produce.
//! It holds no renderer, audio or UI handle.

use glam::Vec3;
use moho_fps_rules::{
    Body, BodyPart, BodyProfile, HitReport, Magazine, MagazineLevel, Rifle, Shot,
};
use moho_physics::{
    CharacterHandle, CharacterState, HitVolume, MoveSettings, PhysicsWorld, first_person_view,
};

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
    /// Pools of every dummy's body.
    pub body: BodyProfile,
    pub rifle: Rifle,
    /// Magazines the player can reload from, used first to last.
    pub spares: Vec<Magazine>,
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
}

/// The six hit volumes of a dummy numbered `target`, one per body part, in the body frame
/// around the capsule centre. Each volume's tag is `SurfaceTag::Part` for that part.
///
/// The body faces +Z at yaw 0, so its left side is +X. The boxes stand inside the capsule's
/// height and sit edge to edge, so a ray at any height inside the body meets a part.
pub fn dummy_hit_volumes(target: u16) -> [HitVolume; 6] {
    let volume = |part, offset: Vec3, half_extents: Vec3| HitVolume {
        offset,
        half_extents,
        tag: SurfaceTag::Part { target, part }.into(),
    };
    let arm = Vec3::new(0.1, 0.3, 0.1);
    let leg = Vec3::new(0.1, 0.525, 0.1);
    [
        volume(
            BodyPart::Head,
            Vec3::new(0.0, 0.85, 0.0),
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

/// The world, the player, the rifle and the dummies.
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
    /// Indexed by target number.
    dummies: Vec<Body>,
    weapon: Weapon,
    outbox: Vec<Feedback>,
}

impl FpsSim {
    /// A sim over `world` (static geometry already in), the player standing at
    /// `player_spawn` and one dummy at each of `dummy_spawns`, numbered by position in the
    /// slice. Spawns are floor points: capsule centres sit [`STAND_HEIGHT`] above them.
    ///
    /// # Panics
    /// If `dummy_spawns` holds more dummies than a `u16` target number can name.
    pub fn new(
        mut world: PhysicsWorld,
        player_spawn: Vec3,
        dummy_spawns: &[Vec3],
        config: SimConfig,
    ) -> Self {
        let player = world.add_character(player_spawn + Vec3::Y * STAND_HEIGHT);
        for (index, &spawn) in dummy_spawns.iter().enumerate() {
            let (target, centre) = dummy_at(index, spawn);
            let dummy = world.add_character(centre);
            for volume in dummy_hit_volumes(target) {
                world
                    .add_hit_volume(dummy, volume)
                    .expect("the dummy was added a moment ago");
            }
        }
        Self {
            world,
            player,
            move_settings: config.move_settings,
            eye_height: config.eye_height,
            range: config.range,
            damage: config.damage,
            cooldown_ticks: config.cooldown_ticks,
            reload_ticks: config.reload_ticks,
            rifle: config.rifle,
            spares: config.spares,
            dummies: dummy_spawns
                .iter()
                .map(|_| Body::new(config.body))
                .collect(),
            weapon: Weapon::Ready,
            outbox: Vec::new(),
        }
    }

    /// Runs one tick of `dt` seconds: moves the player, steps the world, then applies the
    /// weapon actions in `command`.
    pub fn tick(&mut self, command: &FpsCommand, dt: f32) {
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
        if let Some(Strike {
            tag: SurfaceTag::Part { target, part },
            ..
        }) = strike
            && let Some(body) = self.dummies.get_mut(usize::from(target))
        {
            let report = body.hit(part, self.damage);
            self.outbox.push(Feedback::Hit {
                target,
                part,
                report,
            });
        }
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
        self.dummies.get(usize::from(target))
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

    use moho_fps_rules::MagazineLevel;
    use moho_physics::CharacterCommand;

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
    use moho_fps_rules::BodyPart;

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
    use moho_fps_rules::{BodyPart, MagazineLevel};
    use moho_physics::{CharacterCommand, PITCH_LIMIT};

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
