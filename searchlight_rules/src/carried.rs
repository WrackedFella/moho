//! Every item a loadout or a body's remains holds, by id: the one walk over their shape that
//! load and the content setup share.

use crate::inventory::{Item, Remains};
use crate::item::ItemId;
use crate::loadout::Loadout;
use crate::magazine::Magazine;
use crate::rifle::Rifle;
use crate::rig::Rig;

/// An item id and how many of it are carried.
pub type Carried = (ItemId, u32);

fn magazine_items(magazine: &Magazine, out: &mut Vec<Carried>) {
    out.push((magazine.item().clone(), 1));
    out.extend(
        magazine
            .round_items()
            .iter()
            .map(|round| (round.clone(), 1)),
    );
}

fn rifle_items(rifle: &Rifle, weapon: &ItemId, out: &mut Vec<Carried>) {
    out.push((weapon.clone(), 1));
    magazine_items(rifle.magazine(), out);
    out.extend(rifle.chamber().map(|round| (round.clone(), 1)));
}

fn rig_items(rig: &Rig, out: &mut Vec<Carried>) {
    out.push((rig.item().clone(), 1));
    for magazine in rig.magazines() {
        magazine_items(magazine, out);
    }
}

fn item_items(item: &Item, weapon: &ItemId, out: &mut Vec<Carried>) {
    match item {
        Item::Weapon(rifle) => rifle_items(rifle, weapon, out),
        Item::Magazine(magazine) => magazine_items(magazine, out),
        Item::Rounds(stack) => out.push((stack.round.clone(), stack.count)),
        Item::Rig(id) | Item::Other(id) => out.push((id.clone(), 1)),
    }
}

/// Everything `loadout` carries, in the order it is worn and held: the held rifle (item
/// `weapon`, also the item of any rifle in the inventory) with its magazine, rounds and
/// chambered round, the worn rig with its magazines, the backpack, then the inventory.
/// A magazine's rounds are listed one by one; loose rounds come as one entry with their count.
#[must_use]
pub fn loadout_items(loadout: &Loadout, weapon: &ItemId) -> Vec<Carried> {
    let mut out = Vec::new();
    rifle_items(loadout.rifle(), weapon, &mut out);
    rig_items(loadout.rig(), &mut out);
    out.extend(loadout.backpack().map(|backpack| (backpack.clone(), 1)));
    for item in loadout.inventory() {
        item_items(item, weapon, &mut out);
    }
    out
}

/// Everything `remains` holds: the rig with its magazines, the backpack, then what was carried.
/// `weapon` is the item of any rifle among it.
#[must_use]
pub fn remains_items(remains: &Remains, weapon: &ItemId) -> Vec<Carried> {
    let mut out = Vec::new();
    if let Some(rig) = &remains.rig {
        rig_items(rig, &mut out);
    }
    out.extend(
        remains
            .backpack
            .iter()
            .map(|backpack| (backpack.clone(), 1)),
    );
    for item in &remains.carried {
        item_items(item, weapon, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RoundStack;

    fn id(name: &str) -> ItemId {
        ItemId::new(name)
    }

    #[test]
    fn remains_list_the_backpack_then_what_was_carried_with_loose_round_counts() {
        let remains = Remains {
            rig: None,
            backpack: Some(id("pack")),
            carried: vec![
                Item::Other(id("rag")),
                Item::Rounds(RoundStack {
                    round: id("round"),
                    count: 7,
                }),
            ],
        };

        let items = remains_items(&remains, &id("rifle"));

        assert_eq!(items, [(id("pack"), 1), (id("rag"), 1), (id("round"), 7)]);
    }

    #[test]
    fn empty_remains_list_nothing() {
        assert_eq!(remains_items(&Remains::default(), &id("rifle")), []);
    }
}
