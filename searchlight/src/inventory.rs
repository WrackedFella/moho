//! The inventory screen's data: the lists the shell draws, built from what the sim holds. The
//! screen only shows and asks; the sim owns the items and applies every move.

use moho_ui_shell::{ItemList, ItemLists, ListId};
use searchlight_rules::{
    Item, ItemDef, ItemId, LoadThresholds, Loadout, Magazine, Place, Remains, Source,
};

use crate::menu::{MenuAction, item_row};

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

/// The gear screen, left to right: equipment, magazine slots and inventory. Escape is back.
///
/// A row is labelled with its item's id; a magazine's count is its rounds and a stack of
/// rounds' is its size. The magazine slots list has one row per slot, a free one labelled
/// [`EMPTY_SLOT`] with no count.
///
/// The rig and backpack rows of the inventory offer [`ItemAction::Wear`], `lookup` telling a
/// backpack from other items, and the slot list's title names the worn rig and its slot count.
pub fn gear_lists<'a>(
    loadout: &Loadout,
    lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
) -> ItemLists<MenuAction> {
    let _ = lookup;
    ItemLists {
        lists: vec![
            list(EQUIPMENT, "Equipment", loadout, None),
            list(MAGAZINE_SLOTS, "Magazine slots", loadout, None),
            list(INVENTORY, "Inventory", loadout, None),
        ],
        back: Some(MenuAction::Back),
    }
}

/// The loot screen, left to right: the player's flat inventory and one container's flat
/// contents. This is the shape every container takes; today the only one is a dead enemy's
/// body, whose list shows its rig, the magazines in the rig (not its free slots), its backpack,
/// then its carried items.
pub fn loot_lists(loadout: &Loadout, container: &Remains) -> ItemLists<MenuAction> {
    ItemLists {
        lists: vec![
            list(INVENTORY, "Inventory", loadout, Some(container)),
            list(BODY, "Body", loadout, Some(container)),
        ],
        back: Some(MenuAction::Back),
    }
}

/// What row `row` of list `list` is a move of, given what the lists were built from, or `None`
/// for worn equipment, a row past the end, an unknown list, or the body list without a body.
pub fn source_of(
    list: ListId,
    row: usize,
    loadout: &Loadout,
    body: Option<&Remains>,
) -> Option<Source> {
    entries(list, loadout, body)
        .into_iter()
        .nth(row)
        .and_then(|entry| entry.source)
}

/// Where dropping an item on list `list` puts it, or `None` for a list nothing drops on.
pub fn place_of(list: ListId) -> Option<Place> {
    match list {
        INVENTORY => Some(Place::Inventory),
        MAGAZINE_SLOTS => Some(Place::MagazineSlots),
        BODY => Some(Place::Body),
        _ => None,
    }
}

/// The load line above the inventory: carried kilograms against capacity, and where each
/// threshold falls on that scale.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadHeader {
    /// "20.0 of 30.0 kg".
    pub text: String,
    /// The load in kilograms at which the player is slowed, cannot sprint and cannot move.
    pub marks: [f32; 3],
}

/// The header for `load_kg` carried against `capacity_kg`, with the marks of `thresholds`.
pub fn load_header(load_kg: f32, capacity_kg: f32, thresholds: &LoadThresholds) -> LoadHeader {
    LoadHeader {
        text: format!("{load_kg:.1} of {capacity_kg:.1} kg"),
        marks: [
            thresholds.slowed * capacity_kg,
            thresholds.no_sprint * capacity_kg,
            thresholds.cannot_move * capacity_kg,
        ],
    }
}

/// An action a row of the inventory offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    /// Wears the rig or backpack the row shows.
    Wear,
}

/// One row before it is drawn: what it shows and what moving it means.
struct Entry {
    label: String,
    count: Option<u32>,
    source: Option<Source>,
}

impl Entry {
    fn new(label: impl ToString, count: Option<u32>, source: Option<Source>) -> Self {
        Self {
            label: label.to_string(),
            count,
            source,
        }
    }

    fn of_item(item: &Item, source: Source) -> Self {
        match item {
            Item::Weapon(_) => Self::new("Rifle", None, Some(source)),
            Item::Magazine(magazine) => Self::of_magazine(magazine, source),
            Item::Rounds(stack) => Self::new(&stack.round, Some(stack.count), Some(source)),
            Item::Rig(rig) => Self::new(rig, None, Some(source)),
            Item::Other(id) => Self::new(id, None, Some(source)),
        }
    }

    fn of_magazine(magazine: &Magazine, source: Source) -> Self {
        Self::new(magazine.item(), Some(magazine.rounds()), Some(source))
    }
}

/// The rows of list `list` in order; none for an unknown list or the body's without a body.
fn entries(list: ListId, loadout: &Loadout, body: Option<&Remains>) -> Vec<Entry> {
    match list {
        EQUIPMENT => std::iter::once(Entry::new(loadout.rig().item(), None, None))
            .chain(
                loadout
                    .backpack()
                    .map(|backpack| Entry::new(backpack, None, None)),
            )
            .collect(),
        MAGAZINE_SLOTS => loadout
            .rig()
            .slots()
            .iter()
            .enumerate()
            .map(|(slot, held)| match held {
                Some(magazine) => Entry::of_magazine(magazine, Source::MagazineSlot(slot)),
                None => Entry::new(EMPTY_SLOT, None, Some(Source::MagazineSlot(slot))),
            })
            .collect(),
        INVENTORY => loadout
            .inventory()
            .iter()
            .enumerate()
            .map(|(index, item)| Entry::of_item(item, Source::Inventory(index)))
            .collect(),
        BODY => body.map(body_entries).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn body_entries(body: &Remains) -> Vec<Entry> {
    let mut rows = Vec::new();
    if let Some(rig) = &body.rig {
        rows.push(Entry::new(rig.item(), None, Some(Source::BodyRig)));
        for (slot, held) in rig.slots().iter().enumerate() {
            if let Some(magazine) = held {
                rows.push(Entry::of_magazine(magazine, Source::BodyMagazineSlot(slot)));
            }
        }
    }
    if let Some(backpack) = &body.backpack {
        rows.push(Entry::new(backpack, None, Some(Source::BodyBackpack)));
    }
    rows.extend(
        body.carried
            .iter()
            .enumerate()
            .map(|(index, item)| Entry::of_item(item, Source::BodyCarried(index))),
    );
    rows
}

fn list(
    id: ListId,
    title: &str,
    loadout: &Loadout,
    body: Option<&Remains>,
) -> ItemList<MenuAction> {
    let rows = entries(id, loadout, body)
        .into_iter()
        .enumerate()
        .map(|(row, entry)| item_row(row, entry.label, entry.count))
        .collect();
    ItemList {
        id,
        title: title.to_owned(),
        rows,
    }
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
    fn a_looted_body_lists_its_two_magazines_beside_the_inventory() {
        let loadout = player(3, 2);
        let remains = two_magazines();

        let shown = loot_lists(&loadout, &remains);

        assert_eq!(titles(&shown), ["Inventory", "Body"]);
        assert_eq!(list_ids(&shown), [INVENTORY, BODY]);
        assert_eq!(labels(&shown, BODY), ["test_mag", "test_mag"]);
        assert_eq!(labels(&shown, INVENTORY), ["bandage"]);
        assert_eq!(shown.back, Some(MenuAction::Back));
    }

    fn list_ids(lists: &ItemLists<MenuAction>) -> Vec<ListId> {
        lists.lists.iter().map(|list| list.id).collect()
    }

    #[test]
    fn the_loot_screen_has_exactly_the_inventory_then_the_body() {
        let loadout = player(3, 2);

        let shown = loot_lists(&loadout, &full_body());

        assert_eq!(list_ids(&shown), [INVENTORY, BODY]);
        assert_eq!(titles(&shown), ["Inventory", "Body"]);
    }

    #[test]
    fn the_gear_screen_still_has_equipment_magazine_slots_and_inventory() {
        let loadout = player(3, 2);

        let shown = gear_lists(&loadout, |_| None);

        assert_eq!(list_ids(&shown), [EQUIPMENT, MAGAZINE_SLOTS, INVENTORY]);
        let shown_titles = titles(&shown);
        assert_eq!(shown_titles[0], "Equipment");
        assert!(
            shown_titles[1].starts_with("Magazine slots"),
            "{shown_titles:?}"
        );
        assert_eq!(shown_titles[2], "Inventory");
        assert_eq!(shown.back, Some(MenuAction::Back));
    }

    #[test]
    fn the_loot_screens_inventory_list_is_the_one_the_gear_screen_shows() {
        let loadout = sparse_player();

        let gear = gear_lists(&loadout, |_| None);
        let loot = loot_lists(&loadout, &full_body());

        assert_eq!(labels(&loot, INVENTORY), labels(&gear, INVENTORY));
        assert_eq!(labels(&loot, INVENTORY), ["bandage", "rag"]);
    }

    #[test]
    fn the_loot_screens_rows_map_to_the_sources_they_show() {
        let loadout = sparse_player();
        let remains = full_body();

        let shown = loot_lists(&loadout, &remains);

        assert_eq!(
            sources(&shown, INVENTORY, &loadout, Some(&remains)),
            [Some(Source::Inventory(0)), Some(Source::Inventory(1))]
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
        assert_eq!(
            [place_of(INVENTORY), place_of(BODY)],
            [Some(Place::Inventory), Some(Place::Body)]
        );
    }

    #[test]
    fn a_magazine_row_counts_its_rounds() {
        let loadout = player(2, 1);
        let remains = Remains {
            carried: vec![Item::Magazine(mag(30, 12))],
            ..Remains::default()
        };

        let loot = loot_lists(&loadout, &remains);
        let gear = gear_lists(&loadout, |_| None);

        let count_of = |shown: &ItemLists<MenuAction>, list: ListId| {
            shown
                .lists
                .iter()
                .find(|candidate| candidate.id == list)
                .map(|list| list.rows.iter().map(|row| row.count).collect::<Vec<_>>())
        };
        assert_eq!(count_of(&loot, BODY), Some(vec![Some(12)]));
        assert_eq!(count_of(&gear, MAGAZINE_SLOTS), Some(vec![Some(30), None]));
        assert_eq!(
            count_of(&loot, MAGAZINE_SLOTS),
            None,
            "no slots on the loot screen"
        );
    }

    #[test]
    fn without_a_body_the_screen_lists_only_the_players_side() {
        let loadout = player(3, 1);

        let shown = gear_lists(&loadout, |_| None);

        let shown_titles = titles(&shown);
        assert_eq!(shown_titles[0], "Equipment");
        assert!(
            shown_titles[1].starts_with("Magazine slots"),
            "{shown_titles:?}"
        );
        assert_eq!(shown_titles[2], "Inventory");
        assert_eq!(shown_titles.len(), 3);
    }

    #[test]
    fn the_equipment_list_shows_the_worn_rig_and_backpack() {
        let loadout = player(3, 0);

        let shown = gear_lists(&loadout, |_| None);

        assert_eq!(labels(&shown, EQUIPMENT), ["test_rig", "backpack_small"]);
    }

    #[test]
    fn free_magazine_slots_show_as_empty_rows() {
        let loadout = player(4, 1);

        let shown = gear_lists(&loadout, |_| None);

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

        let shown = gear_lists(&loadout, |_| None);
        let loot = loot_lists(&loadout, &remains);

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
            labels(&loot, BODY),
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
            sources(&loot, BODY, &loadout, Some(&remains)),
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

        let shown = loot_lists(&loadout, &remains);

        assert_eq!(
            labels(&shown, BODY),
            ["test_rig", "test_mag", "backpack_small", "bandage"]
        );
    }

    #[test]
    fn a_body_that_carried_nothing_is_an_empty_list_not_a_missing_one() {
        let loadout = player(3, 0);
        let remains = Remains::default();

        let shown = loot_lists(&loadout, &remains);

        assert_eq!(titles(&shown), ["Inventory", "Body"]);
        assert!(labels(&shown, BODY).is_empty());
    }

    const THRESHOLDS: LoadThresholds = LoadThresholds {
        slowed: 0.6,
        no_sprint: 0.8,
        cannot_move: 1.0,
    };

    #[test]
    fn the_header_reads_20_of_30_kg_with_three_marks() {
        let header = load_header(20.0, 30.0, &THRESHOLDS);

        assert_eq!(header.text, "20.0 of 30.0 kg");
        assert_eq!(header.marks, [18.0, 24.0, 30.0]);
    }

    fn gear_defs() -> std::collections::BTreeMap<ItemId, ItemDef> {
        use crate::test_support::weighing;
        use searchlight_rules::{BackpackSpec, ItemClass, ItemKind};

        let rig = |slots| ItemDef {
            class: ItemClass::Gear,
            kind: ItemKind::Rig(RigSpec {
                magazine_slots: slots,
            }),
            ..weighing(1.0)
        };
        let pack = ItemDef {
            class: ItemClass::Gear,
            kind: ItemKind::Backpack(BackpackSpec {}),
            ..weighing(1.0)
        };
        [
            ("test_rig", rig(3)),
            ("rig_assault", rig(5)),
            ("backpack_small", pack.clone()),
            ("backpack_large", pack),
            ("bandage", weighing(0.05)),
            ("rag", weighing(0.1)),
        ]
        .into_iter()
        .map(|(id, def)| (ItemId::new(id), def))
        .collect()
    }

    fn wear_action() -> moho_ui_shell::RowAction<MenuAction> {
        moho_ui_shell::RowAction {
            label: "Wear".to_owned(),
            action: MenuAction::ItemAction(ItemAction::Wear),
            target: moho_ui_shell::ActionTarget::Itself,
        }
    }

    fn actions_of(
        lists: &ItemLists<MenuAction>,
        list: ListId,
    ) -> Vec<Vec<moho_ui_shell::RowAction<MenuAction>>> {
        lists
            .lists
            .iter()
            .find(|candidate| candidate.id == list)
            .unwrap_or_else(|| panic!("no list {list:?} on the screen"))
            .rows
            .iter()
            .map(|row| row.actions.clone())
            .collect()
    }

    /// For each row of `list`, whether it offers the Wear action.
    fn offering_wear(lists: &ItemLists<MenuAction>, list: ListId) -> Vec<bool> {
        actions_of(lists, list)
            .iter()
            .map(|actions| actions.contains(&wear_action()))
            .collect()
    }

    /// A player whose inventory holds a bandage, a 5-slot rig, a large backpack and a rag.
    fn player_with_spare_gear() -> Loadout {
        player(3, 1).with_inventory(vec![
            bandage(),
            Item::Rig(ItemId::new("rig_assault")),
            Item::Other(ItemId::new("backpack_large")),
            Item::Other(ItemId::new("rag")),
        ])
    }

    #[test]
    fn gear_screen_rig_and_backpack_rows_offer_wear() {
        let defs = gear_defs();
        let loadout = player_with_spare_gear();

        let shown = gear_lists(&loadout, |id| defs.get(id));

        assert_eq!(offering_wear(&shown, INVENTORY), [false, true, true, false]);
        assert_eq!(
            offering_wear(&shown, EQUIPMENT),
            [false, false],
            "worn gear offers no wear"
        );
        assert_eq!(
            offering_wear(&shown, MAGAZINE_SLOTS),
            [false, false, false],
            "a magazine slot offers no wear"
        );
        assert_eq!(list_ids(&shown), [EQUIPMENT, MAGAZINE_SLOTS, INVENTORY]);
    }

    #[test]
    fn a_rig_or_backpack_with_no_definition_offers_no_wear() {
        let defs = gear_defs();
        let loadout = player(3, 0).with_inventory(vec![
            Item::Rig(ItemId::new("rig_unknown")),
            Item::Other(ItemId::new("backpack_unknown")),
            Item::Other(ItemId::new("rig_assault")),
            Item::Other(ItemId::new("backpack_large")),
        ]);

        let shown = gear_lists(&loadout, |id| defs.get(id));

        assert_eq!(
            actions_of(&shown, INVENTORY),
            [vec![], vec![], vec![], vec![wear_action()]],
            "an unknown rig, an unknown backpack and a rig carried as a plain item offer none"
        );
    }

    #[test]
    fn loot_screen_rows_offer_no_wear() {
        let loadout = player_with_spare_gear();
        let remains = full_body();

        let shown = loot_lists(&loadout, &remains);

        for list in [INVENTORY, BODY] {
            let offers = offering_wear(&shown, list);
            assert!(
                offers.iter().all(|offered| !offered),
                "{list:?}: {offers:?}"
            );
        }
    }

    #[test]
    fn the_slot_list_title_names_the_worn_rig_and_its_slots() {
        let defs = gear_defs();
        let three = player(3, 2);
        let mut rig = Rig::new(ItemId::new("rig_assault"), &RigSpec { magazine_slots: 5 });
        rig.stow(mag(30, 30)).expect("the rig has a free slot");
        let five = Loadout::new(rifle_from(mag(30, 30)), rig);

        let before = gear_lists(&three, |id| defs.get(id));
        let after = gear_lists(&five, |id| defs.get(id));

        let title = |shown: &ItemLists<MenuAction>| {
            shown
                .lists
                .iter()
                .find(|list| list.id == MAGAZINE_SLOTS)
                .expect("the gear screen has the slot list")
                .title
                .clone()
        };
        let tokens = |text: &str| -> Vec<String> {
            text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|token| !token.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let (before, after) = (title(&before), title(&after));
        let (before_tokens, after_tokens) = (tokens(&before), tokens(&after));
        assert!(before.starts_with("Magazine slots"), "{before}");
        assert!(before_tokens.contains(&"test_rig".to_owned()), "{before}");
        assert!(before_tokens.contains(&"3".to_owned()), "{before}");
        assert!(
            !before_tokens.contains(&"1".to_owned()),
            "free count: {before}"
        );
        assert!(after.starts_with("Magazine slots"), "{after}");
        assert!(after_tokens.contains(&"rig_assault".to_owned()), "{after}");
        assert!(after_tokens.contains(&"5".to_owned()), "{after}");
        assert!(
            !after_tokens.contains(&"4".to_owned()),
            "free count: {after}"
        );
        assert!(!after.contains("test_rig"), "{after}");
    }

    #[test]
    fn the_marks_move_with_the_capacity_and_the_text_rounds_to_a_tenth() {
        let header = load_header(19.96, 40.0, &THRESHOLDS);

        assert_eq!(header.text, "20.0 of 40.0 kg");
        assert_eq!(header.marks, [24.0, 32.0, 40.0]);
    }
}
