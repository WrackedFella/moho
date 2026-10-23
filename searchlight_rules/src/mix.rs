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

/// Where the experienced-and-above grades start on the ladder.
const FIRST_HIGH: usize = 2;

impl WaveGrades {
    pub fn size(&self) -> u32 {
        self.counts.iter().sum()
    }

    pub fn count(&self, grade: Grade) -> u32 {
        self.counts[grade as usize]
    }

    /// Enemies of experienced grade or better.
    pub fn experienced_and_above(&self) -> u32 {
        self.counts[FIRST_HIGH..].iter().sum()
    }

    /// The grade of the wave's `index`th enemy in spawn order, lowest grade first; `None` past
    /// the end.
    pub fn grade_of(&self, index: u32) -> Option<Grade> {
        let mut before = 0u32;
        for (grade, count) in Grade::LADDER.into_iter().zip(self.counts) {
            before = before.saturating_add(count);
            if index < before {
                return Some(grade);
            }
        }
        None
    }
}

/// The experienced-and-above weight and the total weight of `step`.
fn shares(step: &[u32; 5]) -> (u128, u128) {
    let high = step[FIRST_HIGH..].iter().map(|&w| u128::from(w)).sum();
    let total = step.iter().map(|&w| u128::from(w)).sum();
    (high, total)
}

/// Splits `count` into `parts` by `weights` with largest remainder, ties to the lower index;
/// all of it on the first part when the weights sum to zero.
fn split(count: u32, weights: &[u32], parts: &mut [u32]) {
    let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if total == 0 {
        parts.fill(0);
        if let Some(first) = parts.first_mut() {
            *first = count;
        }
        return;
    }
    let mut remainders = [0u64; 5];
    for ((part, remainder), &weight) in parts.iter_mut().zip(&mut remainders).zip(weights) {
        let scaled = u64::from(count) * u64::from(weight);
        *part = u32::try_from(scaled / total).unwrap_or(count);
        *remainder = scaled % total;
    }
    let left = count - parts.iter().sum::<u32>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(remainders[index]), index));
    for &index in order.iter().take(left as usize) {
        parts[index] += 1;
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
    pub fn new(steps: Vec<[u32; 5]>) -> Result<Self, GradeMixError> {
        if steps.is_empty() {
            return Err(GradeMixError::Empty);
        }
        let mut before: Option<(u128, u128)> = None;
        for (step, wave) in steps.iter().zip(1..) {
            let (high, total) = shares(step);
            if total == 0 {
                return Err(GradeMixError::NoWeight { wave });
            }
            if before.is_some_and(|(prev_high, prev_total)| high * prev_total < prev_high * total) {
                return Err(GradeMixError::ShareFalls { wave });
            }
            before = Some((high, total));
        }
        Ok(Self { steps })
    }

    /// Every enemy of every wave is `grade`.
    pub fn only(grade: Grade) -> Self {
        Self {
            steps: vec![Grade::LADDER.map(|on_ladder| u32::from(on_ladder == grade))],
        }
    }

    /// The counts for `wave` (from 1) of `size` enemies, never with a smaller
    /// experienced-and-above share than `previous`, the wave before.
    ///
    /// The experienced-and-above count is the step's share of `size`, rounded half up, raised
    /// to carry the share of `previous` over; each of the two groups is then split among its
    /// grades by weight.
    pub fn grades(&self, wave: u32, size: u32, previous: Option<WaveGrades>) -> WaveGrades {
        let last = self.steps.len() - 1;
        let step =
            &self.steps[usize::try_from(wave.saturating_sub(1)).map_or(last, |at| at.min(last))];
        let (share, total) = shares(step);
        let wanted = (2 * u128::from(size) * share + total) / (2 * total);
        let carried = previous
            .filter(|before| before.size() != 0)
            .map_or(0, |before| {
                let scaled = u128::from(before.experienced_and_above()) * u128::from(size);
                scaled.div_ceil(u128::from(before.size()))
            });
        let high = u32::try_from(wanted.max(carried).min(u128::from(size))).unwrap_or(size);
        let mut counts = [0; 5];
        let (low_counts, high_counts) = counts.split_at_mut(FIRST_HIGH);
        split(size - high, &step[..FIRST_HIGH], low_counts);
        split(high, &step[FIRST_HIGH..], high_counts);
        WaveGrades { counts }
    }
}

impl Default for GradeMix {
    fn default() -> Self {
        Self {
            steps: vec![
                [5, 4, 1, 0, 0],
                [4, 4, 2, 0, 0],
                [3, 3, 3, 1, 0],
                [2, 3, 3, 2, 0],
                [1, 2, 4, 2, 1],
                [1, 1, 3, 3, 2],
                [0, 1, 3, 3, 3],
            ],
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
    }

    #[test]
    fn a_rising_share_of_large_weights_is_accepted_without_overflow() {
        let result = GradeMix::new(vec![
            [1_000_000_000, 1_000_000_000, 1_000_000_000, 0, 0],
            [1_000_000_000, 0, 1_000_000_000, 0, 0],
        ]);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn a_half_share_rounds_up() {
        let half = mix(vec![[1, 0, 1, 0, 0]]).grades(1, 5, None);
        let tenth = GradeMix::default().grades(1, 5, None);

        assert_eq!(counts(&half), [2, 0, 3, 0, 0]);
        assert_eq!(counts(&tenth), [2, 2, 1, 0, 0]);
    }

    #[test]
    fn a_floor_raised_group_with_no_weight_goes_to_its_lowest_grade() {
        let previous = GradeMix::only(Grade::Veteran).grades(1, 2, None);

        let grades = GradeMix::only(Grade::Rookie).grades(1, 4, Some(previous));

        assert_eq!(counts(&grades), [0, 0, 4, 0, 0]);
    }

    #[test]
    fn the_default_mix_is_one_new_accepts() {
        let default = GradeMix::default();

        assert_eq!(GradeMix::new(default.steps.clone()), Ok(default));
    }

    #[test]
    fn a_wave_of_the_largest_size_is_split_without_overflow() {
        let mix = mix(vec![[u32::MAX; 5]]);
        let previous = mix.grades(1, u32::MAX, None);

        let grades = mix.grades(2, u32::MAX, Some(previous));

        assert_eq!(grades.size(), u32::MAX);
        assert!(grades.experienced_and_above() >= previous.experienced_and_above());
        assert_eq!(grades.grade_of(u32::MAX - 1), Some(Grade::Professional));
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
        let grades = mix(vec![[1, 2, 4, 2, 1]]).grades(1, 10, None);
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
