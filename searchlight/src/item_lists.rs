//! The demo contents of the item lists screen: the player's carried items and a test container,
//! and the game's rule for moving a row between them. Stands in until the inventory owns the
//! screen.

use moho_ui_shell::{ActionTarget, ItemList, ItemLists, ListId, ListRow, RowAction};

use crate::menu::MenuAction;

const PLAYER: ListId = ListId(0);
const CONTAINER: ListId = ListId(1);
/// The most rows the player's list holds.
const PLAYER_ROWS: usize = 8;
/// How many filler rows follow the rifle in the container.
const FILLER_ROWS: u32 = 45;

/// What a row of the demo lists can be asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    /// Uses one of the item (a bandage).
    Use,
    /// Loads a magazine from a stack of rounds.
    LoadFromStack,
}

/// The rounds a magazine holds.
const MAGAZINE_CAPACITY: u32 = 30;
/// The id of the player's rounds, the only valid target of loading a magazine.
const ROUNDS: u32 = 2;

/// One stack of an item: its stable id, label, count, description and the action it offers.
#[derive(Clone, Debug)]
struct Item {
    id: u32,
    label: String,
    count: u32,
    description: String,
    action: Option<ItemAction>,
}

impl Item {
    fn new(id: u32, label: &str, count: u32, description: &str) -> Self {
        Self {
            id,
            label: label.to_owned(),
            count,
            description: description.to_owned(),
            action: None,
        }
    }

    fn offering(mut self, action: ItemAction) -> Self {
        self.action = Some(action);
        self
    }

    /// The row action this item offers, if any.
    fn row_actions(&self) -> Vec<RowAction<MenuAction>> {
        let Some(action) = self.action else {
            return Vec::new();
        };
        let (label, target) = match action {
            ItemAction::Use => ("Use", ActionTarget::Itself),
            ItemAction::LoadFromStack => ("Load from stack", ActionTarget::AnotherRow),
        };
        vec![RowAction {
            label: label.to_owned(),
            action: MenuAction::ItemAction(action),
            target,
        }]
    }
}

/// The player's list and the test container.
#[derive(Clone, Debug)]
pub struct DemoItems {
    player: Vec<Item>,
    container: Vec<Item>,
}

impl Default for DemoItems {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoItems {
    /// The player's list with Bandage 3 and Rounds 60, and the container with Rifle 1 and filler.
    pub fn new() -> Self {
        let mut container = vec![
            Item::new(10, "Rifle", 1, "Semi-auto.\nNeeds rounds."),
            Item::new(11, "Magazine", 0, "Holds 30 rounds.").offering(ItemAction::LoadFromStack),
        ];
        container.extend((1..=FILLER_ROWS).map(|n| {
            Item::new(
                100 + n,
                &format!("Scrap {n:02}"),
                1 + n % 5,
                "Spare parts of no use.",
            )
        }));
        Self {
            player: vec![
                Item::new(1, "Bandage", 3, "Clean cloth.\nStops bleeding.")
                    .offering(ItemAction::Use),
                Item::new(ROUNDS, "Rounds", 60, "Loose rifle rounds."),
            ],
            container,
        }
    }

    /// The two lists, the player's first, as the shell draws them; Escape is Back.
    pub fn lists(&self) -> ItemLists<MenuAction> {
        let list = |id, title: &str, items: &[Item]| ItemList {
            id,
            title: title.to_owned(),
            rows: items
                .iter()
                .map(|item| ListRow {
                    label: item.label.clone(),
                    count: Some(item.count),
                    tooltip: Some(item.description.clone()),
                    item: MenuAction::Item(item.id),
                    actions: item.row_actions(),
                })
                .collect(),
        };
        ItemLists {
            lists: vec![
                list(PLAYER, "Carried", &self.player),
                list(CONTAINER, "Container", &self.container),
            ],
            back: Some(MenuAction::Back),
        }
    }

    /// Applies `action` to item `id` of list `list`, with `target` (the item id and list of a
    /// second row) when the action asked for one. Refuses, and changes nothing, when the rule
    /// does not hold; returns whether it applied.
    pub fn act(
        &mut self,
        action: ItemAction,
        id: u32,
        list: ListId,
        target: Option<(u32, ListId)>,
    ) -> bool {
        let offers = |items: &[Item]| {
            items
                .iter()
                .position(|item| item.id == id && item.action == Some(action))
        };
        match action {
            ItemAction::Use => {
                let Some(items) = self.items_mut(list) else {
                    return false;
                };
                let Some(at) = offers(items) else {
                    return false;
                };
                items[at].count -= 1;
                if items[at].count == 0 {
                    items.remove(at);
                }
                true
            }
            ItemAction::LoadFromStack => {
                if list != CONTAINER || target != Some((ROUNDS, PLAYER)) {
                    return false;
                }
                let (Some(magazine), Some(rounds)) = (
                    offers(&self.container),
                    self.player.iter().position(|item| item.id == ROUNDS),
                ) else {
                    return false;
                };
                let moved = (MAGAZINE_CAPACITY - self.container[magazine].count)
                    .min(self.player[rounds].count);
                if moved == 0 {
                    return false;
                }
                self.container[magazine].count += moved;
                self.player[rounds].count -= moved;
                if self.player[rounds].count == 0 {
                    self.player.remove(rounds);
                }
                true
            }
        }
    }

    fn items_mut(&mut self, list: ListId) -> Option<&mut Vec<Item>> {
        match list {
            PLAYER => Some(&mut self.player),
            CONTAINER => Some(&mut self.container),
            _ => None,
        }
    }

    /// Moves the whole stack of item `id` from list `from` to the end of list `to`. Refuses, and
    /// changes nothing, when the item is not in `from`, `from` is `to`, a list does not exist or
    /// the destination is full; returns whether it moved.
    pub fn move_item(&mut self, id: u32, from: ListId, to: ListId) -> bool {
        let has_room = match to {
            PLAYER => self.player.len() < PLAYER_ROWS,
            CONTAINER => true,
            _ => false,
        };
        if from == to || !has_room {
            return false;
        }
        let Some(source) = self.items_mut(from) else {
            return false;
        };
        let Some(at) = source.iter().position(|item| item.id == id) else {
            return false;
        };
        let item = source.remove(at);
        if let Some(target) = self.items_mut(to) {
            target.push(item);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use moho_ui_shell::ListRow;

    use super::*;

    fn id_of(row: &ListRow<MenuAction>) -> u32 {
        let MenuAction::Item(id) = row.item else {
            panic!("row {:?} does not carry an item id", row.label);
        };
        id
    }

    fn shown(items: &DemoItems, list: usize) -> Vec<(String, Option<u32>)> {
        let lists = items.lists();
        assert_eq!(lists.lists.len(), 2, "the player's list and the container");
        lists.lists[list]
            .rows
            .iter()
            .map(|row| (row.label.clone(), row.count))
            .collect()
    }

    fn id_named(items: &DemoItems, list: usize, label: &str) -> u32 {
        let lists = items.lists();
        assert_eq!(lists.lists.len(), 2, "the player's list and the container");
        lists.lists[list]
            .rows
            .iter()
            .find(|row| row.label == label)
            .map_or_else(|| panic!("no row {label:?} in list {list}"), id_of)
    }

    fn first_id(items: &DemoItems, list: usize) -> u32 {
        let lists = items.lists();
        assert_eq!(lists.lists.len(), 2, "the player's list and the container");
        id_of(&lists.lists[list].rows[0])
    }

    fn actions_of(items: &DemoItems, list: usize, label: &str) -> Vec<(String, MenuAction, bool)> {
        items.lists().lists[list]
            .rows
            .iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no row {label:?} in list {list}"))
            .actions
            .iter()
            .map(|action| {
                (
                    action.label.clone(),
                    action.action,
                    action.target == moho_ui_shell::ActionTarget::AnotherRow,
                )
            })
            .collect()
    }

    fn count_of(items: &DemoItems, list: usize, label: &str) -> Option<u32> {
        shown(items, list)
            .into_iter()
            .find(|(shown, _)| shown == label)
            .and_then(|(_, count)| count)
    }

    /// Demo items whose magazine holds `loaded` and whose player has `rounds` rounds.
    fn stocked(loaded: u32, rounds: u32) -> DemoItems {
        let mut items = DemoItems::new();
        items
            .container
            .iter_mut()
            .find(|item| item.label == "Magazine")
            .expect("the container holds a magazine")
            .count = loaded;
        items
            .player
            .iter_mut()
            .find(|item| item.label == "Rounds")
            .expect("the player has rounds")
            .count = rounds;
        items
    }

    #[test]
    fn the_bandage_offers_use_and_the_magazine_offers_load_from_stack_for_another_row() {
        let items = DemoItems::new();

        let bandage = actions_of(&items, 0, "Bandage");
        let magazine = actions_of(&items, 1, "Magazine");

        assert_eq!(bandage.len(), 1);
        assert_eq!(
            (bandage[0].1, bandage[0].2),
            (MenuAction::ItemAction(ItemAction::Use), false)
        );
        assert_eq!(
            magazine,
            [(
                "Load from stack".to_owned(),
                MenuAction::ItemAction(ItemAction::LoadFromStack),
                true
            )]
        );
        assert_eq!(actions_of(&items, 0, "Rounds"), []);
        assert_eq!(actions_of(&items, 1, "Rifle"), []);
        assert_eq!(count_of(&items, 1, "Magazine"), Some(0));
    }

    #[test]
    fn using_a_bandage_lowers_its_count_and_the_last_one_removes_the_row() {
        let mut items = DemoItems::new();
        let bandage = id_named(&items, 0, "Bandage");

        let first = items.act(ItemAction::Use, bandage, ListId(0), None);
        let second = items.act(ItemAction::Use, bandage, ListId(0), None);
        let after_two = count_of(&items, 0, "Bandage");
        let last = items.act(ItemAction::Use, bandage, ListId(0), None);
        let after_last = shown(&items, 0);
        let none_left = items.act(ItemAction::Use, bandage, ListId(0), None);

        assert!(first && second && last);
        assert_eq!(after_two, Some(1));
        assert_eq!(after_last, [("Rounds".to_owned(), Some(60))]);
        assert!(!none_left, "there is no bandage to use");
    }

    #[test]
    fn loading_from_anything_but_rounds_changes_nothing() {
        let mut items = DemoItems::new();
        let magazine = id_named(&items, 1, "Magazine");
        let bandage = id_named(&items, 0, "Bandage");
        let rifle = id_named(&items, 1, "Rifle");
        let rounds = id_named(&items, 0, "Rounds");
        let before = items.lists();
        let load = ItemAction::LoadFromStack;

        let refused = [
            items.act(load, magazine, ListId(1), Some((bandage, ListId(0)))),
            items.act(load, magazine, ListId(1), Some((rifle, ListId(1)))),
            items.act(load, magazine, ListId(1), Some((magazine, ListId(1)))),
            items.act(load, magazine, ListId(1), None),
            items.act(load, magazine, ListId(1), Some((u32::MAX, ListId(0)))),
            items.act(load, magazine, ListId(1), Some((rounds, ListId(1)))),
            items.act(load, magazine, ListId(0), Some((rounds, ListId(0)))),
            items.act(load, bandage, ListId(0), Some((rounds, ListId(0)))),
        ];
        let unchanged = items.lists();
        let accepted = items.act(load, magazine, ListId(1), Some((rounds, ListId(0))));

        assert_eq!(refused, [false; 8]);
        assert_eq!(unchanged, before);
        assert!(accepted, "the player's rounds are the valid target");
        assert_eq!(count_of(&items, 1, "Magazine"), Some(30));
    }

    #[test]
    fn the_magazine_stops_at_30_and_a_spent_stack_is_removed() {
        let load = |loaded, rounds| {
            let mut items = stocked(loaded, rounds);
            let magazine = id_named(&items, 1, "Magazine");
            let stack = id_named(&items, 0, "Rounds");
            let moved = items.act(
                ItemAction::LoadFromStack,
                magazine,
                ListId(1),
                Some((stack, ListId(0))),
            );
            (
                moved,
                count_of(&items, 1, "Magazine"),
                count_of(&items, 0, "Rounds"),
                shown(&items, 0).len(),
            )
        };

        let from_empty = load(0, 60);
        let topped_up = load(25, 60);
        let short_stack = load(0, 10);
        let exact_stack = load(0, 30);

        assert_eq!(from_empty, (true, Some(30), Some(30), 2));
        assert_eq!(topped_up, (true, Some(30), Some(55), 2));
        assert_eq!(short_stack, (true, Some(10), None, 1), "a spent stack goes");
        assert_eq!(exact_stack, (true, Some(30), None, 1));
    }

    #[test]
    fn the_player_starts_with_bandages_and_rounds_and_the_container_holds_a_rifle_and_filler() {
        let items = DemoItems::new();

        let player = shown(&items, 0);
        let container = shown(&items, 1);

        assert_eq!(
            player,
            [
                ("Bandage".to_owned(), Some(3)),
                ("Rounds".to_owned(), Some(60))
            ]
        );
        assert!(
            container.contains(&("Rifle".to_owned(), Some(1))),
            "{container:?}"
        );
        assert!(container.len() > 40, "{} container rows", container.len());
        assert_eq!(items.lists().back, Some(MenuAction::Back));
        let ids: Vec<ListId> = items.lists().lists.iter().map(|list| list.id).collect();
        assert_eq!(ids, [ListId(0), ListId(1)]);
    }

    #[test]
    fn every_row_has_a_distinct_id_and_a_description() {
        let lists = DemoItems::new().lists();

        let rows: Vec<_> = lists.lists.iter().flat_map(|list| &list.rows).collect();

        let ids: BTreeSet<u32> = rows.iter().map(|row| id_of(row)).collect();
        assert!(rows.len() > 40);
        assert_eq!(ids.len(), rows.len(), "ids are unique across both lists");
        for row in rows {
            let tip = row.tooltip.as_deref().unwrap_or_default();
            assert!(!tip.is_empty(), "{:?} has no description", row.label);
        }
    }

    #[test]
    fn a_move_takes_the_whole_stack_and_appends_it_to_the_target_list() {
        let mut items = DemoItems::new();
        let rifle = id_named(&items, 1, "Rifle");
        let rounds = id_named(&items, 0, "Rounds");

        let to_player = items.move_item(rifle, ListId(1), ListId(0));
        let player = shown(&items, 0);
        let container = shown(&items, 1);
        let to_container = items.move_item(rounds, ListId(0), ListId(1));

        assert!(to_player);
        assert_eq!(
            player,
            [
                ("Bandage".to_owned(), Some(3)),
                ("Rounds".to_owned(), Some(60)),
                ("Rifle".to_owned(), Some(1)),
            ]
        );
        assert!(!container.iter().any(|(label, _)| label == "Rifle"));
        assert!(to_container);
        assert_eq!(
            shown(&items, 1).last(),
            Some(&("Rounds".to_owned(), Some(60)))
        );
        assert_eq!(
            shown(&items, 0),
            [
                ("Bandage".to_owned(), Some(3)),
                ("Rifle".to_owned(), Some(1))
            ]
        );
    }

    #[test]
    fn a_move_into_a_full_player_list_changes_neither_list() {
        let mut items = DemoItems::new();
        for _ in 0..6 {
            let id = first_id(&items, 1);
            assert!(
                items.move_item(id, ListId(1), ListId(0)),
                "the player's list has room"
            );
        }
        let before = items.lists();
        let one_more = first_id(&items, 1);

        let moved = items.move_item(one_more, ListId(1), ListId(0));

        assert_eq!(before.lists[0].rows.len(), 8);
        assert!(!moved);
        assert_eq!(items.lists(), before);
    }

    #[test]
    fn moving_out_of_a_full_player_list_is_allowed() {
        let mut items = DemoItems::new();
        for _ in 0..6 {
            let id = first_id(&items, 1);
            assert!(items.move_item(id, ListId(1), ListId(0)));
        }
        let bandage = id_named(&items, 0, "Bandage");

        let moved = items.move_item(bandage, ListId(0), ListId(1));

        assert!(moved);
        assert_eq!(shown(&items, 0).len(), 7);
        assert_eq!(
            shown(&items, 1).last(),
            Some(&("Bandage".to_owned(), Some(3)))
        );
    }

    #[test]
    fn a_move_of_an_item_not_in_the_source_or_between_unknown_lists_changes_nothing() {
        let mut items = DemoItems::new();
        let before = items.lists();
        let rifle = id_named(&items, 1, "Rifle");
        let bandage = id_named(&items, 0, "Bandage");

        let wrong_source = items.move_item(rifle, ListId(0), ListId(1));
        let unknown_id = items.move_item(u32::MAX, ListId(1), ListId(0));
        let unknown_from = items.move_item(bandage, ListId(2), ListId(0));
        let unknown_to = items.move_item(bandage, ListId(0), ListId(2));

        assert!(!(wrong_source || unknown_id || unknown_from || unknown_to));
        assert_eq!(items.lists(), before);
        assert_eq!(before.lists.len(), 2);
    }

    #[test]
    fn a_move_to_the_same_list_or_of_an_unknown_item_or_list_changes_nothing() {
        let mut items = DemoItems::new();
        let before = items.lists();
        let bandage = id_named(&items, 0, "Bandage");
        let rifle = id_named(&items, 1, "Rifle");

        let same_player = items.move_item(bandage, ListId(0), ListId(0));
        let same_container = items.move_item(rifle, ListId(1), ListId(1));
        let unknown_item = items.move_item(u32::MAX, ListId(0), ListId(1));
        let unknown_list = items.move_item(bandage, ListId(0), ListId(7));
        let both_unknown = items.move_item(bandage, ListId(7), ListId(8));

        assert!(!(same_player || same_container || unknown_item || unknown_list || both_unknown));
        assert_eq!(items.lists(), before);
    }

    #[test]
    fn the_player_list_takes_an_eighth_row_and_refuses_a_ninth_and_the_container_never_refuses() {
        let mut items = DemoItems::new();
        for _ in 0..5 {
            let id = first_id(&items, 1);
            assert!(items.move_item(id, ListId(1), ListId(0)));
        }
        let seventh_full = shown(&items, 0).len();
        let id = first_id(&items, 1);
        let eighth = items.move_item(id, ListId(1), ListId(0));
        let after_eighth = items.lists();
        let id = first_id(&items, 1);
        let ninth = items.move_item(id, ListId(1), ListId(0));

        assert_eq!(seventh_full, 7);
        assert!(eighth);
        assert_eq!(after_eighth.lists[0].rows.len(), 8);
        assert!(!ninth);
        assert_eq!(items.lists(), after_eighth);
        let mut back = DemoItems::new();
        assert!(shown(&back, 1).len() > 40);
        for _ in 0..2 {
            let id = first_id(&back, 0);
            assert!(back.move_item(id, ListId(0), ListId(1)));
        }
        assert_eq!(shown(&back, 0), []);
        assert!(shown(&back, 1).len() > 40);
    }
}
