//! The demo contents of the item lists screen: the player's carried items and a test container,
//! and the game's rule for moving a row between them. Stands in until the inventory owns the
//! screen.

use moho_ui_shell::{ItemList, ItemLists, ListId, ListRow};

use crate::menu::MenuAction;

const PLAYER: ListId = ListId(0);
const CONTAINER: ListId = ListId(1);
/// The most rows the player's list holds.
const PLAYER_ROWS: usize = 8;
/// How many filler rows follow the rifle in the container.
const FILLER_ROWS: u32 = 45;

/// One stack of an item: its stable id, label, count and description.
#[derive(Clone, Debug)]
struct Item {
    id: u32,
    label: String,
    count: u32,
    description: String,
}

impl Item {
    fn new(id: u32, label: &str, count: u32, description: &str) -> Self {
        Self {
            id,
            label: label.to_owned(),
            count,
            description: description.to_owned(),
        }
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
        let mut container = vec![Item::new(10, "Rifle", 1, "Semi-auto.\nNeeds rounds.")];
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
                Item::new(1, "Bandage", 3, "Clean cloth.\nStops bleeding."),
                Item::new(2, "Rounds", 60, "Loose rifle rounds."),
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

    fn items_mut(&mut self, list: ListId) -> Option<&mut Vec<Item>> {
        match list {
            PLAYER => Some(&mut self.player),
            CONTAINER => Some(&mut self.container),
            _ => None,
        }
    }

    /// Moves the whole stack of item `id` from list `from` to the end of list `to`. Refuses, and
    /// changes nothing, when the item is not in `from`, a list does not exist or the player's
    /// list would pass 8 rows; returns whether it moved. The player's list is `ListId(0)` and
    /// the container `ListId(1)`.
    pub fn move_item(&mut self, id: u32, from: ListId, to: ListId) -> bool {
        if from == to || self.items_mut(to).is_none() {
            return false;
        }
        if to == PLAYER && self.player.len() >= PLAYER_ROWS {
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
}
