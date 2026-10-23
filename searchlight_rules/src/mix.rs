//! The grades of each wave's enemies: a mix per wave that shifts up the ladder as waves go on.

use crate::grade::Grade;

/// Why a [`GradeMix`] was refused; waves count from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GradeMixError {
    /// A mix with no steps has no wave to describe.
    #[error("a grade mix needs at least one step")]
    Empty,
    /// A step whose weights are all zero picks no grade.
    #[error("wave {wave} gives every grade a weight of zero")]
    NoWeight { wave: u32 },
    /// A step with a smaller share of experienced and above than the step before.
    #[error("wave {wave} has a smaller experienced-and-above share than the wave before")]
    ShareFalls { wave: u32 },
}

/// How many enemies of each grade a wave holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveGrades {
    counts: [u32; 5],
}

impl WaveGrades {
    /// Enemies in the wave.
    pub fn size(&self) -> u32 {
        todo!()
    }

    /// Enemies of `grade`.
    pub fn count(&self, _grade: Grade) -> u32 {
        todo!()
    }

    /// Enemies of experienced grade or better.
    pub fn experienced_and_above(&self) -> u32 {
        todo!()
    }

    /// The grade of the wave's `index`th enemy in spawn order, lowest grade first; `None` past
    /// the end.
    pub fn grade_of(&self, _index: u32) -> Option<Grade> {
        todo!()
    }
}

/// Per-wave grade weights, one step per wave from wave 1; the last step holds past the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GradeMix {
    steps: Vec<[u32; 5]>,
}

impl GradeMix {
    /// A mix of `steps`, each five weights in [`Grade::LADDER`] order.
    ///
    /// # Errors
    /// If `steps` is empty, a step's weights are all zero, or a step's experienced-and-above
    /// share is below the step before it.
    pub fn new(_steps: Vec<[u32; 5]>) -> Result<Self, GradeMixError> {
        todo!()
    }

    /// Every enemy of every wave is `grade`.
    pub fn only(_grade: Grade) -> Self {
        Self::default()
    }

    /// The counts for `wave` (from 1) of `size` enemies, never with a smaller
    /// experienced-and-above share than `previous`, the wave before.
    pub fn grades(&self, _wave: u32, _size: u32, _previous: Option<WaveGrades>) -> WaveGrades {
        WaveGrades { counts: [0; 5] }
    }
}

impl Default for GradeMix {
    fn default() -> Self {
        Self {
            steps: vec![[0, 0, 1, 0, 0]],
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const DEFAULT_HIGH: [u32; 9] = [0, 1, 2, 4, 7, 10, 13, 15, 17];

    /// Wave sizes of the default tuning: 2, 4, 6, ...
    fn default_size(wave: u32) -> u32 {
        2 * wave
    }

    fn chain(mix: &GradeMix, waves: u32, size: impl Fn(u32) -> u32) -> Vec<WaveGrades> {
        let mut previous = None;
        let mut all = Vec::new();
        for wave in 1..=waves {
            let grades = mix.grades(wave, size(wave), previous);
            all.push(grades);
            previous = Some(grades);
        }
        all
    }

    fn counts(grades: &WaveGrades) -> [u32; 5] {
        Grade::LADDER.map(|grade| grades.count(grade))
    }

    fn mix(steps: Vec<[u32; 5]>) -> GradeMix {
        GradeMix::new(steps).expect("a valid mix")
    }

    #[test]
    fn wave_one_of_the_default_mix_is_mostly_rookies_and_novices() {
        let mix = GradeMix::default();

        for size in 1..=20 {
            let grades = mix.grades(1, size, None);

            let low = grades.count(Grade::Rookie) + grades.count(Grade::Novice);
            assert_eq!(grades.size(), size);
            assert!(2 * low > size, "size {size}: {low} low grades");
        }
    }

    #[test]
    fn wave_one_of_two_is_a_rookie_and_a_novice() {
        let grades = GradeMix::default().grades(1, 2, None);

        assert_eq!(counts(&grades), [1, 1, 0, 0, 0]);
    }

    #[test]
    fn wave_five_has_a_larger_high_share_than_wave_one() {
        let waves = chain(&GradeMix::default(), 5, default_size);

        let (one, five) = (&waves[0], &waves[4]);

        assert_eq!(counts(one), [1, 1, 0, 0, 0]);
        assert_eq!(counts(five), [1, 2, 4, 2, 1]);
        assert!(
            u64::from(five.experienced_and_above()) * u64::from(one.size())
                > u64::from(one.experienced_and_above()) * u64::from(five.size())
        );
    }

    proptest! {
        #[test]
        fn the_high_share_never_falls_from_one_wave_to_the_next(
            raw in prop::collection::vec(prop::array::uniform5(0u32..20), 1..6),
            first in 1u32..50,
            growth in 1u32..20,
        ) {
            let mut steps: Vec<[u32; 5]> = raw
                .into_iter()
                .map(|step| if step == [0; 5] { [0, 0, 1, 0, 0] } else { step })
                .collect();
            let share = |step: &[u32; 5]| (u64::from(step[2] + step[3] + step[4]), u64::from(step.iter().sum::<u32>()));
            steps.sort_by(|a, b| {
                let ((ah, at), (bh, bt)) = (share(a), share(b));
                (ah * bt).cmp(&(bh * at))
            });
            let mix = GradeMix::new(steps).expect("steps sorted by share are valid");

            let waves = chain(&mix, 40, |wave| first + growth * (wave - 1));

            for (index, grades) in waves.iter().enumerate() {
                let size = first + growth * index as u32;
                prop_assert_eq!(counts(grades).iter().sum::<u32>(), size);
                prop_assert_eq!(grades.size(), size);
                prop_assert!(grades.experienced_and_above() <= size);
            }
            for pair in waves.windows(2) {
                prop_assert!(
                    u64::from(pair[1].experienced_and_above()) * u64::from(pair[0].size())
                        >= u64::from(pair[0].experienced_and_above()) * u64::from(pair[1].size())
                );
            }
        }
    }

    #[test]
    fn the_default_mix_never_steps_down_past_its_last_step() {
        let waves = chain(&GradeMix::default(), 40, default_size);

        let high: Vec<u32> = waves[..9]
            .iter()
            .map(WaveGrades::experienced_and_above)
            .collect();

        assert_eq!(high, DEFAULT_HIGH);
        for pair in waves.windows(2) {
            assert!(
                u64::from(pair[1].experienced_and_above()) * u64::from(pair[0].size())
                    >= u64::from(pair[0].experienced_and_above()) * u64::from(pair[1].size())
            );
        }
        for (index, grades) in waves.iter().enumerate() {
            assert_eq!(grades.size(), default_size(index as u32 + 1));
        }
    }

    #[test]
    fn an_empty_mix_is_refused() {
        assert_eq!(GradeMix::new(vec![]), Err(GradeMixError::Empty));
    }

    #[test]
    fn a_step_of_zero_weights_is_refused() {
        assert_eq!(
            GradeMix::new(vec![[0; 5]]),
            Err(GradeMixError::NoWeight { wave: 1 })
        );
        assert_eq!(
            GradeMix::new(vec![[1, 0, 0, 0, 0], [1, 1, 1, 0, 0], [0; 5]]),
            Err(GradeMixError::NoWeight { wave: 3 })
        );
    }

    #[test]
    fn a_step_with_a_lower_high_share_is_refused() {
        assert_eq!(
            GradeMix::new(vec![[1, 1, 2, 0, 0], [1, 1, 1, 0, 0]]),
            Err(GradeMixError::ShareFalls { wave: 2 })
        );
        assert_eq!(
            GradeMix::new(vec![[1, 0, 1, 0, 0], [1, 0, 2, 0, 0], [3, 0, 3, 0, 1]]),
            Err(GradeMixError::ShareFalls { wave: 3 })
        );
        assert_eq!(
            GradeMix::new(vec![
                [1_000_000_000, 0, 1_000_000_000, 0, 0],
                [1_000_000_000, 1_000_000_000, 1_000_000_000, 0, 0]
            ]),
            Err(GradeMixError::ShareFalls { wave: 2 })
        );
    }

    #[test]
    fn an_equal_share_is_accepted() {
        assert!(GradeMix::new(vec![[1, 1, 1, 0, 0], [2, 2, 2, 0, 0]]).is_ok());
        assert!(GradeMix::new(vec![[1, 0, 1, 0, 0], [1, 0, 0, 1, 0]]).is_ok());
        assert!(
            GradeMix::new(vec![
                [1_000_000_000, 1_000_000_000, 1_000_000_000, 0, 0],
                [1_000_000_000, 0, 1_000_000_000, 0, 0],
            ])
            .is_ok()
        );
    }

    #[test]
    fn waves_past_the_last_step_use_the_last_step() {
        let mix = mix(vec![[3, 0, 1, 0, 0], [1, 0, 1, 0, 0]]);

        let one = mix.grades(1, 4, None);
        let two = mix.grades(2, 4, None);
        let three = mix.grades(3, 4, None);
        let far = mix.grades(1000, 4, None);

        assert_eq!(counts(&one), [3, 0, 1, 0, 0]);
        assert_eq!(counts(&two), [2, 0, 2, 0, 0]);
        assert_eq!(counts(&three), [2, 0, 2, 0, 0]);
        assert_eq!(counts(&far), [2, 0, 2, 0, 0]);
    }

    #[test]
    fn each_group_is_split_by_weight_with_ties_to_the_lower_grade() {
        let tied_low = mix(vec![[1, 1, 0, 0, 0]]).grades(1, 3, None);
        let tied_high = mix(vec![[0, 0, 1, 1, 1]]).grades(1, 4, None);
        let three_to_one = mix(vec![[3, 1, 0, 0, 0]]).grades(1, 5, None);
        let one_to_two = mix(vec![[1, 2, 0, 0, 0]]).grades(1, 4, None);
        let high_weighted = mix(vec![[0, 0, 1, 2, 3]]).grades(1, 6, None);
        let both = mix(vec![[1, 1, 1, 1, 0]]).grades(1, 6, None);

        assert_eq!(counts(&tied_low), [2, 1, 0, 0, 0]);
        assert_eq!(counts(&tied_high), [0, 0, 2, 1, 1]);
        assert_eq!(counts(&three_to_one), [4, 1, 0, 0, 0]);
        assert_eq!(counts(&one_to_two), [1, 3, 0, 0, 0]);
        assert_eq!(counts(&high_weighted), [0, 0, 1, 2, 3]);
        assert_eq!(counts(&both), [2, 1, 2, 1, 0]);
    }

    #[test]
    fn only_one_grade_fills_every_wave_with_it() {
        for grade in Grade::LADDER {
            let mix = GradeMix::only(grade);

            let waves = chain(&mix, 10, |_| 7);

            for (index, grades) in waves.iter().enumerate() {
                let mut expected = [0; 5];
                expected[Grade::LADDER.iter().position(|&g| g == grade).unwrap()] = 7;
                assert_eq!(counts(grades), expected, "{grade:?} wave {}", index + 1);
            }
        }
    }

    #[test]
    fn grade_of_lists_lowest_grades_first_and_none_past_the_end() {
        let grades = chain(&GradeMix::default(), 5, default_size)[4];
        let expected = [
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
        ];

        let listed: Vec<Option<Grade>> = (0..10).map(|index| grades.grade_of(index)).collect();

        assert_eq!(listed, expected.map(Some));
        assert_eq!(grades.grade_of(10), None);
        assert_eq!(grades.grade_of(11), None);
        assert_eq!(grades.grade_of(u32::MAX), None);
    }
}
