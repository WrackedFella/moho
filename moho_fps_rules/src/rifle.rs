//! A rifle: one chambered round plus a magazine.

use crate::magazine::Magazine;
use serde::{Deserialize, Serialize};

/// Outcome of pulling the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shot {
    /// A round was spent.
    Fired,
    /// Nothing was chambered; no round was spent.
    Dry,
}

/// A rifle with a chamber and a magazine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rifle {
    chambered: bool,
    magazine: Magazine,
}

impl Rifle {
    /// A rifle with `magazine` inserted and one round chambered from it, if it has any.
    #[must_use]
    pub fn loaded(magazine: Magazine) -> Self {
        Self {
            chambered: false,
            magazine,
        }
    }

    /// Whether a round is chambered.
    #[must_use]
    pub fn is_chambered(&self) -> bool {
        self.chambered
    }

    /// The inserted magazine.
    #[must_use]
    pub fn magazine(&self) -> &Magazine {
        &self.magazine
    }

    /// Spends the chambered round, then chambers the next from the magazine.
    pub fn fire(&mut self) -> Shot {
        Shot::Dry
    }

    /// Swaps in `spare` and returns the ejected magazine.
    ///
    /// The chambered round stays; an empty chamber is fed from the new magazine.
    /// With no spare nothing changes and `None` is returned.
    pub fn reload(&mut self, spare: Option<Magazine>) -> Option<Magazine> {
        let _ = spare;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn mag(capacity: u32, rounds: u32) -> Magazine {
        Magazine::new(capacity, rounds).expect("test rounds fit the capacity")
    }

    #[test]
    fn firing_consumes_the_chamber_then_feeds_from_the_magazine() {
        let mut rifle = Rifle::loaded(mag(30, 3));
        assert!(rifle.is_chambered());
        assert_eq!(rifle.magazine().rounds(), 2);

        let first = rifle.fire();

        assert_eq!(first, Shot::Fired);
        assert!(rifle.is_chambered());
        assert_eq!(rifle.magazine().rounds(), 1);

        let rest: Vec<Shot> = (0..3).map(|_| rifle.fire()).collect();

        assert_eq!(rest, [Shot::Fired, Shot::Fired, Shot::Dry]);
        assert!(!rifle.is_chambered());
        assert_eq!(rifle.magazine().rounds(), 0);
    }

    #[test]
    fn reload_swaps_the_magazine_and_keeps_the_chambered_round() {
        let mut rifle = Rifle::loaded(mag(30, 10));

        let ejected = rifle.reload(Some(mag(30, 20)));

        assert_eq!(ejected, Some(mag(30, 9)));
        assert!(rifle.is_chambered());
        assert_eq!(rifle.magazine(), &mag(30, 20));
    }

    #[test]
    fn reload_without_a_spare_changes_nothing() {
        let mut rifle = Rifle::loaded(mag(30, 10));
        let before = rifle.clone();

        let ejected = rifle.reload(None);

        assert_eq!(ejected, None);
        assert_eq!(rifle, before);
    }

    #[test]
    fn reload_with_an_empty_chamber_chambers_from_the_new_magazine() {
        let mut rifle = Rifle::loaded(mag(30, 0));

        let ejected = rifle.reload(Some(mag(30, 5)));

        assert_eq!(ejected, Some(mag(30, 0)));
        assert!(rifle.is_chambered());
        assert_eq!(rifle.magazine().rounds(), 4);
    }

    proptest! {
        #[test]
        fn rounds_fired_never_exceed_rounds_loaded(
            capacity in 1u32..100,
            first in 0u32..100,
            spare in proptest::option::of(0u32..100),
            before_reload in 0u32..120,
        ) {
            let first = first.min(capacity);
            let mut rifle = Rifle::loaded(mag(capacity, first));
            let mut loaded = first;
            let mut fired = 0u32;
            let mut ejected_rounds = 0u32;

            let bound = capacity + 2;
            for _ in 0..before_reload {
                if rifle.fire() == Shot::Fired { fired += 1; }
            }
            if let Some(spare) = spare {
                let spare = spare.min(capacity);
                let ejected = rifle.reload(Some(mag(capacity, spare)));
                prop_assert!(ejected.is_some());
                ejected_rounds = ejected.map_or(0, |m| m.rounds());
                loaded += spare;
            }
            let mut drained = false;
            for _ in 0..bound {
                if rifle.fire() == Shot::Fired {
                    fired += 1;
                } else {
                    drained = true;
                    break;
                }
            }

            prop_assert!(drained, "rifle still firing after {} extra shots", bound);
            prop_assert_eq!(rifle.fire(), Shot::Dry);
            prop_assert_eq!(fired + ejected_rounds, loaded);
        }
    }
}
