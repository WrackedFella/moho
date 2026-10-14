//! Magazine capacity, rounds and the approximate fill level.

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

/// A magazine holding up to `capacity` rounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawMagazine")]
pub struct Magazine {
    pub(crate) capacity: u32,
    pub(crate) rounds: u32,
}

/// Unvalidated wire form of a [`Magazine`], checked by [`Magazine::new`] on deserialize.
#[derive(Deserialize)]
struct RawMagazine {
    capacity: u32,
    rounds: u32,
}

impl TryFrom<RawMagazine> for Magazine {
    type Error = MagazineError;

    fn try_from(raw: RawMagazine) -> Result<Self, Self::Error> {
        Self::new(raw.capacity, raw.rounds)
    }
}

impl Magazine {
    /// Builds a magazine holding `rounds` of `capacity`.
    ///
    /// # Errors
    ///
    /// [`MagazineError::OverCapacity`] when `rounds` exceeds `capacity`.
    pub fn new(capacity: u32, rounds: u32) -> Result<Self, MagazineError> {
        if rounds > capacity {
            return Err(MagazineError::OverCapacity { rounds, capacity });
        }
        Ok(Self { capacity, rounds })
    }

    /// Maximum rounds the magazine holds.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Rounds currently held.
    #[must_use]
    pub fn rounds(&self) -> u32 {
        self.rounds
    }

    /// Approximate fill, for HUD display without exact counts.
    #[must_use]
    pub fn level(&self) -> MagazineLevel {
        // Widened so the comparisons against quarters of capacity cannot overflow.
        let scaled = u64::from(self.rounds) * 4;
        let capacity = u64::from(self.capacity);
        if self.rounds == 0 {
            MagazineLevel::Empty
        } else if self.rounds == self.capacity {
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
    use proptest::prelude::*;

    fn level_of(capacity: u32, rounds: u32) -> MagazineLevel {
        Magazine::new(capacity, rounds)
            .expect("test rounds fit the capacity")
            .level()
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
    fn new_rejects_rounds_over_capacity() {
        let over = Magazine::new(30, 31);
        let exact = Magazine::new(30, 30);

        assert_eq!(
            over,
            Err(MagazineError::OverCapacity {
                rounds: 31,
                capacity: 30
            })
        );
        assert!(exact.is_ok());
    }

    #[test]
    fn capacity_returns_the_constructed_capacity() {
        let magazine = Magazine::new(30, 7).expect("rounds fit the capacity");

        assert_eq!(magazine.capacity(), 30);
        assert_eq!(magazine.rounds(), 7);
    }

    #[test]
    fn zero_capacity_magazine_is_empty_without_dividing_by_zero() {
        let magazine = Magazine::new(0, 0).expect("zero rounds fit zero capacity");

        assert_eq!(magazine.level(), MagazineLevel::Empty);
        assert_eq!(magazine.capacity(), 0);
    }

    #[test]
    fn deserialize_rejects_rounds_over_capacity() {
        let json = r#"{"capacity":30,"rounds":40}"#;

        let result = serde_json::from_str::<Magazine>(json);

        assert!(
            result.is_err(),
            "over-capacity data deserialized: {result:?}"
        );
    }

    #[test]
    fn serialized_magazine_round_trips_equal() {
        let original = Magazine::new(30, 7).expect("rounds fit the capacity");

        let json = serde_json::to_string(&original).expect("magazine serializes");
        let restored: Magazine = serde_json::from_str(&json).expect("valid data deserializes");

        assert_eq!(restored, original);
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
    }
}
