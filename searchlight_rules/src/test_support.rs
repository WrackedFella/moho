//! Shared builders for the crate's unit tests.

use crate::item::{Caliber, ItemId, MagazineSpec, MagazineWell};
use crate::magazine::Magazine;

pub(crate) fn id(text: &str) -> ItemId {
    ItemId::new(text)
}

pub(crate) fn spec(well: &str, capacity: u32) -> MagazineSpec {
    MagazineSpec {
        well: MagazineWell::new(well),
        caliber: Caliber::new("5.56x45"),
        capacity,
    }
}

/// A magazine item `item` for well `well`, holding `rounds` of `ball`.
pub(crate) fn mag_of(item: &str, well: &str, capacity: u32, rounds: u32) -> Magazine {
    Magazine::filled(id(item), &spec(well, capacity), &id("ball"), rounds)
        .expect("test rounds fit the capacity")
}

/// A magazine `mag` for the `rifle` well.
pub(crate) fn mag(capacity: u32, rounds: u32) -> Magazine {
    mag_of("mag", "rifle", capacity, rounds)
}

pub(crate) fn rifle_well() -> MagazineWell {
    MagazineWell::new("rifle")
}
