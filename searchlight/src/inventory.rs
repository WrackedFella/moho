//! The inventory screen's data: the lists the shell draws, built from what the sim holds. The
//! screen only shows and asks; the sim owns the items and applies every move.

use moho_ui_shell::{ItemLists, ListId};
use searchlight_rules::{Loadout, Remains};

use crate::menu::MenuAction;

/// The worn rig and backpack; nothing in it moves.
pub const EQUIPMENT: ListId = ListId(0);
/// The magazines in the rig's slots.
pub const MAGAZINE_SLOTS: ListId = ListId(1);
/// Everything carried that is not held, worn or in a slot.
pub const INVENTORY: ListId = ListId(2);
/// The body being looted.
pub const BODY: ListId = ListId(3);

/// The lists of the screen, left to right: equipment, magazine slots and inventory, then the
/// body's list while `body` is being looted. Escape is back.
///
/// A row is labelled with its item's id; a magazine's count is its rounds and a stack of
/// rounds' is its size.
pub fn lists(loadout: &Loadout, body: Option<&Remains>) -> ItemLists<MenuAction> {
    let _ = (loadout, body);
    panic!("not yet implemented: lists")
}

#[cfg(test)]
mod tests {
    use searchlight_rules::{Item, ItemId, Rig, RigSpec};

    use super::*;
    use crate::test_support::{mag, rifle_from};

    fn bandage() -> Item {
        Item::Other(ItemId::new("bandage"))
    }

    fn rig_with(slots: u32, held: usize) -> Rig {
        let mut rig = Rig::new(
            ItemId::new("test_rig"),
            &RigSpec {
                magazine_slots: slots,
            },
        );
        for _ in 0..held {
            rig.stow(mag(30, 30)).expect("the rig has a free slot");
        }
        rig
    }

    /// A player with `held` magazines in a rig of `slots`, a small backpack and a bandage.
    fn player(slots: u32, held: usize) -> Loadout {
        let rig = rig_with(slots, held);
        Loadout::new(rifle_from(mag(30, 30)), rig)
            .with_backpack(ItemId::new("backpack_small"))
            .with_inventory(vec![bandage()])
    }

    fn two_magazines() -> Remains {
        Remains {
            carried: vec![Item::Magazine(mag(30, 30)), Item::Magazine(mag(30, 30))],
            ..Remains::default()
        }
    }

    fn titles(lists: &ItemLists<MenuAction>) -> Vec<&str> {
        lists.lists.iter().map(|list| list.title.as_str()).collect()
    }

    fn labels(lists: &ItemLists<MenuAction>, list: ListId) -> Vec<&str> {
        lists
            .lists
            .iter()
            .find(|candidate| candidate.id == list)
            .unwrap_or_else(|| panic!("no list {list:?} on the screen"))
            .rows
            .iter()
            .map(|row| row.label.as_str())
            .collect()
    }

    #[test]
    fn a_looted_body_lists_its_two_magazines_beside_slots_and_inventory() {
        let loadout = player(3, 2);
        let remains = two_magazines();

        let shown = lists(&loadout, Some(&remains));

        assert_eq!(
            titles(&shown),
            ["Equipment", "Magazine slots", "Inventory", "Body"]
        );
        let ids: Vec<ListId> = shown.lists.iter().map(|list| list.id).collect();
        assert_eq!(ids, [EQUIPMENT, MAGAZINE_SLOTS, INVENTORY, BODY]);
        assert_eq!(labels(&shown, BODY), ["test_mag", "test_mag"]);
        assert_eq!(labels(&shown, MAGAZINE_SLOTS), ["test_mag", "test_mag"]);
        assert_eq!(labels(&shown, INVENTORY), ["bandage"]);
        assert_eq!(shown.back, Some(MenuAction::Back));
    }

    #[test]
    fn a_magazine_row_counts_its_rounds() {
        let loadout = player(2, 1);
        let remains = Remains {
            carried: vec![Item::Magazine(mag(30, 12))],
            ..Remains::default()
        };

        let shown = lists(&loadout, Some(&remains));

        let count_of = |list: ListId| {
            shown
                .lists
                .iter()
                .find(|candidate| candidate.id == list)
                .map(|list| list.rows.iter().map(|row| row.count).collect::<Vec<_>>())
        };
        assert_eq!(count_of(BODY), Some(vec![Some(12)]));
        assert_eq!(count_of(MAGAZINE_SLOTS), Some(vec![Some(30)]));
    }

    #[test]
    fn without_a_body_the_screen_lists_only_the_players_side() {
        let loadout = player(3, 1);

        let shown = lists(&loadout, None);

        assert_eq!(titles(&shown), ["Equipment", "Magazine slots", "Inventory"]);
    }

    #[test]
    fn the_equipment_list_shows_the_worn_rig_and_backpack() {
        let loadout = player(3, 0);

        let shown = lists(&loadout, None);

        assert_eq!(labels(&shown, EQUIPMENT), ["test_rig", "backpack_small"]);
    }

    #[test]
    fn free_magazine_slots_are_not_rows() {
        let loadout = player(4, 1);

        let shown = lists(&loadout, None);

        assert_eq!(labels(&shown, MAGAZINE_SLOTS), ["test_mag"]);
    }

    #[test]
    fn the_body_list_shows_its_rig_the_rigs_magazines_its_backpack_then_what_it_carried() {
        let loadout = player(3, 0);
        let remains = Remains {
            rig: Some(rig_with(2, 1)),
            backpack: Some(ItemId::new("backpack_small")),
            carried: vec![bandage()],
        };

        let shown = lists(&loadout, Some(&remains));

        assert_eq!(
            labels(&shown, BODY),
            ["test_rig", "test_mag", "backpack_small", "bandage"]
        );
    }

    #[test]
    fn a_body_that_carried_nothing_is_an_empty_list_not_a_missing_one() {
        let loadout = player(3, 0);
        let remains = Remains::default();

        let shown = lists(&loadout, Some(&remains));

        assert_eq!(titles(&shown).last(), Some(&"Body"));
        assert!(labels(&shown, BODY).is_empty());
    }
}
