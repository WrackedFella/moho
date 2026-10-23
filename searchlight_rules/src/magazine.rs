//! Magazine capacity, rounds and the approximate fill level.

use crate::item::{Caliber, ItemId, MagazineSpec, MagazineWell};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why a [`Magazine`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum MagazineError {
    /// More rounds were supplied than the magazine holds.
    #[error("{rounds} rounds exceed the capacity of {capacity}")]
    OverCapacity {
        /// Rounds requested.
        rounds: u32,
        /// Capacity of the magazine.
        capacity: u32,
    },
    /// A round was loaded into a magazine already at capacity.
    #[error("the magazine is full at {capacity} rounds")]
    Full {
        /// Capacity of the magazine.
        capacity: u32,
    },
}

/// Approximate fill of a magazine, ordered from emptiest to fullest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MagazineLevel {
    /// No rounds.
    Empty,
    /// Above zero, up to and including 25% of capacity.
    NearlyEmpty,
    /// Above 25%, up to and including 75% of capacity.
    AboutHalf,
    /// Above 75% of capacity but not full.
    MostlyFull,
    /// Every slot filled.
    Full,
}

/// A magazine item holding an ordered stack of rounds, up to its capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawMagazine")]
pub struct Magazine {
    item: ItemId,
    well: MagazineWell,
    caliber: Caliber,
    capacity: u32,
    /// Last loaded is last in the vector and fires first.
    rounds: Vec<ItemId>,
}

/// Unvalidated wire form of a [`Magazine`], checked on deserialize.
#[derive(Deserialize)]
struct RawMagazine {
    item: ItemId,
    well: MagazineWell,
    caliber: Caliber,
    capacity: u32,
    rounds: Vec<ItemId>,
}

impl TryFrom<RawMagazine> for Magazine {
    type Error = MagazineError;

    fn try_from(raw: RawMagazine) -> Result<Self, Self::Error> {
        let held = u32::try_from(raw.rounds.len()).unwrap_or(u32::MAX);
        if held > raw.capacity {
            return Err(MagazineError::OverCapacity {
                rounds: held,
                capacity: raw.capacity,
            });
        }
        Ok(Self {
            item: raw.item,
            well: raw.well,
            caliber: raw.caliber,
            capacity: raw.capacity,
            rounds: raw.rounds,
        })
    }
}

impl Magazine {
    /// An empty magazine of the item `item` described by `spec`.
    #[must_use]
    pub fn empty(item: ItemId, spec: &MagazineSpec) -> Self {
        Self {
            item,
            well: spec.well.clone(),
            caliber: spec.caliber.clone(),
            capacity: spec.capacity,
            rounds: Vec::new(),
        }
    }

    /// A magazine holding `count` rounds of type `round`.
    ///
    /// # Errors
    ///
    /// [`MagazineError::OverCapacity`] when `count` exceeds the capacity.
    pub fn filled(
        item: ItemId,
        spec: &MagazineSpec,
        round: &ItemId,
        count: u32,
    ) -> Result<Self, MagazineError> {
        if count > spec.capacity {
            return Err(MagazineError::OverCapacity {
                rounds: count,
                capacity: spec.capacity,
            });
        }
        let mut magazine = Self::empty(item, spec);
        magazine.rounds = vec![round.clone(); count as usize];
        Ok(magazine)
    }

    /// Loads `round` on top of the stack.
    ///
    /// # Errors
    ///
    /// [`MagazineError::Full`] when the magazine is at capacity.
    pub fn load(&mut self, round: ItemId) -> Result<(), MagazineError> {
        if self.rounds() >= self.capacity {
            return Err(MagazineError::Full {
                capacity: self.capacity,
            });
        }
        self.rounds.push(round);
        Ok(())
    }

    /// Takes the round loaded last.
    pub fn pop(&mut self) -> Option<ItemId> {
        self.rounds.pop()
    }

    /// The item this magazine is an instance of.
    #[must_use]
    pub fn item(&self) -> &ItemId {
        &self.item
    }

    /// The well it fits.
    #[must_use]
    pub fn well(&self) -> &MagazineWell {
        &self.well
    }

    /// The caliber it takes.
    #[must_use]
    pub fn caliber(&self) -> &Caliber {
        &self.caliber
    }

    /// Maximum rounds the magazine holds.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Rounds currently held.
    #[must_use]
    pub fn rounds(&self) -> u32 {
        // Held rounds never exceed the u32 capacity, so the length fits.
        u32::try_from(self.rounds.len()).unwrap_or(u32::MAX)
    }

    /// Approximate fill, for HUD display without exact counts.
    #[must_use]
    pub fn level(&self) -> MagazineLevel {
        let rounds = self.rounds();
        // Widened so the comparisons against quarters of capacity cannot overflow.
        let scaled = u64::from(rounds) * 4;
        let capacity = u64::from(self.capacity);
        if rounds == 0 {
            MagazineLevel::Empty
        } else if rounds == self.capacity {
            MagazineLevel::Full
        } else if scaled <= capacity {
            MagazineLevel::NearlyEmpty
        } else if scaled <= capacity * 3 {
            MagazineLevel::AboutHalf
        } else {
            MagazineLevel::MostlyFull
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Caliber, ItemId, MagazineWell};
    use crate::test_support::{id, mag, mag_of, spec};
    use proptest::prelude::*;

    fn level_of(capacity: u32, rounds: u32) -> MagazineLevel {
        mag(capacity, rounds).level()
    }

    #[test]
    fn level_names_the_approximate_fill() {
        use MagazineLevel::{AboutHalf, Empty, Full, MostlyFull, NearlyEmpty};
        let table = [
            (100, 0, Empty),
            (100, 1, NearlyEmpty),
            (100, 24, NearlyEmpty),
            (100, 25, NearlyEmpty),
            (100, 26, AboutHalf),
            (100, 50, AboutHalf),
            (100, 75, AboutHalf),
            (100, 76, MostlyFull),
            (100, 99, MostlyFull),
            (100, 100, Full),
            (8, 2, NearlyEmpty),
            (8, 3, AboutHalf),
            (8, 6, AboutHalf),
            (8, 7, MostlyFull),
            (8, 8, Full),
            (30, 7, NearlyEmpty),
            (30, 8, AboutHalf),
            (30, 22, AboutHalf),
            (30, 23, MostlyFull),
            (101, 26, AboutHalf),
            (101, 76, MostlyFull),
            (1, 0, Empty),
            (1, 1, Full),
        ];

        for (capacity, rounds, expected) in table {
            assert_eq!(
                level_of(capacity, rounds),
                expected,
                "capacity {capacity}, rounds {rounds}"
            );
        }
    }

    #[test]
    fn filled_rejects_rounds_over_capacity() {
        let over = Magazine::filled(id("mag"), &spec("rifle", 30), &id("ball"), 31);

        assert_eq!(
            over,
            Err(MagazineError::OverCapacity {
                rounds: 31,
                capacity: 30
            })
        );
    }

    #[test]
    fn filled_accepts_a_full_magazine() {
        let full = Magazine::filled(id("mag"), &spec("rifle", 30), &id("ball"), 30)
            .expect("a full magazine fits its capacity");

        assert_eq!(full.capacity(), 30);
        assert_eq!(full.rounds(), 30);
    }

    #[test]
    fn empty_holds_no_rounds_and_keeps_the_spec() {
        let spec = spec("rifle", 30);

        let magazine = Magazine::empty(id("mag_rifle"), &spec);

        assert_eq!(magazine.rounds(), 0);
        assert_eq!(magazine.capacity(), 30);
        assert_eq!(magazine.level(), MagazineLevel::Empty);
        assert_eq!(magazine.item(), &ItemId::new("mag_rifle"));
        assert_eq!(magazine.well(), &MagazineWell::new("rifle"));
        assert_eq!(magazine.caliber(), &Caliber::new("5.56x45"));
    }

    #[test]
    fn filled_keeps_identity_and_counts_the_rounds() {
        let magazine = mag_of("mag_a", "rifle", 30, 7);

        assert_eq!(magazine.item(), &ItemId::new("mag_a"));
        assert_eq!(magazine.capacity(), 30);
        assert_eq!(magazine.rounds(), 7);
    }

    #[test]
    fn zero_capacity_magazine_is_empty_without_dividing_by_zero() {
        let magazine = mag(0, 0);

        assert_eq!(magazine.level(), MagazineLevel::Empty);
        assert_eq!(magazine.capacity(), 0);
    }

    #[test]
    fn load_refuses_a_round_when_the_magazine_is_full() {
        for (capacity, rounds) in [(2, 2), (0, 0)] {
            let mut magazine = mag(capacity, rounds);

            let result = magazine.load(id("ball"));

            assert_eq!(result, Err(MagazineError::Full { capacity }));
            assert_eq!(magazine.rounds(), rounds, "capacity {capacity}");
        }
    }

    #[test]
    fn load_adds_one_round_up_to_capacity() {
        let mut magazine = mag(2, 0);

        assert_eq!(magazine.load(id("ball")), Ok(()));
        assert_eq!(magazine.rounds(), 1);
        assert_eq!(magazine.load(id("ball")), Ok(()));
        assert_eq!(magazine.rounds(), 2);
        assert_eq!(magazine.level(), MagazineLevel::Full);
    }

    #[test]
    fn pop_on_an_empty_magazine_returns_nothing() {
        let mut magazine = mag(30, 0);

        let round = magazine.pop();

        assert_eq!(round, None);
        assert_eq!(magazine.rounds(), 0);
    }

    #[test]
    fn rounds_fire_last_loaded_first() {
        let mut magazine = Magazine::empty(id("mag"), &spec("rifle", 3));
        for round in ["a", "b", "c"] {
            magazine.load(id(round)).expect("room for three rounds");
        }

        let popped: Vec<Option<ItemId>> = (0..4).map(|_| magazine.pop()).collect();

        assert_eq!(popped, [Some(id("c")), Some(id("b")), Some(id("a")), None]);
        assert_eq!(magazine.rounds(), 0);
    }

    #[test]
    fn filled_stacks_the_given_round_type() {
        let mut magazine = mag_of("mag", "rifle", 5, 2);

        assert_eq!(magazine.pop(), Some(id("ball")));
        assert_eq!(magazine.pop(), Some(id("ball")));
        assert_eq!(magazine.pop(), None);
    }

    #[test]
    fn deserialize_rejects_rounds_over_capacity() {
        let json = r#"{"item":"mag","well":"rifle","caliber":"5.56x45","capacity":2,"rounds":["a","b","c"]}"#;

        let result = serde_json::from_str::<Magazine>(json);

        assert!(
            result.is_err(),
            "over-capacity data deserialized: {result:?}"
        );
    }

    #[test]
    fn serialized_magazine_round_trips_equal_with_round_order() {
        let mut original = Magazine::empty(id("mag"), &spec("rifle", 30));
        for round in ["a", "b", "c"] {
            original.load(id(round)).expect("room");
        }

        let json = serde_json::to_string(&original).expect("magazine serializes");
        let mut restored: Magazine = serde_json::from_str(&json).expect("valid data deserializes");

        assert_eq!(restored, original);
        assert_eq!(restored.pop(), Some(id("c")));
    }

    proptest! {
        #[test]
        fn level_never_drops_as_rounds_rise(capacity in 1u32..500, a in 0u32..500, b in 0u32..500) {
            let (lo, hi) = (a.min(b).min(capacity), a.max(b).min(capacity));

            let low_level = level_of(capacity, lo);
            let high_level = level_of(capacity, hi);

            prop_assert!(low_level <= high_level);
            prop_assert_eq!(low_level == MagazineLevel::Empty, lo == 0);
            prop_assert_eq!(high_level == MagazineLevel::Full, hi == capacity);
        }

        #[test]
        fn rounds_always_pop_in_reverse_of_load_order(
            names in proptest::collection::vec(0u8..6, 0..40),
        ) {
            let mut magazine = Magazine::empty(id("mag"), &spec("rifle", 40));
            for name in &names {
                magazine.load(id(&format!("r{name}"))).expect("at most 40 rounds");
            }

            let mut popped = Vec::new();
            while let Some(round) = magazine.pop() {
                popped.push(round);
            }

            let expected: Vec<ItemId> = names.iter().rev().map(|n| id(&format!("r{n}"))).collect();
            prop_assert_eq!(popped, expected);
            prop_assert_eq!(magazine.rounds(), 0);
        }
    }
}
