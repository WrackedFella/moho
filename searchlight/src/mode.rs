//! The arena as a game mode: waves of enemies that spawn at the enemy markers, and a score for
//! the kills. The sim underneath knows neither.

use std::sync::Arc;

use crate::arena::Arena;
use crate::input::FpsCommand;
use crate::sim::{Feedback, FpsSim, SimConfig, Start};
use crate::wave::{WaveState, WaveTuning};

/// A sim that starts empty and is fed waves of enemies as the player clears them.
#[derive(Debug)]
pub struct ArenaMode {
    sim: FpsSim,
    waves: WaveState,
    tuning: WaveTuning,
    /// Kills of the sim already scored.
    seen_kills: u32,
}

impl ArenaMode {
    /// A mode over `arena` whose first wave is already standing, as far as the enemy markers
    /// allow, before the first tick.
    pub fn new(arena: Arc<Arena>, config: SimConfig, waves: WaveTuning) -> Self {
        let mut mode = Self {
            sim: FpsSim::with_start(arena, config, Start::Empty),
            waves: WaveState::begin(&waves),
            tuning: waves,
            seen_kills: 0,
        };
        mode.step_waves();
        mode
    }

    /// Scores the sim's new kills and spawns the wave's enemies at the first free markers; none
    /// while the player is dead.
    fn step_waves(&mut self) {
        let kills = self.sim.kills();
        let scan = !self.sim.player_body().is_dead()
            && (self.waves.to_spawn() != 0 || self.sim.enemies_alive() == 0);
        let free = if scan {
            self.sim.free_enemy_spawns()
        } else {
            Vec::new()
        };
        let spawn = self.waves.step(
            &self.tuning,
            kills - self.seen_kills,
            self.sim.enemies_alive(),
            free.len(),
        );
        self.seen_kills = kills;
        for &marker in free.iter().take(spawn as usize) {
            if let Err(error) = self.sim.spawn_enemy(marker) {
                tracing::warn!(%error, marker, "wave enemy not spawned");
                break;
            }
        }
    }

    /// Runs one tick of `dt` seconds: the sim, then the score of its kills and the wave's
    /// spawns. A restart of a dead player begins wave 1 again with no score.
    pub fn tick(&mut self, command: &FpsCommand, dt: f32) {
        let restarting = command.restart && self.sim.player_body().is_dead();
        self.sim.tick(command, dt);
        if restarting {
            self.waves = WaveState::begin(&self.tuning);
            self.seen_kills = 0;
        }
        self.step_waves();
    }

    pub fn sim(&self) -> &FpsSim {
        &self.sim
    }

    /// Takes the sim's pending feedback, oldest first.
    pub fn drain_feedback(&mut self) -> Vec<Feedback> {
        self.sim.drain_feedback()
    }

    /// The running wave, counting from 1.
    pub fn wave(&self) -> u32 {
        self.waves.wave()
    }

    /// Points scored since the last restart.
    pub fn score(&self) -> u32 {
        self.waves.score()
    }

    #[cfg(test)]
    pub(crate) fn sim_mut(&mut self) -> &mut FpsSim {
        &mut self.sim
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use searchlight_rules::Rifle;

    use super::*;
    use crate::sim::STAND_HEIGHT;
    use crate::test_support::{DT, arena_with, mag, test_config};

    fn idle() -> FpsCommand {
        FpsCommand::default()
    }

    fn restart() -> FpsCommand {
        FpsCommand {
            restart: true,
            ..FpsCommand::default()
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
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        config
    }

    fn tuning(first_size: u32, growth: u32, score_per_kill: u32) -> WaveTuning {
        WaveTuning::new(first_size, growth, score_per_kill).expect("a valid tuning")
    }

    fn calm_mode(marker_count: usize, waves: WaveTuning) -> ArenaMode {
        let arena = arena_with(
            &[],
            Vec3::ZERO,
            &markers(marker_count),
            &[Vec3::new(0.0, 0.0, 25.0)],
        );
        ArenaMode::new(arena, calm_config(), waves)
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
    fn killing_the_last_enemy_starts_the_next_wave() {
        let markers = markers(4);
        let mut mode = calm_mode(4, tuning(1, 2, 100));
        mode.sim_mut().kill_enemy(0);

        mode.tick(&idle(), DT);

        assert_eq!(mode.wave(), 2);
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
        mode.sim_mut().kill_enemy(1);
        mode.sim_mut().kill_enemy(2);
        mode.tick(&idle(), DT);
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
        let mut sim = FpsSim::start(arena.clone(), config.clone());
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

        assert_eq!(waiting, 2);
        assert_eq!(freed, (3, 2, 1, 100));
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
}
