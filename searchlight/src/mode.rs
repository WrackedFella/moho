//! The arena as a game mode: waves of enemies that spawn at the enemy markers, and a score for
//! the kills. The sim underneath knows neither.

use std::sync::Arc;

use searchlight_rules::Grade;

use crate::arena::{Arena, LevelMode};
use crate::control::ControlAction;
use crate::input::SearchlightCommand;
use crate::sim::{Feedback, SearchlightSim, SimConfig, Start};
use crate::wave::{WaveState, WaveTuning};

/// The running wave, counting from 1, and the points scored since the last restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub wave: u32,
    pub score: u32,
}

fn actions_of(arena: &Arena) -> Vec<ControlAction> {
    arena
        .controls
        .iter()
        .map(|control| control.action)
        .collect()
}

/// The waves of a level that has them.
#[derive(Debug)]
struct Waves {
    state: WaveState,
    tuning: WaveTuning,
    /// Kills of the sim already scored.
    seen_kills: u32,
}

/// A sim that starts empty and, in an arena, is fed waves of enemies as the player clears them.
#[derive(Debug)]
pub struct ArenaMode {
    sim: SearchlightSim,
    /// `None` in a sandbox: nothing spawns and nothing is scored.
    waves: Option<Waves>,
    /// What each of the arena's controls does, by control index.
    actions: Vec<ControlAction>,
}

/// The player slot the one player of the sim uses, until the sim holds more than one.
const HOST_SLOT: u8 = 0;

impl ArenaMode {
    /// A mode over `arena` whose first wave is already standing, as far as the enemy markers
    /// allow, before the first tick.
    pub fn new(arena: Arc<Arena>, config: SimConfig, waves: WaveTuning) -> Self {
        let mut mode = Self {
            actions: actions_of(&arena),
            sim: SearchlightSim::with_start(arena, config, Start::Empty),
            waves: Some(Waves {
                state: WaveState::begin(&waves),
                tuning: waves,
                seen_kills: 0,
            }),
        };
        mode.step_waves(None);
        mode
    }

    /// Scores the sim's new kills and spawns the wave's enemies at the first free markers, each
    /// of `forced` or else the grade the wave's mix gives its place in the wave; none while the
    /// player is dead. Does nothing in a sandbox.
    fn step_waves(&mut self, forced: Option<Grade>) {
        let Some(waves) = self.waves.as_mut() else {
            return;
        };
        let kills = self.sim.kills();
        let scan = !self.sim.player_body().is_dead()
            && (waves.state.to_spawn() != 0 || self.sim.enemies_alive() == 0);
        let free = if scan {
            self.sim.free_enemy_spawns()
        } else {
            Vec::new()
        };
        let spawn = waves.state.step(
            &waves.tuning,
            kills - waves.seen_kills,
            self.sim.enemies_alive(),
            free.len(),
        );
        waves.seen_kills = kills;
        let grades = waves.state.grades();
        let first = waves.state.spawned() - spawn;
        let mut spawned = 0;
        for (&marker, index) in free.iter().zip(first..first + spawn) {
            let grade = forced
                .or_else(|| grades.grade_of(index))
                .unwrap_or_default();
            if let Err(error) = self.sim.spawn_enemy(marker, grade) {
                tracing::warn!(%error, marker, "wave enemy not spawned");
                break;
            }
            spawned += 1;
        }
        waves.state.unspawned(spawn - spawned);
    }

    /// Runs one tick of `dt` seconds: the sim, then the score of its kills and the wave's
    /// spawns. A restart of a dead player begins wave 1 again with no score.
    pub fn tick(&mut self, command: &SearchlightCommand, dt: f32) {
        let restarting = command.restart && self.sim.player_body().is_dead();
        self.sim.tick(command, dt);
        if restarting && let Some(waves) = self.waves.as_mut() {
            waves.state = WaveState::begin(&waves.tuning);
            waves.seen_kills = 0;
        }
        if command.use_pressed
            && let Some(action) = self.control_in_reach()
            && action.honored_from(HOST_SLOT)
        {
            self.apply(action);
        }
        self.step_waves(command.forced_grade);
    }

    /// The action of the control the player is looking at within reach, if any.
    pub fn control_in_reach(&self) -> Option<ControlAction> {
        let index = self.sim.control_in_reach()?;
        self.actions.get(usize::from(index)).copied()
    }

    /// Does `action`; calling the next wave does nothing in a sandbox or while enemies remain.
    fn apply(&mut self, action: ControlAction) {
        match action {
            ControlAction::NextWave => {
                if let Some(waves) = self.waves.as_mut() {
                    waves
                        .state
                        .call_next(&waves.tuning, self.sim.enemies_alive());
                }
            }
        }
    }

    pub fn sim(&self) -> &SearchlightSim {
        &self.sim
    }

    /// Takes the sim's pending feedback, oldest first.
    pub fn drain_feedback(&mut self) -> Vec<Feedback> {
        self.sim.drain_feedback()
    }

    /// The wave and score, or `None` in a sandbox, which has neither.
    pub fn tally(&self) -> Option<Tally> {
        self.waves.as_ref().map(|waves| Tally {
            wave: waves.state.wave(),
            score: waves.state.score(),
        })
    }

    /// A mode over `arena` with no waves: nothing spawns and nothing is scored.
    pub fn sandbox(arena: Arc<Arena>, config: SimConfig) -> Self {
        Self {
            actions: actions_of(&arena),
            sim: SearchlightSim::with_start(arena, config, Start::Empty),
            waves: None,
        }
    }

    /// The mode a level of `mode` plays as: [`ArenaMode::new`] for an arena,
    /// [`ArenaMode::sandbox`] for a sandbox.
    pub fn for_level(
        arena: Arc<Arena>,
        mode: LevelMode,
        config: SimConfig,
        waves: WaveTuning,
    ) -> Self {
        match mode {
            LevelMode::Arena => Self::new(arena, config, waves),
            LevelMode::Sandbox => Self::sandbox(arena, config),
        }
    }

    #[cfg(test)]
    pub(crate) fn sim_mut(&mut self) -> &mut SearchlightSim {
        &mut self.sim
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use searchlight_rules::GradeMix;

    use super::*;
    use crate::sim::STAND_HEIGHT;
    use crate::test_support::{
        DT, WaveScore, arena_with, arena_with_controls, mag, rifle_from, test_config,
    };

    fn idle() -> SearchlightCommand {
        SearchlightCommand::default()
    }

    /// The use input pressed, with everything else as `command` has it.
    fn using(command: &SearchlightCommand) -> SearchlightCommand {
        SearchlightCommand {
            use_pressed: true,
            ..*command
        }
    }

    /// Where the next-wave button stands in front of the player, who starts facing +Z.
    const BUTTON: Vec3 = Vec3::new(0.0, 0.0, 1.5);

    /// One tick with use pressed and `command` otherwise: the player presses the button.
    fn press_the_button_with(mode: &mut ArenaMode, command: &SearchlightCommand) {
        mode.tick(&using(command), DT);
    }

    fn press_the_button(mode: &mut ArenaMode) {
        press_the_button_with(mode, &idle());
    }

    /// The player at the origin turned to face away from the button.
    fn turning_away() -> SearchlightCommand {
        SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                yaw_delta: std::f32::consts::PI,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        }
    }

    /// Kills every enemy standing and ticks once, so the wave is cleared and its kills scored.
    fn kill_everyone(mode: &mut ArenaMode) {
        for index in 0..mode.sim().enemy_count() {
            if mode.sim().enemy_state(index).is_some() {
                mode.sim_mut().kill_enemy(index);
            }
        }
        mode.tick(&idle(), DT);
    }

    fn restart() -> SearchlightCommand {
        SearchlightCommand {
            restart: true,
            ..SearchlightCommand::default()
        }
    }

    /// `count` markers in a row, three metres apart, well clear of the player at the origin.
    fn markers(count: usize) -> Vec<Vec3> {
        (0..count)
            .map(|index| Vec3::new(-9.0 + 3.0 * index as f32, 0.0, 12.0))
            .collect()
    }

    /// Enemies that neither see the player nor leave their markers.
    fn calm_config() -> SimConfig {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        config
    }

    fn tuning(first_size: u32, growth: u32, score_per_kill: u32) -> WaveTuning {
        mixed_tuning(first_size, growth, score_per_kill)
            .with_mix(GradeMix::only(Grade::Experienced))
    }

    fn mixed_tuning(first_size: u32, growth: u32, score_per_kill: u32) -> WaveTuning {
        WaveTuning::new(first_size, growth, score_per_kill).expect("a valid tuning")
    }

    /// A mode with the next-wave button at `button` and `walls` as world boxes.
    fn calm_mode_with_button(
        button: Vec3,
        walls: &[(Vec3, Vec3)],
        marker_count: usize,
        waves: WaveTuning,
    ) -> ArenaMode {
        let arena = arena_with_controls(
            walls,
            Vec3::ZERO,
            &markers(marker_count),
            &[Vec3::new(0.0, 0.0, 25.0)],
            &[button],
        );
        ArenaMode::new(arena, calm_config(), waves)
    }

    /// A mode with the next-wave button within reach in front of the player.
    fn calm_mode(marker_count: usize, waves: WaveTuning) -> ArenaMode {
        calm_mode_with_button(BUTTON, &[], marker_count, waves)
    }

    fn assert_enemy_at(mode: &ArenaMode, index: usize, marker: Vec3) {
        let state = mode
            .sim()
            .enemy_state(index)
            .unwrap_or_else(|| panic!("enemy {index} is not in the world"));
        assert!(
            (state.position.x - marker.x).abs() < 1e-3
                && (state.position.z - marker.z).abs() < 1e-3
                && (state.position.y - STAND_HEIGHT).abs() < 1e-3,
            "enemy {index} at {:?}, marker {marker:?}",
            state.position
        );
    }

    #[test]
    fn the_first_wave_spawns_at_enemy_markers() {
        let markers = markers(3);

        let mode = calm_mode(3, tuning(2, 2, 100));

        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.score(), 0);
        assert_eq!(mode.sim().enemy_count(), 2);
        assert_eq!(mode.sim().enemies_alive(), 2);
        assert_enemy_at(&mode, 0, markers[0]);
        assert_enemy_at(&mode, 1, markers[1]);
        assert_eq!(mode.sim().free_enemy_spawns(), [2]);
    }

    #[test]
    fn using_the_button_after_a_clear_spawns_wave_two_at_the_markers() {
        let markers = markers(4);
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        let waiting = (mode.wave(), mode.sim().enemy_count());

        press_the_button(&mut mode);

        assert_eq!(waiting, (1, 1));
        assert_eq!(mode.wave(), 2);
        assert_eq!(mode.score(), 100);
        assert_eq!(mode.sim().enemies_alive(), 3);
        assert_eq!(mode.sim().enemy_count(), 4);
        assert_enemy_at(&mode, 1, markers[0]);
        assert_enemy_at(&mode, 2, markers[1]);
        assert_enemy_at(&mode, 3, markers[2]);
    }

    #[test]
    fn a_wave_with_enemies_left_stays_the_wave() {
        let mut mode = calm_mode(4, tuning(2, 2, 100));
        mode.sim_mut().kill_enemy(0);

        mode.tick(&idle(), DT);

        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemies_alive(), 1);
        assert_eq!(mode.sim().enemy_count(), 2);
    }

    #[test]
    fn a_kill_in_the_arena_scores() {
        let mut mode = calm_mode(4, tuning(3, 1, 50));
        assert_eq!(mode.score(), 0);
        mode.sim_mut().kill_enemy(0);

        mode.tick(&idle(), DT);
        let after_one = mode.score();
        mode.tick(&idle(), DT);
        let after_a_quiet_tick = mode.score();
        mode.sim_mut().kill_enemy(1);
        mode.tick(&idle(), DT);

        assert_eq!((after_one, after_a_quiet_tick), (50, 50));
        assert_eq!(mode.score(), 100);
        assert_eq!(mode.wave(), 1);
    }

    #[test]
    fn two_kills_in_one_tick_both_score() {
        let mut mode = calm_mode(4, tuning(3, 1, 50));
        mode.sim_mut().kill_enemy(0);
        mode.sim_mut().kill_enemy(1);

        mode.tick(&idle(), DT);

        assert_eq!(mode.score(), 100);
    }

    #[test]
    fn restart_after_death_returns_to_wave_one_with_no_score() {
        let markers = markers(4);
        let mut mode = calm_mode(4, tuning(1, 1, 100));
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        press_the_button(&mut mode);
        mode.sim_mut().kill_enemy(1);
        mode.sim_mut().kill_enemy(2);
        mode.tick(&idle(), DT);
        press_the_button(&mut mode);
        assert_eq!((mode.wave(), mode.score()), (3, 300));
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);
        assert_eq!((mode.wave(), mode.score()), (3, 300));

        mode.tick(&restart(), DT);
        let right_after = (mode.wave(), mode.score());
        mode.tick(&idle(), DT);

        assert_eq!(right_after, (1, 0));
        assert_eq!((mode.wave(), mode.score()), (1, 0));
        assert!(!mode.sim().player_body().is_dead());
        assert_eq!(mode.sim().enemy_count(), 1);
        assert_eq!(mode.sim().enemies_alive(), 1);
        assert_enemy_at(&mode, 0, markers[0]);
    }

    #[test]
    fn restart_while_alive_keeps_wave_and_score() {
        let mut mode = calm_mode(4, tuning(1, 1, 100));
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        press_the_button(&mut mode);
        assert_eq!((mode.wave(), mode.score()), (2, 100));
        let enemies = mode.sim().enemy_count();

        mode.tick(&restart(), DT);

        assert_eq!((mode.wave(), mode.score()), (2, 100));
        assert_eq!(mode.sim().enemy_count(), enemies);
        assert_eq!(mode.sim().enemies_alive(), 2);
    }

    /// The arena where two enemies face the player and one shot of theirs kills.
    fn firing_squad() -> (Arc<Arena>, SimConfig) {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &[Vec3::new(-3.0, 0.0, 10.0), Vec3::new(3.0, 0.0, 10.0)],
            &[Vec3::new(0.0, 0.0, 6.0)],
        );
        let mut config = calm_config();
        config.enemy.sight_range = 30.0;
        config.enemy.damage = 100;
        (arena, config)
    }

    /// The tick on which an unattended player dies in `arena`.
    fn tick_of_death(arena: &Arc<Arena>, config: &SimConfig) -> usize {
        let mut sim = SearchlightSim::start(arena.clone(), config.clone());
        (0..300)
            .find(|_| {
                sim.tick(&idle(), DT);
                sim.player_body().is_dead()
            })
            .expect("the enemies kill the player")
    }

    #[test]
    fn no_enemies_spawn_while_the_player_is_dead() {
        let (arena, config) = firing_squad();
        let death = tick_of_death(&arena, &config);
        let mut mode = ArenaMode::new(arena, config, tuning(3, 1, 100));
        assert_eq!(mode.sim().enemy_count(), 2);
        for _ in 0..death {
            mode.tick(&idle(), DT);
        }
        assert!(!mode.sim().player_body().is_dead());

        // The marker one enemy leaves by dying frees up on the tick the other kills the player.
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        let on_death = (mode.score(), mode.wave(), mode.sim().enemy_count());
        for _ in 0..5 {
            mode.tick(&idle(), DT);
        }
        mode.tick(&restart(), DT);
        let after_restart = (mode.score(), mode.wave(), mode.sim().enemy_count());
        mode.tick(&idle(), DT);

        assert_eq!(on_death, (100, 1, 2));
        assert_eq!(after_restart, (0, 1, 2));
        assert_eq!((mode.score(), mode.wave()), (0, 1));
        assert!(!mode.sim().player_body().is_dead());
    }

    #[test]
    fn a_wave_larger_than_the_markers_spawns_as_they_free_up() {
        let markers = markers(2);
        let mut mode = calm_mode(2, tuning(3, 1, 100));
        for _ in 0..3 {
            mode.tick(&idle(), DT);
        }
        let waiting = mode.sim().enemy_count();

        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        let freed = (
            mode.sim().enemy_count(),
            mode.sim().enemies_alive(),
            mode.wave(),
            mode.score(),
        );
        mode.sim_mut().kill_enemy(1);
        mode.sim_mut().kill_enemy(2);
        mode.tick(&idle(), DT);
        let cleared = (mode.wave(), mode.sim().enemy_count());
        press_the_button(&mut mode);

        assert_eq!(waiting, 2);
        assert_eq!(freed, (3, 2, 1, 100));
        assert_eq!(cleared, (1, 3));
        assert_enemy_at(&mode, 3, markers[0]);
        assert_enemy_at(&mode, 4, markers[1]);
        assert_eq!(mode.wave(), 2);
        assert_eq!(mode.score(), 300);
        assert_eq!(mode.sim().enemies_alive(), 2);
        assert_eq!(mode.sim().enemy_count(), 5);
    }

    #[test]
    fn a_marker_that_turns_out_blocked_does_not_shorten_the_wave() {
        let close = [Vec3::new(0.0, 0.0, 12.0), Vec3::new(0.4, 0.0, 12.0)];
        let arena = arena_with(&[], Vec3::ZERO, &close, &[Vec3::new(0.0, 0.0, 25.0)]);
        let mut mode = ArenaMode::new(arena, calm_config(), tuning(2, 2, 100));
        let standing = (mode.sim().enemy_count(), mode.sim().enemies_alive());
        let wave_at_start = mode.wave();
        mode.sim_mut().kill_enemy(0);

        mode.tick(&idle(), DT);

        assert_eq!(standing, (1, 1));
        assert_eq!(wave_at_start, 1);
        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemy_count(), 2);
        assert_eq!(mode.sim().enemies_alive(), 1);
    }

    fn forcing(grade: Option<Grade>) -> SearchlightCommand {
        SearchlightCommand {
            forced_grade: grade,
            ..SearchlightCommand::default()
        }
    }

    fn grades_from(mode: &ArenaMode, from: usize) -> Vec<Option<Grade>> {
        (from..mode.sim().enemy_count())
            .map(|index| mode.sim().enemy_grade(index))
            .collect()
    }

    /// Kills every enemy standing and ticks with `command`, then presses the button with
    /// `command` too, so the next wave comes under it.
    fn clear_wave_with(mode: &mut ArenaMode, command: &SearchlightCommand) {
        for index in 0..mode.sim().enemy_count() {
            if mode.sim().enemy_state(index).is_some() {
                mode.sim_mut().kill_enemy(index);
            }
        }
        mode.tick(command, DT);
        press_the_button_with(mode, command);
    }

    fn clear_wave(mode: &mut ArenaMode) {
        clear_wave_with(mode, &idle());
    }

    const WAVE_ONE_OF_TEN: [Grade; 10] = [
        Grade::Rookie,
        Grade::Rookie,
        Grade::Rookie,
        Grade::Rookie,
        Grade::Rookie,
        Grade::Novice,
        Grade::Novice,
        Grade::Novice,
        Grade::Novice,
        Grade::Experienced,
    ];

    #[test]
    fn an_unforced_first_wave_spawns_mostly_rookies_and_novices() {
        let mode = calm_mode(12, mixed_tuning(10, 2, 100));

        let grades = grades_from(&mode, 0);

        assert_eq!(grades, WAVE_ONE_OF_TEN.map(Some));
    }

    #[test]
    fn wave_five_spawns_a_larger_high_share_than_wave_one() {
        let mut mode = calm_mode(12, mixed_tuning(2, 2, 100));
        let wave_one = grades_from(&mode, 0);
        for _ in 1..5 {
            clear_wave(&mut mode);
        }

        let wave_five = grades_from(&mode, 20);

        assert_eq!(mode.wave(), 5);
        assert_eq!(mode.sim().enemy_count(), 30);
        assert_eq!(wave_one, [Some(Grade::Rookie), Some(Grade::Novice)]);
        assert_eq!(
            wave_five,
            [
                Grade::Rookie,
                Grade::Novice,
                Grade::Novice,
                Grade::Experienced,
                Grade::Experienced,
                Grade::Experienced,
                Grade::Experienced,
                Grade::Veteran,
                Grade::Veteran,
                Grade::Professional,
            ]
            .map(Some)
        );
    }

    #[test]
    fn a_forced_grade_sets_every_new_spawn() {
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        mode.sim_mut().kill_enemy(0);

        mode.tick(&forcing(Some(Grade::Rookie)), DT);
        press_the_button_with(&mut mode, &forcing(Some(Grade::Rookie)));

        assert_eq!(mode.sim().enemy_count(), 4);
        assert_eq!(grades_from(&mode, 0)[0], Some(Grade::Experienced));
        assert_eq!(grades_from(&mode, 1), [Some(Grade::Rookie); 3]);
    }

    #[test]
    fn a_forced_grade_holds_for_the_wave_a_restart_brings() {
        let mut mode = calm_mode(4, tuning(2, 2, 100));
        mode.tick(&forcing(Some(Grade::Rookie)), DT);
        mode.sim_mut().kill_player();
        mode.tick(&forcing(Some(Grade::Rookie)), DT);

        mode.tick(
            &SearchlightCommand {
                restart: true,
                forced_grade: Some(Grade::Rookie),
                ..SearchlightCommand::default()
            },
            DT,
        );

        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemy_count(), 2);
        assert_eq!(grades_from(&mode, 0), [Some(Grade::Rookie); 2]);
    }

    #[test]
    fn a_forced_professional_fills_the_wave_a_restart_brings() {
        let mut mode = calm_mode(4, mixed_tuning(2, 2, 100));
        let before = grades_from(&mode, 0);
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);

        mode.tick(
            &SearchlightCommand {
                restart: true,
                forced_grade: Some(Grade::Professional),
                ..SearchlightCommand::default()
            },
            DT,
        );

        assert_eq!(before, [Some(Grade::Rookie), Some(Grade::Novice)]);
        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemy_count(), 2);
        assert_eq!(grades_from(&mode, 0), [Some(Grade::Professional); 2]);
    }

    #[test]
    fn clearing_the_force_spawns_the_mix_again() {
        let mut mode = calm_mode(2, mixed_tuning(2, 2, 100));
        clear_wave_with(&mut mode, &forcing(Some(Grade::Veteran)));
        let forced = grades_from(&mode, 2);
        for index in 2..4 {
            mode.sim_mut().kill_enemy(index);
        }

        mode.tick(&forcing(None), DT);

        assert_eq!(mode.wave(), 2);
        assert_eq!(mode.sim().enemy_count(), 6);
        assert_eq!(forced, [Some(Grade::Veteran); 2]);
        assert_eq!(
            grades_from(&mode, 4),
            [Some(Grade::Novice), Some(Grade::Experienced)],
            "the third and fourth of a wave of rookie, rookie, novice, experienced"
        );
    }

    #[test]
    fn a_wave_held_back_by_busy_markers_keeps_its_grades_in_order() {
        let mut mode = calm_mode(1, mixed_tuning(2, 2, 100));

        for _ in 0..5 {
            let newest = mode.sim().enemy_count() - 1;
            mode.sim_mut().kill_enemy(newest);
            mode.tick(&idle(), DT);
            press_the_button(&mut mode);
        }

        assert_eq!(mode.wave(), 2);
        assert_eq!(
            grades_from(&mode, 0),
            [
                Grade::Rookie,
                Grade::Novice,
                Grade::Rookie,
                Grade::Rookie,
                Grade::Novice,
                Grade::Experienced,
            ]
            .map(Some)
        );
    }

    #[test]
    fn a_restart_begins_the_mix_again_from_wave_one() {
        let mut mode = calm_mode(4, mixed_tuning(2, 2, 100));
        clear_wave(&mut mode);
        assert_eq!(mode.wave(), 2);
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);

        mode.tick(&restart(), DT);

        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemy_count(), 2);
        assert_eq!(
            grades_from(&mode, 0),
            [Some(Grade::Rookie), Some(Grade::Novice)]
        );
    }

    #[test]
    fn a_forced_wave_does_not_feed_the_next_waves_floor() {
        let mut mode = calm_mode(8, mixed_tuning(2, 2, 100));
        clear_wave_with(&mut mode, &forcing(Some(Grade::Professional)));
        let forced = grades_from(&mode, 2);

        clear_wave_with(&mut mode, &forcing(None));

        assert_eq!(forced, [Some(Grade::Professional); 4]);
        assert_eq!(mode.wave(), 3);
        assert_eq!(mode.sim().enemy_count(), 12);
        assert_eq!(
            grades_from(&mode, 6),
            [
                Grade::Rookie,
                Grade::Rookie,
                Grade::Novice,
                Grade::Novice,
                Grade::Experienced,
                Grade::Experienced,
            ]
            .map(Some)
        );
    }

    #[test]
    fn an_enemy_whose_marker_turns_out_blocked_keeps_its_grade() {
        let close = [Vec3::new(0.0, 0.0, 12.0), Vec3::new(0.4, 0.0, 12.0)];
        let arena = arena_with(&[], Vec3::ZERO, &close, &[Vec3::new(0.0, 0.0, 25.0)]);
        let mut mode = ArenaMode::new(arena, calm_config(), mixed_tuning(2, 2, 100));
        mode.sim_mut().kill_enemy(0);

        mode.tick(&idle(), DT);

        assert_eq!(mode.wave(), 1);
        assert_eq!(
            grades_from(&mode, 0),
            [Some(Grade::Rookie), Some(Grade::Novice)]
        );
    }

    #[test]
    fn the_feedback_of_the_sim_is_drained_through_the_mode() {
        let mut mode = calm_mode(1, tuning(1, 1, 100));
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);

        let feedback = mode.drain_feedback();

        assert!(
            feedback.contains(&Feedback::EnemyDied { target: 0 }),
            "{feedback:?}"
        );
        assert!(mode.drain_feedback().is_empty());
    }

    /// An arena whose enemy markers would fill a first wave of three.
    fn arena_with_enemy_markers() -> Arc<Arena> {
        arena_with(&[], Vec3::ZERO, &markers(3), &[Vec3::new(0.0, 0.0, 25.0)])
    }

    #[test]
    fn a_sandbox_spawns_no_enemy_in_sixty_seconds() {
        let arena = arena_with_enemy_markers();
        let waves_mode = ArenaMode::new(arena.clone(), calm_config(), tuning(3, 1, 100));
        let mut mode = ArenaMode::sandbox(arena, calm_config());

        for _ in 0..3600 {
            mode.tick(&idle(), DT);
        }

        assert_eq!(
            waves_mode.sim().enemy_count(),
            3,
            "the fixture must be one where waves would spawn"
        );
        assert_eq!(mode.sim().enemy_count(), 0);
        assert_eq!(mode.sim().enemies_alive(), 0);
    }

    #[test]
    fn a_sandbox_has_no_tally() {
        let mut mode = ArenaMode::sandbox(arena_with_enemy_markers(), calm_config());
        let at_start = mode.tally();

        mode.tick(&idle(), DT);

        assert_eq!(at_start, None);
        assert_eq!(mode.tally(), None);
    }

    #[test]
    fn a_sandbox_level_builds_a_mode_with_no_tally() {
        let mode = ArenaMode::for_level(
            arena_with_enemy_markers(),
            LevelMode::Sandbox,
            calm_config(),
            tuning(3, 1, 100),
        );

        assert_eq!(mode.tally(), None);
        assert_eq!(mode.sim().enemy_count(), 0);
    }

    #[test]
    fn an_arena_level_builds_a_mode_on_wave_one() {
        let mode = ArenaMode::for_level(
            arena_with_enemy_markers(),
            LevelMode::Arena,
            calm_config(),
            tuning(3, 1, 100),
        );

        assert_eq!(mode.tally(), Some(Tally { wave: 1, score: 0 }));
        assert_eq!(mode.sim().enemy_count(), 3);
    }

    #[test]
    fn a_dead_player_restarts_in_the_sandbox() {
        let mut mode = ArenaMode::sandbox(arena_with_enemy_markers(), calm_config());
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);
        let dead = mode.sim().player_body().is_dead();

        mode.tick(&restart(), DT);
        mode.tick(&idle(), DT);

        assert!(dead);
        assert!(!mode.sim().player_body().is_dead());
        assert_eq!(mode.tally(), None);
        assert_eq!(mode.sim().enemy_count(), 0);
    }

    #[test]
    fn a_cleared_wave_waits_thirty_seconds_without_a_use() {
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        kill_everyone(&mut mode);
        let cleared = (mode.wave(), mode.score(), mode.sim().enemy_count());

        for _ in 0..1800 {
            mode.tick(&idle(), DT);
        }

        assert_eq!(
            mode.sim().control_in_reach(),
            Some(0),
            "the button is in reach the whole time"
        );
        assert_eq!(cleared, (1, 100, 1));
        assert_eq!(mode.wave(), 1);
        assert_eq!(mode.sim().enemy_count(), 1);
        assert_eq!(mode.sim().enemies_alive(), 0);
    }

    #[test]
    fn using_the_button_with_enemies_left_changes_nothing() {
        let mut mode = calm_mode(4, tuning(2, 2, 100));
        mode.sim_mut().kill_enemy(0);
        mode.tick(&idle(), DT);
        let before = (mode.wave(), mode.score(), mode.sim().enemy_count());

        press_the_button(&mut mode);
        press_the_button(&mut mode);

        assert_eq!(before, (1, 100, 2));
        assert_eq!(
            (mode.wave(), mode.score(), mode.sim().enemy_count()),
            before
        );
        assert_eq!(mode.sim().enemies_alive(), 1);
    }

    #[test]
    fn a_use_through_a_wall_calls_no_wave() {
        let wall = (Vec3::new(-2.0, 0.0, 0.8), Vec3::new(2.0, 3.0, 1.0));
        let mut walled = calm_mode_with_button(BUTTON, &[wall], 4, tuning(1, 2, 100));
        let mut open = calm_mode(4, tuning(1, 2, 100));
        kill_everyone(&mut walled);
        kill_everyone(&mut open);

        press_the_button(&mut walled);
        press_the_button(&mut open);

        assert_eq!(open.wave(), 2, "the button works with no wall in the way");
        assert_eq!(walled.wave(), 1);
        assert_eq!(walled.sim().enemy_count(), 1);
    }

    #[test]
    fn a_use_looking_away_from_the_button_calls_no_wave() {
        let mut away = calm_mode(4, tuning(1, 2, 100));
        let mut toward = calm_mode(4, tuning(1, 2, 100));
        kill_everyone(&mut away);
        kill_everyone(&mut toward);

        away.tick(&turning_away(), DT);
        press_the_button(&mut away);
        press_the_button(&mut toward);

        assert_eq!(toward.wave(), 2);
        assert_eq!(away.wave(), 1);
        assert_eq!(away.sim().enemy_count(), 1);
    }

    #[test]
    fn a_use_out_of_reach_of_the_button_calls_no_wave() {
        let far = Vec3::new(0.0, 0.0, 4.0);
        let mut distant = calm_mode_with_button(far, &[], 4, tuning(1, 2, 100));
        kill_everyone(&mut distant);

        press_the_button(&mut distant);

        assert_eq!(distant.wave(), 1);
        assert_eq!(distant.sim().enemy_count(), 1);
    }

    #[test]
    fn a_use_on_the_tick_of_the_last_kill_calls_the_next_wave() {
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        mode.sim_mut().kill_enemy(0);

        press_the_button(&mut mode);

        assert_eq!((mode.wave(), mode.score()), (2, 100));
        assert_eq!(mode.sim().enemies_alive(), 3);
    }

    #[test]
    fn pressing_the_button_twice_calls_only_one_wave() {
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        kill_everyone(&mut mode);

        press_the_button(&mut mode);
        press_the_button(&mut mode);

        assert_eq!(mode.wave(), 2);
        assert_eq!(mode.sim().enemy_count(), 4);
    }

    #[test]
    fn a_use_while_dead_does_nothing() {
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        kill_everyone(&mut mode);
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);

        press_the_button(&mut mode);
        let dead = (mode.wave(), mode.sim().enemy_count());
        mode.tick(&restart(), DT);
        mode.tick(&idle(), DT);

        assert_eq!(dead, (1, 1));
        assert_eq!((mode.wave(), mode.score()), (1, 0));
        assert_eq!(mode.sim().enemy_count(), 1);
        assert_eq!(mode.sim().enemies_alive(), 1);
    }

    #[test]
    fn a_restart_while_a_wave_waits_brings_wave_one() {
        let markers = markers(4);
        let mut mode = calm_mode(4, tuning(1, 1, 100));
        clear_wave(&mut mode);
        kill_everyone(&mut mode);
        assert_eq!((mode.wave(), mode.score()), (2, 300));
        mode.sim_mut().kill_player();
        mode.tick(&idle(), DT);

        mode.tick(&restart(), DT);
        let right_after = (mode.wave(), mode.score());
        for _ in 0..600 {
            mode.tick(&idle(), DT);
        }

        assert_eq!(right_after, (1, 0));
        assert_eq!((mode.wave(), mode.score()), (1, 0));
        assert!(!mode.sim().player_body().is_dead());
        assert_eq!(mode.sim().enemy_count(), 1);
        assert_eq!(mode.sim().enemies_alive(), 1);
        assert_enemy_at(&mode, 0, markers[0]);
    }

    #[test]
    fn using_the_button_in_a_sandbox_does_nothing() {
        let arena = arena_with_controls(
            &[],
            Vec3::ZERO,
            &markers(3),
            &[Vec3::new(0.0, 0.0, 25.0)],
            &[BUTTON],
        );
        let mut waves = ArenaMode::new(arena.clone(), calm_config(), tuning(1, 1, 100));
        kill_everyone(&mut waves);
        press_the_button(&mut waves);
        let mut sandbox = ArenaMode::sandbox(arena, calm_config());

        for _ in 0..60 {
            press_the_button(&mut sandbox);
        }

        assert_eq!(waves.wave(), 2, "the same arena calls a wave in arena mode");
        assert_eq!(sandbox.tally(), None);
        assert_eq!(sandbox.sim().enemy_count(), 0);
        assert_eq!(sandbox.sim().control_in_reach(), Some(0));
    }
}
