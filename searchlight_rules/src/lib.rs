//! Platform-free rules for the FPS line: items, magazine, rifle, loadout, body damage, lean, accuracy,
//! enemy grades and the grade mix of each wave.

mod accuracy;
mod ballistics;
mod body;
mod grade;
mod inventory;
mod item;
mod kit;
mod load;
mod loadout;
mod magazine;
mod mix;
mod posture;
mod rifle;
mod rig;
mod stats;
#[cfg(test)]
mod test_support;
mod weapon;

pub use accuracy::{Accuracy, AccuracyTuning, disc_offset};
pub use ballistics::{
    Ballistics, Flight, FlightBounds, GravityScale, GravityScaleError, MAX_GRAVITY_SCALE,
    MIN_SPEED, RoundStats, STANDARD_GRAVITY, arrival_damage,
};
pub use body::{Body, BodyPart, BodyProfile, HitReport};
pub use grade::{Grade, GradeDials, UnknownGrade};
pub use inventory::{Item, MoveRefused, Place, Remains, RoundStack, Source};
pub use item::{
    BackpackSpec, Caliber, ItemClass, ItemDef, ItemDefError, ItemId, ItemKind, ItemStat, Lifecycle,
    MagazineSpec, MagazineWell, RigSpec, RoundSpec, Size,
};
pub use kit::{Kit, KitDef, KitError, KitItem, KitMagazine};
pub use load::{
    LoadState, LoadThresholds, LoadThresholdsError, load, load_contributions, load_state,
};
pub use loadout::Loadout;
pub use magazine::{Magazine, MagazineError, MagazineLevel};
pub use mix::{GradeMix, GradeMixError, WaveGrades};
pub use posture::{BODY_RIGHT, Lean, LeanReach, LeanSide, LeanTuning, PartOffset, leaned_parts};
pub use rifle::{Rifle, RifleError, Shot};
pub use rig::Rig;
pub use stats::{
    Contribution, PlayerDef, PlayerDefError, SourceLabel, Stat, UnknownItem, Verb,
    equipment_contributions, explain, resolve,
};
pub use weapon::{FireBehaviour, RifleDef};
