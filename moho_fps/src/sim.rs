//! The fixed-tick simulation: player, rifle, dummies and the feedback they produce.
//! It holds no renderer, audio or UI handle.

use glam::Vec3;
use moho_fps_rules::{Body, BodyPart, BodyProfile, HitReport, Magazine, MagazineLevel, Rifle};
use moho_physics::{CharacterState, HitVolume, MoveSettings, PhysicsWorld};

use crate::input::FpsCommand;

/// Height of a character's capsule centre above its feet, in metres.
pub const STAND_HEIGHT: f32 = 1.15;

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
pub fn dummy_hit_volumes(_target: u16) -> [HitVolume; 6] {
    unimplemented!("one box per body part around the capsule centre")
}

/// The world, the player, the rifle and the dummies.
#[derive(Debug)]
pub struct FpsSim {
    _private: (),
}

impl FpsSim {
    /// A sim over `world` (static geometry already in), the player standing at
    /// `player_spawn` and one dummy at each of `dummy_spawns`, numbered by position in the
    /// slice. Spawns are floor points: capsule centres sit [`STAND_HEIGHT`] above them.
    pub fn new(
        _world: PhysicsWorld,
        _player_spawn: Vec3,
        _dummy_spawns: &[Vec3],
        _config: SimConfig,
    ) -> Self {
        unimplemented!("place the player and dummies, give each dummy its hit volumes")
    }

    /// Runs one tick of `dt` seconds: moves the player, steps the world, then applies the
    /// weapon actions in `command`.
    pub fn tick(&mut self, _command: &FpsCommand, _dt: f32) {
        unimplemented!("move, step, then weapon actions")
    }

    /// The player's state after the last tick.
    pub fn player(&self) -> CharacterState {
        unimplemented!("read the player character")
    }

    pub fn rifle(&self) -> &Rifle {
        unimplemented!("the player's rifle")
    }

    /// Magazines left to reload from.
    pub fn spares(&self) -> &[Magazine] {
        unimplemented!("the spare magazines")
    }

    /// The body of dummy `target`, or `None` if there is no such dummy.
    pub fn dummy_body(&self, _target: u16) -> Option<&Body> {
        unimplemented!("the dummy's body")
    }

    /// Feedback produced and not yet drained.
    pub fn pending_feedback(&self) -> &[Feedback] {
        unimplemented!("the outbox")
    }

    /// Takes all pending feedback, oldest first.
    pub fn drain_feedback(&mut self) -> Vec<Feedback> {
        unimplemented!("empty the outbox")
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
