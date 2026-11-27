//! Shared builders for the crate's unit tests.

use crate::item::{Caliber, ItemId, MagazineSpec, MagazineWell, SlotKind};
use crate::magazine::Magazine;

pub(crate) fn id(text: &str) -> ItemId {
    ItemId::new(text)
}

pub(crate) fn spec(well: &str, capacity: u32) -> MagazineSpec {
    MagazineSpec {
        well: MagazineWell::new(well),
        caliber: Caliber::new("5.56x45"),
        slot: SlotKind::new("rifle"),
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

/// A rig section with `count` magazine slots of each `(kind, count)`, written as data is.
pub(crate) fn rig_spec(slots: &[(&str, u32)]) -> crate::item::RigSpec {
    let table: serde_json::Map<String, serde_json::Value> = slots
        .iter()
        .map(|&(kind, count)| (kind.to_owned(), serde_json::json!(count)))
        .collect();
    serde_json::from_value(serde_json::json!({ "magazine_slots": table }))
        .expect("a rig section lists its slots by kind")
}

/// A 30-round magazine item `item` for `well` that takes a slot of kind `slot`, holding
/// 30 rounds of `ball`, written as data is.
pub(crate) fn mag_in_slot(item: &str, well: &str, slot: &str) -> Magazine {
    let spec: MagazineSpec = serde_json::from_value(serde_json::json!({
        "well": well, "caliber": "5.56x45", "capacity": 30, "slot": slot
    }))
    .expect("a magazine section names the kind of slot it takes");
    Magazine::filled(id(item), &spec, &id("ball"), 30).expect("test rounds fit the capacity")
}
