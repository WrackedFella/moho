//! A rifle: one chambered round plus a magazine.

use crate::item::{ItemId, MagazineWell};
use crate::magazine::Magazine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Outcome of pulling the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shot {
    /// A round was spent.
    Fired,
    /// Nothing was chambered; no round was spent.
    Dry,
}

/// Why a magazine could not be put into a rifle.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RifleError {
    /// The magazine is for another well.
    #[error("magazine {magazine} does not fit a {well} well")]
    DoesNotFit {
        /// The magazine's item.
        magazine: ItemId,
        /// The rifle's well.
        well: MagazineWell,
    },
}

/// A rifle with a chamber and a magazine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rifle {
    well: MagazineWell,
    chamber: Option<ItemId>,
    magazine: Magazine,
}

impl Rifle {
    /// A rifle taking `well`, with `magazine` inserted and one round chambered from it,
    /// if it has any.
    ///
    /// # Errors
    ///
    /// [`RifleError::DoesNotFit`] when the magazine is for another well.
    pub fn loaded(well: MagazineWell, magazine: Magazine) -> Result<Self, RifleError> {
        let mut rifle = Self {
            well,
            chamber: None,
            magazine,
        };
        if !rifle.fits(&rifle.magazine) {
            return Err(RifleError::DoesNotFit {
                magazine: rifle.magazine.item().clone(),
                well: rifle.well,
            });
        }
        rifle.feed();
        Ok(rifle)
    }

    /// Chambers a round from the magazine when the chamber is empty.
    fn feed(&mut self) {
        if self.chamber.is_none() {
            self.chamber = self.magazine.pop();
        }
    }

    /// Whether `magazine` is for this rifle's well.
    #[must_use]
    pub fn fits(&self, magazine: &Magazine) -> bool {
        magazine.well() == &self.well
    }

    /// Whether a round is chambered.
    #[must_use]
    pub fn is_chambered(&self) -> bool {
        self.chamber.is_some()
    }

    /// The chambered round, if any.
    #[must_use]
    pub fn chamber(&self) -> Option<&ItemId> {
        self.chamber.as_ref()
    }

    /// The inserted magazine.
    #[must_use]
    pub fn magazine(&self) -> &Magazine {
        &self.magazine
    }

    /// Spends the chambered round, then chambers the next from the magazine.
    pub fn fire(&mut self) -> Shot {
        if self.chamber.take().is_none() {
            return Shot::Dry;
        }
        self.feed();
        Shot::Fired
    }

    /// Swaps in `spare` and returns the ejected magazine.
    ///
    /// The chambered round stays; an empty chamber is fed from the new magazine.
    ///
    /// # Errors
    ///
    /// Gives `spare` back, with the rifle unchanged, when it does not fit the well.
    pub fn reload(&mut self, spare: Magazine) -> Result<Magazine, Magazine> {
        if !self.fits(&spare) {
            return Err(spare);
        }
        let ejected = std::mem::replace(&mut self.magazine, spare);
        self.feed();
        Ok(ejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemId, MagazineWell};
    use crate::test_support::{id, mag, mag_of, rifle_well, spec};
    use proptest::prelude::*;

    fn loaded(magazine: Magazine) -> Rifle {
        Rifle::loaded(rifle_well(), magazine).expect("the test magazine fits the rifle")
    }

    #[test]
    fn firing_consumes_the_chamber_then_feeds_from_the_magazine() {
        let mut rifle = loaded(mag(30, 3));
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
    fn the_chamber_holds_the_round_popped_from_the_magazine() {
        let mut magazine = Magazine::empty(id("mag"), &spec("rifle", 5));
        magazine.load(id("a")).unwrap();
        magazine.load(id("b")).unwrap();
        magazine.load(id("c")).unwrap();

        let mut rifle = loaded(magazine);

        assert_eq!(rifle.chamber(), Some(&id("c")));
        assert_eq!(rifle.fire(), Shot::Fired);
        assert_eq!(rifle.chamber(), Some(&id("b")));
        assert_eq!(rifle.fire(), Shot::Fired);
        assert_eq!(rifle.chamber(), Some(&id("a")));
        assert_eq!(rifle.fire(), Shot::Fired);
        assert_eq!(rifle.chamber(), None);
    }

    #[test]
    fn loaded_refuses_a_magazine_that_does_not_fit() {
        let wrong = mag_of("mag_pistol", "pistol", 15, 15);

        let Err(error) = Rifle::loaded(rifle_well(), wrong) else {
            panic!("a pistol magazine was accepted by a rifle well");
        };

        assert!(
            matches!(
                &error,
                RifleError::DoesNotFit { magazine, well }
                    if *magazine == ItemId::new("mag_pistol") && *well == MagazineWell::new("rifle")
            ),
            "{error:?}"
        );
    }

    #[test]
    fn fits_compares_the_magazine_well() {
        let rifle = loaded(mag(30, 3));

        assert!(rifle.fits(&mag_of("other", "rifle", 20, 0)));
        assert!(!rifle.fits(&mag_of("other", "pistol", 20, 0)));
    }

    #[test]
    fn reload_swaps_the_magazine_and_keeps_the_chambered_round() {
        let mut rifle = loaded(mag(30, 10));

        let ejected = rifle.reload(mag(30, 20));

        assert_eq!(ejected, Ok(mag(30, 9)));
        assert!(rifle.is_chambered());
        assert_eq!(rifle.magazine(), &mag(30, 20));
    }

    #[test]
    fn reload_with_a_magazine_that_does_not_fit_changes_nothing() {
        let mut rifle = loaded(mag(30, 10));
        let before = rifle.clone();
        let wrong = mag_of("mag_pistol", "pistol", 15, 15);

        let result = rifle.reload(wrong.clone());

        assert_eq!(result, Err(wrong));
        assert_eq!(rifle, before);
    }

    #[test]
    fn reload_with_an_empty_chamber_chambers_from_the_new_magazine() {
        let mut rifle = loaded(mag(30, 0));

        let ejected = rifle.reload(mag(30, 5));

        assert_eq!(ejected, Ok(mag(30, 0)));
        assert!(rifle.is_chambered());
        assert_eq!(rifle.chamber(), Some(&id("ball")));
        assert_eq!(rifle.magazine().rounds(), 4);
    }

    #[test]
    fn reload_keeps_capacity_of_the_inserted_and_ejected_magazines() {
        let mut rifle = loaded(mag(30, 10));

        let ejected = rifle
            .reload(mag(12, 5))
            .expect("a fitting spare was supplied");

        assert_eq!(ejected.capacity(), 30);
        assert_eq!(rifle.magazine().capacity(), 12);
    }

    #[test]
    fn loaded_with_an_empty_magazine_has_nothing_chambered_and_fires_dry() {
        let mut rifle = loaded(mag(30, 0));

        let shot = rifle.fire();

        assert!(!rifle.is_chambered());
        assert_eq!(rifle.chamber(), None);
        assert_eq!(shot, Shot::Dry);
        assert_eq!(rifle.magazine().rounds(), 0);
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
            let mut rifle = loaded(mag(capacity, first));
            let mut loaded_rounds = first;
            let mut fired = 0u32;
            let mut ejected_rounds = 0u32;

            let bound = capacity + 2;
            for _ in 0..before_reload {
                if rifle.fire() == Shot::Fired { fired += 1; }
            }
            if let Some(spare) = spare {
                let spare = spare.min(capacity);
                let ejected = rifle.reload(mag(capacity, spare));
                prop_assert!(ejected.is_ok());
                ejected_rounds = ejected.map_or(0, |m| m.rounds());
                loaded_rounds += spare;
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
            prop_assert_eq!(fired + ejected_rounds, loaded_rounds);
        }
    }
}
