//! Platform-free rules for the FPS line: magazine, rifle and body damage.

mod body;
mod magazine;
mod rifle;
mod weapon;

pub use body::{Body, BodyPart, BodyProfile, HitReport};
pub use magazine::{Magazine, MagazineError, MagazineLevel};
pub use rifle::{Rifle, Shot};
pub use weapon::{FireBehaviour, RifleDef};
