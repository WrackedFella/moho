//! Platform-free rules for the FPS line: magazine, rifle, body damage and enemy grades.

mod body;
mod grade;
mod magazine;
mod posture;
mod rifle;
mod weapon;

pub use body::{Body, BodyPart, BodyProfile, HitReport};
pub use grade::{Grade, GradeDials, UnknownGrade};
pub use magazine::{Magazine, MagazineError, MagazineLevel};
pub use posture::{Lean, LeanReach, LeanSide, LeanTuning};
pub use rifle::{Rifle, Shot};
pub use weapon::{FireBehaviour, RifleDef};
