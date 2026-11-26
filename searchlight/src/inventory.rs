//! The inventory screen's data: the lists the shell draws, built from what the sim holds. The
//! screen only shows and asks; the sim owns the items and applies every move.

use moho_ui_shell::{ItemLists, ListId};
use searchlight_rules::{Loadout, Place, Remains, Source};

use crate::menu::MenuAction;

/// The label of a magazine slot with no magazine in it.
pub const EMPTY_SLOT: &str = "(empty)";

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
/// rounds' is its size. The magazine slots list has one row per slot, a free one labelled
/// [`EMPTY_SLOT`] with no count. The body's list shows its rig, the magazines in the rig (not
/// its free slots), its backpack, then its carried items.
pub fn lists(loadout: &Loadout, body: Option<&Remains>) -> ItemLists<MenuAction> {
    let _ = (loadout, body);
    panic!("not yet implemented: lists")
}

/// What row `row` of list `list` is a move of, given what the lists were built from, or `None`
/// for worn equipment, a row past the end, an unknown list, or the body list without a body.
pub fn source_of(
    list: ListId,
    row: usize,
    loadout: &Loadout,
    body: Option<&Remains>,
) -> Option<Source> {
    let _ = (list, row, loadout, body);
    panic!("not yet implemented: source_of")
}

/// Where dropping an item on list `list` puts it, or `None` for a list nothing drops on.
pub fn place_of(list: ListId) -> Option<Place> {
    let _ = list;
    panic!("not yet implemented: place_of")
}

#[cfg(test)]
mod tests {
    use searchlight_rules::{Item, ItemId, Place, Rig, RigSpec, Source};

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
        assert_eq!(
            labels(&shown, MAGAZINE_SLOTS),
            ["test_mag", "test_mag", EMPTY_SLOT]
        );
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
        assert_eq!(count_of(MAGAZINE_SLOTS), Some(vec![Some(30), None]));
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
    fn free_magazine_slots_show_as_empty_rows() {
        let loadout = player(4, 1);

        let shown = lists(&loadout, None);

        assert_eq!(
            labels(&shown, MAGAZINE_SLOTS),
            ["test_mag", EMPTY_SLOT, EMPTY_SLOT, EMPTY_SLOT]
        );
    }

    /// A player whose rig has magazines in slots 1 and 3 of 4, and two items in the inventory.
    fn sparse_player() -> Loadout {
        let rig = Rig::from_slots(
            ItemId::new("test_rig"),
            &RigSpec { magazine_slots: 4 },
            vec![None, Some(mag(30, 30)), None, Some(mag(30, 30))],
        )
        .expect("four slots for a four-slot spec");
        Loadout::new(rifle_from(mag(30, 30)), rig)
            .with_backpack(ItemId::new("backpack_small"))
            .with_inventory(vec![bandage(), Item::Other(ItemId::new("rag"))])
    }

    /// A body wearing a rig with magazines in slots 0 and 2 of 3, a backpack, and two items.
    fn full_body() -> Remains {
        Remains {
            rig: Some(
                Rig::from_slots(
                    ItemId::new("body_rig"),
                    &RigSpec { magazine_slots: 3 },
                    vec![Some(mag(30, 30)), None, Some(mag(30, 30))],
                )
                .expect("three slots for a three-slot spec"),
            ),
            backpack: Some(ItemId::new("backpack_small")),
            carried: vec![bandage(), Item::Other(ItemId::new("rag"))],
        }
    }

    fn sources(
        shown: &ItemLists<MenuAction>,
        list: ListId,
        loadout: &Loadout,
        body: Option<&Remains>,
    ) -> Vec<Option<Source>> {
        let rows = shown
            .lists
            .iter()
            .find(|candidate| candidate.id == list)
            .map_or(0, |list| list.rows.len());
        (0..rows)
            .map(|row| source_of(list, row, loadout, body))
            .collect()
    }

    #[test]
    fn every_row_maps_to_the_source_it_shows() {
        let loadout = sparse_player();
        let remains = full_body();

        let shown = lists(&loadout, Some(&remains));

        assert_eq!(
            sources(&shown, EQUIPMENT, &loadout, Some(&remains)),
            [None, None],
            "worn equipment does not move"
        );
        assert_eq!(
            sources(&shown, MAGAZINE_SLOTS, &loadout, Some(&remains)),
            [
                Some(Source::MagazineSlot(0)),
                Some(Source::MagazineSlot(1)),
                Some(Source::MagazineSlot(2)),
                Some(Source::MagazineSlot(3)),
            ]
        );
        assert_eq!(
            labels(&shown, MAGAZINE_SLOTS),
            [EMPTY_SLOT, "test_mag", EMPTY_SLOT, "test_mag"]
        );
        assert_eq!(
            sources(&shown, INVENTORY, &loadout, Some(&remains)),
            [Some(Source::Inventory(0)), Some(Source::Inventory(1))]
        );
        assert_eq!(
            labels(&shown, BODY),
            [
                "body_rig",
                "test_mag",
                "test_mag",
                "backpack_small",
                "bandage",
                "rag"
            ]
        );
        assert_eq!(
            sources(&shown, BODY, &loadout, Some(&remains)),
            [
                Some(Source::BodyRig),
                Some(Source::BodyMagazineSlot(0)),
                Some(Source::BodyMagazineSlot(2)),
                Some(Source::BodyBackpack),
                Some(Source::BodyCarried(0)),
                Some(Source::BodyCarried(1)),
            ]
        );
    }

    #[test]
    fn a_row_past_the_end_of_a_list_has_no_source() {
        let loadout = sparse_player();
        let remains = full_body();

        let past = [
            source_of(MAGAZINE_SLOTS, 4, &loadout, Some(&remains)),
            source_of(INVENTORY, 2, &loadout, Some(&remains)),
            source_of(BODY, 6, &loadout, Some(&remains)),
            source_of(BODY, 0, &loadout, None),
            source_of(ListId(9), 0, &loadout, Some(&remains)),
        ];

        assert_eq!(past, [None; 5]);
    }

    #[test]
    fn a_body_with_no_rig_or_backpack_maps_its_rows_to_carried_items() {
        let loadout = sparse_player();
        let remains = two_magazines();

        let mapped = [
            source_of(BODY, 0, &loadout, Some(&remains)),
            source_of(BODY, 1, &loadout, Some(&remains)),
        ];

        assert_eq!(
            mapped,
            [Some(Source::BodyCarried(0)), Some(Source::BodyCarried(1))]
        );
    }

    #[test]
    fn each_list_names_the_place_a_drop_on_it_means() {
        assert_eq!(place_of(INVENTORY), Some(Place::Inventory));
        assert_eq!(place_of(MAGAZINE_SLOTS), Some(Place::MagazineSlots));
        assert_eq!(place_of(BODY), Some(Place::Body));
        assert_eq!(place_of(EQUIPMENT), None);
        assert_eq!(place_of(ListId(9)), None);
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
