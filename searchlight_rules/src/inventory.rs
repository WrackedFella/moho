//! What is carried: instances of items, a dead enemy's remains, and the places a move names.

use crate::item::ItemId;
use crate::magazine::Magazine;
use crate::rifle::Rifle;
use crate::rig::Rig;
use thiserror::Error;

/// A stack of loose rounds of one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundStack {
    /// The round item.
    pub round: ItemId,
    /// How many rounds.
    pub count: u32,
}

/// One thing carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A rifle with the magazine in it.
    Weapon(Rifle),
    /// A magazine and the rounds in it.
    Magazine(Magazine),
    /// Loose rounds.
    Rounds(RoundStack),
    /// A rig by item id. Carried, it is a plain item holding no magazines; only a worn rig
    /// has slots.
    Rig(ItemId),
    /// Anything else by item id: backpacks, tools, bandages.
    Other(ItemId),
}

/// A dead enemy's gear. It has no limit on what it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Remains {
    /// The rig it wore, with its magazines.
    pub rig: Option<Rig>,
    /// The backpack it wore.
    pub backpack: Option<ItemId>,
    /// What it carried.
    pub carried: Vec<Item>,
}

/// Where a move takes an item from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The player's inventory, by index.
    Inventory(usize),
    /// The player's rig, by slot.
    MagazineSlot(usize),
    /// The body's carried items, by index.
    BodyCarried(usize),
    /// A magazine in the slot of the body's worn rig.
    BodyMagazineSlot(usize),
    /// The body's worn rig, with its magazines.
    BodyRig,
    /// The body's worn backpack.
    BodyBackpack,
}

impl Source {
    /// Whether this source is on the body being looted.
    #[must_use]
    pub fn on_body(self) -> bool {
        matches!(
            self,
            Self::BodyCarried(_) | Self::BodyMagazineSlot(_) | Self::BodyRig | Self::BodyBackpack
        )
    }
}

/// Where a move puts an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The end of the player's inventory.
    Inventory,
    /// The first free slot of the player's rig.
    MagazineSlots,
    /// The end of the body's carried items.
    Body,
}

/// Why a wear was refused. A refused wear changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum WearRefused {
    /// The item is neither a rig nor a backpack, or its definition is missing or not of that kind.
    #[error("that cannot be worn")]
    NotWearable,
    /// The inventory holds nothing at that index.
    #[error("nothing there to wear")]
    NoSuchItem,
}

/// Why a move was refused. A refused move changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum MoveRefused {
    /// The source holds nothing.
    #[error("nothing there to move")]
    NoSuchItem,
    /// The move names the body but no body was given.
    #[error("there is no body to move from or to")]
    NoBody,
    /// Only magazines go in magazine slots.
    #[error("only a magazine goes in a magazine slot")]
    NotAMagazine,
    /// Every magazine slot is taken.
    #[error("no free magazine slot")]
    NoFreeSlot,
}
