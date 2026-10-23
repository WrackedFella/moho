//! Platform-free rules for the FPS line: items, magazine, rifle, loadout and body damage.

mod body;
mod item;
mod kit;
mod loadout;
mod magazine;
mod rifle;
mod rig;
#[cfg(test)]
mod test_support;
mod weapon;

pub use body::{Body, BodyPart, BodyProfile, HitReport};
pub use item::{
    Caliber, ItemClass, ItemDef, ItemDefError, ItemId, ItemKind, Lifecycle, MagazineSpec,
    MagazineWell, RigSpec, RoundSpec, Size,
};
pub use kit::{Kit, KitDef, KitError, KitMagazine};
pub use loadout::Loadout;
pub use magazine::{Magazine, MagazineError, MagazineLevel};
pub use rifle::{Rifle, RifleError, Shot};
pub use rig::Rig;
pub use weapon::{FireBehaviour, RifleDef};
