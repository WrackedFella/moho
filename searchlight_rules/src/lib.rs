//! Platform-free rules for the FPS line: items, magazine, rifle, loadout, body damage, lean,
//! enemy grades and the grade mix of each wave.

mod body;
mod grade;
mod item;
mod kit;
mod loadout;
mod magazine;
mod mix;
mod posture;
mod rifle;
mod rig;
#[cfg(test)]
mod test_support;
mod weapon;

pub use body::{Body, BodyPart, BodyProfile, HitReport};
pub use grade::{Grade, GradeDials, UnknownGrade};
pub use item::{
    Caliber, ItemClass, ItemDef, ItemDefError, ItemId, ItemKind, Lifecycle, MagazineSpec,
    MagazineWell, RigSpec, RoundSpec, Size,
};
pub use kit::{Kit, KitDef, KitError, KitMagazine};
pub use loadout::Loadout;
pub use magazine::{Magazine, MagazineError, MagazineLevel};
pub use mix::{GradeMix, GradeMixError, WaveGrades};
pub use posture::{Lean, LeanReach, LeanSide, LeanTuning};
pub use rifle::{Rifle, RifleError, Shot};
pub use rig::Rig;
pub use weapon::{FireBehaviour, RifleDef};
