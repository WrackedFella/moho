//! Waves of enemies and the score for killing them, as plain numbers: no physics, no markers.

/// Why a [`WaveTuning`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WaveTuningError {
    /// An empty first wave would count as cleared before it began, forever.
    #[error("the first wave must have at least one enemy")]
    ZeroFirstSize,
    /// With no growth every wave would be as large as the first.
    #[error("each wave must grow by at least one enemy")]
    ZeroGrowth,
}

/// How big the waves are and what a kill is worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveTuning {
    first_size: u32,
    growth: u32,
    score_per_kill: u32,
}

impl WaveTuning {
    /// Wave 1 holds `first_size` enemies and each later wave `growth` more than the one
    /// before; a kill scores `score_per_kill`.
    ///
    /// # Errors
    /// If `first_size` or `growth` is zero.
    pub fn new(first_size: u32, growth: u32, score_per_kill: u32) -> Result<Self, WaveTuningError> {
        let _ = (first_size, growth, score_per_kill);
        todo!()
    }

    pub fn first_size(&self) -> u32 {
        todo!()
    }

    pub fn growth(&self) -> u32 {
        todo!()
    }

    pub fn score_per_kill(&self) -> u32 {
        todo!()
    }

    /// Enemies in `wave`, counting the first wave as 1; saturates at `u32::MAX`.
    pub fn size(&self, wave: u32) -> u32 {
        let _ = wave;
        todo!()
    }
}

impl Default for WaveTuning {
    fn default() -> Self {
        todo!()
    }
}

/// Which wave is running, how many of its enemies are still to appear, and the score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveState {
    wave: u32,
    to_spawn: u32,
    score: u32,
}

impl WaveState {
    /// Wave 1 with nothing spawned yet and no score.
    pub fn begin(tuning: &WaveTuning) -> Self {
        let _ = tuning;
        todo!()
    }

    pub fn wave(&self) -> u32 {
        todo!()
    }

    pub fn score(&self) -> u32 {
        todo!()
    }

    pub fn to_spawn(&self) -> u32 {
        todo!()
    }

    /// Scores `kills`, starts the next wave if this one is cleared (nothing left to spawn and
    /// none alive), and returns how many enemies to spawn now: as many as `free_spawns`
    /// allows of those still to come.
    pub fn step(
        &mut self,
        tuning: &WaveTuning,
        kills: u32,
        alive: usize,
        free_spawns: usize,
    ) -> u32 {
        let _ = (tuning, kills, alive, free_spawns);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tuning(first_size: u32, growth: u32, score_per_kill: u32) -> WaveTuning {
        WaveTuning::new(first_size, growth, score_per_kill).expect("a valid tuning")
    }

    #[test]
    fn the_first_wave_has_its_configured_size() {
        let tuning = tuning(3, 2, 100);

        let state = WaveState::begin(&tuning);

        assert_eq!(state.wave(), 1);
        assert_eq!(state.to_spawn(), 3);
        assert_eq!(state.score(), 0);
        assert_eq!(tuning.size(1), 3);
        assert_eq!(tuning.size(2), 5);
        assert_eq!(tuning.size(3), 7);
    }

    #[test]
    fn the_default_tuning_starts_with_two_enemies_and_grows_by_two() {
        let tuning = WaveTuning::default();

        assert_eq!(
            (
                tuning.first_size(),
                tuning.growth(),
                tuning.score_per_kill()
            ),
            (2, 2, 100)
        );
        assert_eq!((tuning.size(1), tuning.size(2)), (2, 4));
    }

    #[test]
    fn clearing_the_last_enemy_starts_the_next_wave() {
        let tuning = tuning(2, 3, 10);
        let mut state = WaveState::begin(&tuning);
        let first = state.step(&tuning, 0, 0, 2);

        let second = state.step(&tuning, 2, 0, 10);

        assert_eq!(first, 2);
        assert_eq!(state.wave(), 2);
        assert_eq!(state.score(), 20);
        assert_eq!(second, 5);
        assert_eq!(state.to_spawn(), 0);
    }

    #[test]
    fn a_wave_with_enemies_left_does_not_advance() {
        let tuning = tuning(2, 3, 10);
        let mut state = WaveState::begin(&tuning);
        state.step(&tuning, 0, 0, 2);

        let spawned = state.step(&tuning, 1, 1, 10);

        assert_eq!(spawned, 0);
        assert_eq!(state.wave(), 1);
        assert_eq!(state.to_spawn(), 0);
        assert_eq!(state.score(), 10);
    }

    #[test]
    fn a_wave_with_enemies_still_to_spawn_is_not_cleared() {
        let tuning = tuning(3, 2, 10);
        let mut state = WaveState::begin(&tuning);
        state.step(&tuning, 0, 0, 1);

        let spawned = state.step(&tuning, 1, 0, 0);

        assert_eq!(spawned, 0);
        assert_eq!(state.wave(), 1);
        assert_eq!(state.to_spawn(), 2);
    }

    #[test]
    fn the_wave_before_its_first_spawn_is_not_cleared() {
        let tuning = tuning(2, 2, 10);
        let mut state = WaveState::begin(&tuning);

        let spawned = state.step(&tuning, 0, 0, 0);

        assert_eq!(spawned, 0);
        assert_eq!(state.wave(), 1);
        assert_eq!(state.to_spawn(), 2);
    }

    proptest! {
        #[test]
        fn each_wave_is_larger_than_the_last(
            first_size in 1..=u32::MAX,
            growth in 1..=u32::MAX,
            wave in 1..u32::MAX,
        ) {
            let tuning = tuning(first_size, growth, 1);

            let this = tuning.size(wave);
            let next = tuning.size(wave + 1);

            let exact = u128::from(first_size) + u128::from(growth) * u128::from(wave - 1);
            let saturated = exact.min(u128::from(u32::MAX));
            prop_assert_eq!(u128::from(this), saturated);
            prop_assert!(next >= this);
            if this < u32::MAX {
                prop_assert!(next > this);
            }
        }
    }

    #[test]
    fn the_size_saturates_instead_of_overflowing() {
        let tuning = tuning(u32::MAX, u32::MAX, 1);

        assert_eq!(tuning.size(1), u32::MAX);
        assert_eq!(tuning.size(2), u32::MAX);
        assert_eq!(tuning.size(u32::MAX), u32::MAX);
    }

    #[test]
    fn growth_of_zero_is_rejected() {
        assert_eq!(WaveTuning::new(2, 0, 100), Err(WaveTuningError::ZeroGrowth));
    }

    #[test]
    fn a_first_wave_of_zero_is_rejected() {
        assert_eq!(
            WaveTuning::new(0, 2, 100),
            Err(WaveTuningError::ZeroFirstSize)
        );
    }

    #[test]
    fn a_kill_worth_nothing_is_accepted() {
        let tuning = WaveTuning::new(1, 1, 0).expect("scoring nothing is allowed");

        assert_eq!(tuning.score_per_kill(), 0);
    }

    #[test]
    fn a_kill_adds_the_configured_score() {
        let tuning = tuning(5, 1, 25);
        let mut state = WaveState::begin(&tuning);

        state.step(&tuning, 0, 1, 0);
        let none = state.score();
        state.step(&tuning, 1, 1, 0);
        let one = state.score();
        state.step(&tuning, 3, 1, 0);
        let several = state.score();

        assert_eq!((none, one, several), (0, 25, 100));
    }

    #[test]
    fn score_saturates_instead_of_overflowing() {
        let tuning = tuning(5, 1, u32::MAX);
        let mut state = WaveState::begin(&tuning);

        state.step(&tuning, 1, 1, 0);
        let one = state.score();
        state.step(&tuning, 1, 1, 0);
        let two = state.score();
        let huge = WaveTuning::new(5, 1, 2).expect("valid");
        let mut other = WaveState::begin(&huge);
        other.step(&huge, u32::MAX, 1, 0);

        assert_eq!((one, two), (u32::MAX, u32::MAX));
        assert_eq!(other.score(), u32::MAX);
    }

    #[test]
    fn enemies_spawn_only_into_free_markers() {
        let tuning = tuning(4, 1, 10);
        let mut state = WaveState::begin(&tuning);

        let none = state.step(&tuning, 0, 0, 0);
        let left_after_none = state.to_spawn();
        let fewer = state.step(&tuning, 0, 0, 1);
        let left_after_fewer = state.to_spawn();
        let exactly = state.step(&tuning, 0, 1, 3);
        let left_after_exactly = state.to_spawn();
        let nothing_left = state.step(&tuning, 0, 4, 10);

        assert_eq!((none, left_after_none), (0, 4));
        assert_eq!((fewer, left_after_fewer), (1, 3));
        assert_eq!((exactly, left_after_exactly), (3, 0));
        assert_eq!(nothing_left, 0);
    }

    #[test]
    fn more_free_markers_than_enemies_to_come_spawns_only_those_to_come() {
        let tuning = tuning(2, 1, 10);
        let mut state = WaveState::begin(&tuning);

        let spawned = state.step(&tuning, 0, 0, 5);

        assert_eq!(spawned, 2);
        assert_eq!(state.to_spawn(), 0);
    }
}
