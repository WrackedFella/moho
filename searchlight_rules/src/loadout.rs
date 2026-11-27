//! One player's carried state: the rifle in hand, the rig that feeds it, the backpack worn and
//! the inventory of everything else.

use crate::inventory::{Item, MoveRefused, Place, Remains, Source};
use crate::item::{ItemId, SlotKind};
use crate::magazine::Magazine;
use crate::rifle::{Rifle, Shot};
use crate::rig::Rig;

/// What a move's source holds.
enum Held {
    /// A magazine, taking a slot of this kind.
    Magazine(SlotKind),
    /// Anything else.
    Other,
}

/// A rifle, the rig it reloads from, the backpack worn and the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loadout {
    rifle: Rifle,
    rig: Rig,
    backpack: Option<ItemId>,
    inventory: Vec<Item>,
}

impl Loadout {
    /// A loadout of `rifle` and `rig`, with no backpack and an empty inventory.
    #[must_use]
    pub fn new(rifle: Rifle, rig: Rig) -> Self {
        Self {
            rifle,
            rig,
            backpack: None,
            inventory: Vec::new(),
        }
    }

    /// This loadout wearing the backpack item `backpack`.
    #[must_use]
    pub fn with_backpack(mut self, backpack: ItemId) -> Self {
        self.backpack = Some(backpack);
        self
    }

    /// This loadout with `inventory` as its inventory.
    #[must_use]
    pub fn with_inventory(mut self, inventory: Vec<Item>) -> Self {
        self.inventory = inventory;
        self
    }

    /// The backpack item being worn.
    #[must_use]
    pub fn backpack(&self) -> Option<&ItemId> {
        self.backpack.as_ref()
    }

    /// Everything carried that is not held, worn, or in a magazine slot, in order.
    #[must_use]
    pub fn inventory(&self) -> &[Item] {
        &self.inventory
    }

    /// Moves one item from `from` to `to`, `body` being the dead enemy being looted, if any.
    ///
    /// Nothing refuses a move for weight or room: the inventory and the body take any number
    /// of items.
    ///
    /// # Errors
    ///
    /// A [`MoveRefused`] when the source holds nothing, a body is named but not given, a
    /// non-magazine is bound for the magazine slots, or no slot is free. A refused move
    /// changes nothing.
    pub fn move_item(
        &mut self,
        body: Option<&mut Remains>,
        from: Source,
        to: Place,
    ) -> Result<(), MoveRefused> {
        let mut body = body;
        if (to == Place::Body || from.on_body()) && body.is_none() {
            return Err(MoveRefused::NoBody);
        }
        let held = self
            .held(body.as_deref(), from)
            .ok_or(MoveRefused::NoSuchItem)?;
        if to == Place::MagazineSlots {
            let Held::Magazine(kind) = held else {
                return Err(MoveRefused::NotAMagazine);
            };
            let leaves_a_fitting_slot = matches!(from, Source::MagazineSlot(slot) if self.rig.slot_kind(slot) == Some(&kind));
            if !leaves_a_fitting_slot && !self.rig.has_free_slot_for(&kind) {
                return Err(MoveRefused::NoFreeSlot);
            }
        }

        let item = self
            .take(body.as_deref_mut(), from)
            .expect("the source was checked to hold an item");
        match (to, item) {
            (Place::Inventory, item) => self.inventory.push(item),
            (Place::Body, item) => body
                .expect("a body place was checked to have a body")
                .carried
                .push(item),
            (Place::MagazineSlots, Item::Magazine(magazine)) => {
                self.rig
                    .stow(magazine)
                    .expect("a free slot was checked for");
            }
            (Place::MagazineSlots, _) => unreachable!("only a magazine was let through"),
        }
        Ok(())
    }

    /// What `from` holds, if anything.
    fn held(&self, body: Option<&Remains>, from: Source) -> Option<Held> {
        let in_slot = |slots: &[Option<Magazine>], index: usize| {
            slots
                .get(index)
                .and_then(Option::as_ref)
                .map(|magazine| Held::Magazine(magazine.slot().clone()))
        };
        let kind_of = |item: &Item| match item {
            Item::Magazine(magazine) => Held::Magazine(magazine.slot().clone()),
            _ => Held::Other,
        };
        match from {
            Source::Inventory(index) => self.inventory.get(index).map(kind_of),
            Source::MagazineSlot(index) => in_slot(self.rig.slots(), index),
            Source::BodyCarried(index) => body?.carried.get(index).map(kind_of),
            Source::BodyMagazineSlot(index) => in_slot(body?.rig.as_ref()?.slots(), index),
            Source::BodyRig => body?.rig.as_ref().map(|_| Held::Other),
            Source::BodyBackpack => body?.backpack.as_ref().map(|_| Held::Other),
        }
    }

    /// Removes and returns what `from` holds. Call it only for a source
    /// [`Self::held`] found an item in: an index past the end of a list panics.
    /// Taking a body's rig yields the rig alone; its magazines join the body's carried items.
    fn take(&mut self, body: Option<&mut Remains>, from: Source) -> Option<Item> {
        match from {
            Source::Inventory(index) => Some(self.inventory.remove(index)),
            Source::MagazineSlot(index) => self.rig.take_slot(index).map(Item::Magazine),
            Source::BodyCarried(index) => Some(body?.carried.remove(index)),
            Source::BodyMagazineSlot(index) => {
                body?.rig.as_mut()?.take_slot(index).map(Item::Magazine)
            }
            Source::BodyRig => {
                let body = body?;
                let mut rig = body.rig.take()?;
                for slot in 0..rig.slots().len() {
                    if let Some(magazine) = rig.take_slot(slot) {
                        body.carried.push(Item::Magazine(magazine));
                    }
                }
                Some(Item::Rig(rig.item().clone()))
            }
            Source::BodyBackpack => body?.backpack.take().map(Item::Other),
        }
    }

    /// The rifle in hand.
    #[must_use]
    pub fn rifle(&self) -> &Rifle {
        &self.rifle
    }

    /// The rig being worn.
    #[must_use]
    pub fn rig(&self) -> &Rig {
        &self.rig
    }

    /// Pulls the rifle's trigger.
    pub fn fire(&mut self) -> Shot {
        self.rifle.fire()
    }

    /// Whether the rig holds a magazine that fits the rifle.
    #[must_use]
    pub fn can_reload(&self) -> bool {
        self.rig.magazines().any(|m| self.rifle.fits(m))
    }

    /// Takes the next fitting magazine from the rig and puts the ejected one in the slot
    /// that frees. Returns `false`, changing nothing, when no magazine fits.
    pub fn reload(&mut self) -> bool {
        let rifle = &mut self.rifle;
        let Some((slot, spare)) = self.rig.take_next(|m| rifle.fits(m)) else {
            return false;
        };
        let Ok(ejected) = rifle.reload(spare) else {
            unreachable!("take_next only yields magazines that fit the rifle");
        };
        self.rig.put(slot, ejected);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::RoundStack;
    use crate::item::ItemId;
    use crate::magazine::Magazine;
    use crate::rifle::{Rifle, Shot};
    use crate::rig::Rig;
    use crate::test_support::{id, mag_of, rifle_well};
    use proptest::prelude::*;

    fn rifle_with(magazine: Magazine) -> Rifle {
        Rifle::loaded(rifle_well(), magazine).expect("the test magazine fits the rifle")
    }

    /// A loadout with `magazine` in the rifle and one rig slot per `layout` entry.
    fn loadout(magazine: Magazine, layout: &[Option<Magazine>]) -> Loadout {
        let mut rig = Rig::new(
            id("rig"),
            &crate::test_support::rig_spec(&[("rifle", u32::try_from(layout.len()).unwrap())]),
        );
        for (slot, spare) in layout.iter().enumerate() {
            if let Some(spare) = spare {
                rig.put(slot, spare.clone());
            }
        }
        Loadout::new(rifle_with(magazine), rig)
    }

    fn full(item: &str) -> Magazine {
        mag_of(item, "rifle", 30, 30)
    }

    fn slot_items(loadout: &Loadout) -> Vec<Option<String>> {
        loadout
            .rig()
            .slots()
            .iter()
            .map(|slot| slot.as_ref().map(|m| m.item().as_str().to_owned()))
            .collect()
    }

    fn rifle_item(loadout: &Loadout) -> String {
        loadout.rifle().magazine().item().as_str().to_owned()
    }

    #[test]
    fn reload_takes_the_next_magazine_and_stows_the_ejected_one_in_its_slot() {
        let mut loadout = loadout(
            mag_of("spent", "rifle", 30, 10),
            &[Some(full("fresh_a")), Some(full("fresh_b"))],
        );

        let reloaded = loadout.reload();

        assert!(reloaded);
        assert_eq!(loadout.rifle().magazine(), &full("fresh_a"));
        assert_eq!(
            loadout.rig().slots(),
            [Some(mag_of("spent", "rifle", 30, 9)), Some(full("fresh_b"))]
        );
    }

    #[test]
    fn reload_leaves_the_empty_magazine_on_the_rig_with_its_rounds_intact() {
        let mut loadout = loadout(mag_of("spent", "rifle", 30, 0), &[Some(full("fresh"))]);

        loadout.reload();

        let on_rig: Vec<&Magazine> = loadout.rig().magazines().collect();
        assert_eq!(on_rig, [&mag_of("spent", "rifle", 30, 0)]);
        assert_eq!(loadout.rifle().magazine().rounds(), 29);
        assert!(loadout.rifle().is_chambered());
    }

    #[test]
    fn reload_with_an_empty_rig_changes_nothing() {
        let layouts: [Vec<Option<Magazine>>; 3] = [
            vec![None, None, None],
            vec![],
            vec![Some(mag_of("pistol_mag", "pistol", 15, 15))],
        ];
        for layout in layouts {
            let mut loadout = loadout(mag_of("spent", "rifle", 30, 10), &layout);
            let before = loadout.clone();

            assert!(!loadout.can_reload(), "{layout:?}");
            let reloaded = loadout.reload();

            assert!(!reloaded, "{layout:?}");
            assert_eq!(loadout, before, "{layout:?}");
        }
    }

    #[test]
    fn can_reload_is_true_when_a_fitting_magazine_is_on_the_rig() {
        let loadout = loadout(mag_of("spent", "rifle", 30, 10), &[None, Some(full("a"))]);

        assert!(loadout.can_reload());
    }

    #[test]
    fn reload_skips_a_magazine_that_does_not_fit() {
        let wrong = mag_of("pistol_mag", "pistol", 15, 15);
        let mut loadout = loadout(
            mag_of("spent", "rifle", 30, 10),
            &[Some(wrong.clone()), Some(full("fresh"))],
        );

        assert!(loadout.can_reload());
        let reloaded = loadout.reload();

        assert!(reloaded);
        assert_eq!(rifle_item(&loadout), "fresh");
        assert_eq!(
            loadout.rig().slots(),
            [Some(wrong), Some(mag_of("spent", "rifle", 30, 9))]
        );
    }

    #[test]
    fn reload_cycles_slots_and_never_swaps_back() {
        let mut loadout = loadout(
            full("r"),
            &[Some(full("a")), Some(full("b")), Some(full("c"))],
        );

        let mut in_rifle = Vec::new();
        for _ in 0..4 {
            assert!(loadout.reload());
            in_rifle.push(rifle_item(&loadout));
        }

        assert_eq!(in_rifle, ["a", "b", "c", "r"]);
        assert_eq!(
            slot_items(&loadout),
            [
                Some("c".to_owned()),
                Some("a".to_owned()),
                Some("b".to_owned())
            ]
        );
    }

    #[test]
    fn reload_passes_over_a_free_slot_to_the_next_magazine() {
        let mut loadout = loadout(full("r"), &[Some(full("a")), None, Some(full("c"))]);

        loadout.reload();
        loadout.reload();

        assert_eq!(rifle_item(&loadout), "c");
        assert_eq!(
            slot_items(&loadout),
            [Some("r".to_owned()), None, Some("a".to_owned())]
        );
    }

    #[test]
    fn a_lone_magazine_on_the_rig_is_the_only_one_to_swap_to() {
        let mut loadout = loadout(full("r"), &[Some(full("a")), None, None]);

        loadout.reload();
        loadout.reload();

        assert_eq!(rifle_item(&loadout), "r");
        assert_eq!(slot_items(&loadout), [Some("a".to_owned()), None, None]);
    }

    #[test]
    fn fire_spends_the_chambered_round_and_runs_dry() {
        let mut loadout = loadout(mag_of("m", "rifle", 30, 2), &[]);

        let shots: Vec<Shot> = (0..3).map(|_| loadout.fire()).collect();

        assert_eq!(
            shots,
            [Shot::Fired(id("ball")), Shot::Fired(id("ball")), Shot::Dry]
        );
        assert_eq!(loadout.rifle().magazine().rounds(), 0);
    }

    fn total_rounds(loadout: &Loadout) -> u32 {
        loadout.rifle().magazine().rounds()
            + u32::from(loadout.rifle().is_chambered())
            + loadout.rig().magazines().map(Magazine::rounds).sum::<u32>()
    }

    fn all_items(loadout: &Loadout) -> Vec<ItemId> {
        let mut items: Vec<ItemId> = loadout
            .rig()
            .magazines()
            .map(|m| m.item().clone())
            .chain([loadout.rifle().magazine().item().clone()])
            .collect();
        items.sort();
        items
    }

    proptest! {
        #[test]
        fn reload_never_creates_or_loses_rounds_or_magazines(
            rifle_rounds in 0u32..=10,
            layout in proptest::collection::vec(
                proptest::option::of((any::<bool>(), 0u32..=10)),
                0..6,
            ),
            reloads in 0usize..20,
            shots in 0usize..15,
        ) {
            let slots: Vec<Option<Magazine>> = layout
                .iter()
                .enumerate()
                .map(|(i, slot)| slot.map(|(fits, rounds)| {
                    mag_of(&format!("spare_{i}"), if fits { "rifle" } else { "pistol" }, 10, rounds)
                }))
                .collect();
            let mut loadout = loadout(mag_of("held", "rifle", 10, rifle_rounds), &slots);
            for _ in 0..shots {
                loadout.fire();
            }
            let rounds_before = total_rounds(&loadout);
            let items_before = all_items(&loadout);
            let slot_count = loadout.rig().slots().len();

            for _ in 0..reloads {
                let expected = loadout.can_reload();
                prop_assert_eq!(loadout.reload(), expected);
            }

            prop_assert_eq!(total_rounds(&loadout), rounds_before);
            prop_assert_eq!(all_items(&loadout), items_before);
            prop_assert_eq!(loadout.rig().slots().len(), slot_count);
        }
    }

    fn bandage() -> Item {
        Item::Other(id("bandage"))
    }

    fn mag_item(item: &str) -> Item {
        Item::Magazine(full(item))
    }

    /// A rig of `layout.len()` slots with a full magazine named by each `Some` entry.
    fn rig_holding(item: &str, layout: &[Option<&str>]) -> Rig {
        let mut rig = Rig::new(
            id(item),
            &crate::test_support::rig_spec(&[("rifle", u32::try_from(layout.len()).unwrap())]),
        );
        for (slot, magazine) in layout.iter().enumerate() {
            if let Some(magazine) = magazine {
                rig.put(slot, full(magazine));
            }
        }
        rig
    }

    /// A player with `slots` on the rig and `inventory` in the pack, wearing `backpack`.
    fn player(slots: &[Option<&str>], inventory: Vec<Item>) -> Loadout {
        let layout: Vec<Option<Magazine>> = slots.iter().map(|s| s.map(full)).collect();
        loadout(mag_of("held", "rifle", 30, 10), &layout)
            .with_backpack(id("pack"))
            .with_inventory(inventory)
    }

    fn body(rig: Option<Rig>, carried: Vec<Item>) -> Remains {
        Remains {
            rig,
            backpack: None,
            carried,
        }
    }

    #[test]
    fn taking_a_magazine_from_a_body_puts_it_in_the_inventory_and_off_the_body() {
        let mut loadout = player(&[None], vec![bandage()]);
        let mut remains = body(
            None,
            vec![bandage(), mag_item("loot"), Item::Other(id("rag"))],
        );

        let moved = loadout.move_item(Some(&mut remains), Source::BodyCarried(1), Place::Inventory);

        assert_eq!(moved, Ok(()));
        assert_eq!(loadout.inventory(), [bandage(), mag_item("loot")]);
        assert_eq!(remains.carried, [bandage(), Item::Other(id("rag"))]);
    }

    fn inventory_item(kind: u8, n: usize) -> Item {
        let item = format!("i{n}");
        let partial = 1 + u32::try_from(n % 29).unwrap();
        match kind {
            0 => Item::Magazine(mag_of(&item, "rifle", 30, partial)),
            1 => Item::Other(id(&item)),
            2 => Item::Rounds(RoundStack {
                round: id(&item),
                count: 5 + u32::try_from(n).unwrap(),
            }),
            3 => Item::Weapon(rifle_with(mag_of(
                &format!("{item}_in"),
                "rifle",
                30,
                partial,
            ))),
            _ => Item::Rig(id(&item)),
        }
    }

    proptest! {
        #[test]
        fn a_take_from_a_body_is_accepted_at_any_carried_weight(
            kinds in proptest::collection::vec(0u8..5, 0..60),
            slots_full in any::<bool>(),
        ) {
            let carried: Vec<Item> = kinds
                .iter()
                .enumerate()
                .map(|(n, kind)| inventory_item(*kind, n))
                .collect();
            let slots: &[Option<&str>] = if slots_full {
                &[Some("a"), Some("b")]
            } else {
                &[None, None]
            };
            let mut loadout = player(slots, carried.clone());
            let heavy = Item::Other(id("two_kg_item"));
            let mut remains = body(None, vec![heavy.clone()]);

            let moved =
                loadout.move_item(Some(&mut remains), Source::BodyCarried(0), Place::Inventory);

            prop_assert_eq!(moved, Ok(()));
            prop_assert_eq!(loadout.inventory().len(), carried.len() + 1);
            prop_assert_eq!(loadout.inventory().last(), Some(&heavy));
            prop_assert_eq!(&loadout.inventory()[..carried.len()], carried.as_slice());
            prop_assert!(remains.carried.is_empty());
        }
    }

    #[test]
    fn a_magazine_moves_from_the_inventory_to_a_free_slot() {
        let mut loadout = player(
            &[Some("a"), None, Some("b")],
            vec![bandage(), mag_item("m")],
        );

        let moved = loadout.move_item(None, Source::Inventory(1), Place::MagazineSlots);

        assert_eq!(moved, Ok(()));
        assert_eq!(
            slot_items(&loadout),
            [
                Some("a".to_owned()),
                Some("m".to_owned()),
                Some("b".to_owned())
            ]
        );
        assert_eq!(loadout.inventory(), [bandage()]);
    }

    #[test]
    fn a_magazine_goes_to_the_first_of_several_free_slots() {
        let mut loadout = player(&[None, None, Some("b")], vec![mag_item("m")]);

        loadout
            .move_item(None, Source::Inventory(0), Place::MagazineSlots)
            .expect("a slot is free");

        assert_eq!(
            slot_items(&loadout),
            [Some("m".to_owned()), None, Some("b".to_owned())]
        );
        assert_eq!(loadout.inventory(), []);
    }

    #[test]
    fn reload_with_empty_slots_and_a_full_magazine_in_the_inventory_does_nothing() {
        let mut loadout = player(&[None, None], vec![mag_item("spare")]);
        let before = loadout.clone();

        assert!(!loadout.can_reload());
        let reloaded = loadout.reload();

        assert!(!reloaded);
        assert_eq!(loadout, before);
    }

    #[test]
    fn a_bandage_to_the_slots_is_refused_and_changes_nothing() {
        let others = [
            bandage(),
            Item::Weapon(rifle_with(full("in_rifle"))),
            Item::Rounds(RoundStack {
                round: id("ball"),
                count: 30,
            }),
            Item::Rig(id("spare_rig")),
        ];
        for item in others {
            let mut loadout = player(&[None, Some("a")], vec![item.clone()]);
            let before = loadout.clone();

            let moved = loadout.move_item(None, Source::Inventory(0), Place::MagazineSlots);

            assert_eq!(moved, Err(MoveRefused::NotAMagazine), "{item:?}");
            assert_eq!(loadout, before, "{item:?}");
        }
        let mut loadout = player(&[None], vec![]);
        let mut remains = body(Some(rig_holding("body_rig", &[Some("x")])), vec![]);
        let (before, remains_before) = (loadout.clone(), remains.clone());

        let moved = loadout.move_item(Some(&mut remains), Source::BodyRig, Place::MagazineSlots);

        assert_eq!(moved, Err(MoveRefused::NotAMagazine), "a body's rig");
        assert_eq!(loadout, before);
        assert_eq!(remains, remains_before);
    }

    #[test]
    fn a_magazine_to_full_slots_is_refused_and_changes_nothing() {
        for slots in [&[Some("a"), Some("b")][..], &[]] {
            let mut loadout = player(slots, vec![mag_item("m")]);
            let before = loadout.clone();

            let moved = loadout.move_item(None, Source::Inventory(0), Place::MagazineSlots);

            assert_eq!(moved, Err(MoveRefused::NoFreeSlot), "{slots:?}");
            assert_eq!(loadout, before, "{slots:?}");
        }
    }

    #[test]
    fn a_magazine_from_a_slot_goes_to_the_end_of_the_inventory() {
        let mut loadout = player(&[Some("a"), Some("b")], vec![bandage()]);

        loadout
            .move_item(None, Source::MagazineSlot(0), Place::Inventory)
            .expect("the slot holds a magazine");

        assert_eq!(slot_items(&loadout), [None, Some("b".to_owned())]);
        assert_eq!(loadout.inventory(), [bandage(), mag_item("a")]);
    }

    #[test]
    fn a_source_holding_nothing_is_refused_and_changes_nothing() {
        let sources = [
            Source::Inventory(1),
            Source::MagazineSlot(1),
            Source::MagazineSlot(9),
            Source::BodyCarried(1),
            Source::BodyMagazineSlot(0),
            Source::BodyMagazineSlot(9),
            Source::BodyRig,
            Source::BodyBackpack,
        ];
        for source in sources {
            for place in [Place::Inventory, Place::MagazineSlots, Place::Body] {
                let mut loadout = player(&[Some("a"), None], vec![bandage()]);
                let mut remains = body(None, vec![bandage()]);
                let (loadout_before, remains_before) = (loadout.clone(), remains.clone());

                let moved = loadout.move_item(Some(&mut remains), source, place);

                assert_eq!(
                    moved,
                    Err(MoveRefused::NoSuchItem),
                    "{source:?} to {place:?}"
                );
                assert_eq!(loadout, loadout_before, "{source:?} to {place:?}");
                assert_eq!(remains, remains_before, "{source:?} to {place:?}");
            }
        }
    }

    #[test]
    fn an_empty_slot_of_a_bodys_rig_holds_nothing_to_move() {
        let rig = rig_holding("body_rig", &[Some("x"), None, Some("y")]);
        let mut loadout = player(&[None], vec![bandage()]);
        let mut remains = body(Some(rig), vec![]);
        let (loadout_before, remains_before) = (loadout.clone(), remains.clone());

        let moved = loadout.move_item(
            Some(&mut remains),
            Source::BodyMagazineSlot(1),
            Place::Inventory,
        );

        assert_eq!(moved, Err(MoveRefused::NoSuchItem));
        assert_eq!(loadout, loadout_before);
        assert_eq!(remains, remains_before);
    }

    #[test]
    fn a_body_source_or_place_with_no_body_is_refused_and_changes_nothing() {
        let moves = [
            (Source::BodyCarried(0), Place::Inventory),
            (Source::BodyMagazineSlot(0), Place::MagazineSlots),
            (Source::BodyRig, Place::Inventory),
            (Source::BodyBackpack, Place::Inventory),
            (Source::Inventory(0), Place::Body),
        ];
        for (from, to) in moves {
            let mut loadout = player(&[None], vec![bandage()]);
            let before = loadout.clone();

            let moved = loadout.move_item(None, from, to);

            assert_eq!(moved, Err(MoveRefused::NoBody), "{from:?} to {to:?}");
            assert_eq!(loadout, before, "{from:?} to {to:?}");
        }
    }

    #[test]
    fn an_inventory_item_moves_to_the_end_of_the_body_s_carried_items() {
        let mut loadout = player(&[None], vec![bandage(), mag_item("m")]);
        let mut remains = body(None, vec![Item::Other(id("rag"))]);

        let moved = loadout.move_item(Some(&mut remains), Source::Inventory(0), Place::Body);

        assert_eq!(moved, Ok(()));
        assert_eq!(loadout.inventory(), [mag_item("m")]);
        assert_eq!(remains.carried, [Item::Other(id("rag")), bandage()]);
    }

    #[test]
    fn a_magazine_in_the_bodys_rig_goes_to_the_inventory_or_a_free_slot() {
        let rig = rig_holding("body_rig", &[Some("x"), None, Some("y")]);
        let mut loadout = player(&[None], vec![]);
        let mut remains = body(Some(rig), vec![]);

        let to_pack = loadout.move_item(
            Some(&mut remains),
            Source::BodyMagazineSlot(2),
            Place::Inventory,
        );
        let to_slot = loadout.move_item(
            Some(&mut remains),
            Source::BodyMagazineSlot(0),
            Place::MagazineSlots,
        );

        assert_eq!((to_pack, to_slot), (Ok(()), Ok(())));
        assert_eq!(loadout.inventory(), [mag_item("y")]);
        assert_eq!(slot_items(&loadout), [Some("x".to_owned())]);
        let left: Vec<&Magazine> = remains.rig.as_ref().unwrap().magazines().collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn taking_a_bodys_backpack_puts_its_item_in_the_inventory() {
        let mut loadout = player(&[None], vec![]);
        let mut remains = Remains {
            backpack: Some(id("backpack_small")),
            ..Remains::default()
        };

        let moved = loadout.move_item(Some(&mut remains), Source::BodyBackpack, Place::Inventory);

        assert_eq!(moved, Ok(()));
        assert_eq!(loadout.inventory(), [Item::Other(id("backpack_small"))]);
        assert_eq!(remains.backpack, None);
        assert_eq!(loadout.backpack(), Some(&id("pack")));
    }

    #[test]
    fn taking_a_bodys_rig_leaves_its_magazines_on_the_body() {
        let rig = rig_holding("body_rig", &[Some("x"), None, Some("y")]);
        let mut loadout = player(&[None], vec![]);
        let mut remains = body(Some(rig), vec![bandage()]);

        let taken = loadout.move_item(Some(&mut remains), Source::BodyRig, Place::Inventory);

        assert_eq!(taken, Ok(()));
        assert_eq!(loadout.inventory(), [Item::Rig(id("body_rig"))]);
        assert_eq!(remains.rig, None);
        assert_eq!(
            remains.carried,
            [bandage(), mag_item("x"), mag_item("y")],
            "the magazines stay on the body, after what it carried"
        );
        let carried = loadout.move_item(Some(&mut remains), Source::Inventory(0), Place::Body);
        assert_eq!(carried, Ok(()));
        assert_eq!(
            remains.carried,
            [
                bandage(),
                mag_item("x"),
                mag_item("y"),
                Item::Rig(id("body_rig"))
            ]
        );
        assert_eq!(loadout.inventory(), []);
    }

    /// Every leaf item the player and the body hold, wherever it sits, as sorted labels.
    fn leaves(loadout: &Loadout, remains: &Remains) -> Vec<String> {
        fn item(out: &mut Vec<String>, item: &Item) {
            match item {
                Item::Weapon(rifle) => out.push(format!(
                    "mag {} {}",
                    rifle.magazine().item(),
                    rifle.magazine().rounds()
                )),
                Item::Magazine(m) => out.push(format!("mag {} {}", m.item(), m.rounds())),
                Item::Rounds(stack) => out.push(format!("rounds {} {}", stack.round, stack.count)),
                Item::Rig(rig) => out.push(format!("rig {rig}")),
                Item::Other(id) => out.push(format!("other {id}")),
            }
        }
        fn rig_leaves(out: &mut Vec<String>, rig: &Rig) {
            out.push(format!("rig {}", rig.item()));
            out.extend(
                rig.magazines()
                    .map(|m| format!("mag {} {}", m.item(), m.rounds())),
            );
        }

        let held = loadout.rifle().magazine();
        let mut out = vec![format!("mag {} {}", held.item(), held.rounds())];
        rig_leaves(&mut out, loadout.rig());
        out.extend(loadout.backpack().map(|b| format!("other {b}")));
        loadout.inventory().iter().for_each(|i| item(&mut out, i));
        if let Some(rig) = &remains.rig {
            rig_leaves(&mut out, rig);
        }
        out.extend(remains.backpack.iter().map(|b| format!("other {b}")));
        remains.carried.iter().for_each(|i| item(&mut out, i));
        out.sort();
        out
    }

    fn source_of(kind: u8, index: usize) -> Source {
        match kind {
            0 => Source::Inventory(index),
            1 => Source::MagazineSlot(index),
            2 => Source::BodyCarried(index),
            3 => Source::BodyMagazineSlot(index),
            4 => Source::BodyRig,
            _ => Source::BodyBackpack,
        }
    }

    fn place_of(kind: u8) -> Place {
        match kind {
            0 => Place::Inventory,
            1 => Place::MagazineSlots,
            _ => Place::Body,
        }
    }

    proptest! {
        #[test]
        fn any_move_sequence_keeps_every_item_exactly_once(
            kinds in proptest::collection::vec(0u8..5, 0..5),
            body_kinds in proptest::collection::vec(0u8..5, 0..5),
            slots in proptest::collection::vec(any::<bool>(), 0..4),
            body_slots in proptest::collection::vec(any::<bool>(), 0..4),
            body_has_rig in any::<bool>(),
            body_has_backpack in any::<bool>(),
            with_body in proptest::collection::vec(any::<bool>(), 12),
            moves in proptest::collection::vec((0u8..6, 0usize..6, 0u8..3), 0..12),
        ) {
            let names: Vec<String> = (0..slots.len()).map(|n| format!("s{n}")).collect();
            let layout: Vec<Option<&str>> = slots
                .iter()
                .zip(&names)
                .map(|(held, name)| held.then_some(name.as_str()))
                .collect();
            let inventory = kinds.iter().enumerate().map(|(n, k)| inventory_item(*k, n)).collect();
            let mut loadout = player(&layout, inventory);
            let body_names: Vec<String> = (0..body_slots.len()).map(|n| format!("b{n}")).collect();
            let body_layout: Vec<Option<&str>> = body_slots
                .iter()
                .zip(&body_names)
                .map(|(held, name)| held.then_some(name.as_str()))
                .collect();
            let mut remains = Remains {
                rig: body_has_rig.then(|| rig_holding("body_rig", &body_layout)),
                backpack: body_has_backpack.then(|| id("body_pack")),
                carried: body_kinds
                    .iter()
                    .enumerate()
                    .map(|(n, k)| inventory_item(*k, 100 + n))
                    .collect(),
            };
            let expected = leaves(&loadout, &remains);

            for ((kind, index, place), has_body) in moves.into_iter().zip(with_body) {
                let (loadout_before, remains_before) = (loadout.clone(), remains.clone());
                let result = loadout.move_item(
                    has_body.then_some(&mut remains),
                    source_of(kind, index),
                    place_of(place),
                );

                if result.is_err() {
                    prop_assert_eq!(&loadout, &loadout_before);
                    prop_assert_eq!(&remains, &remains_before);
                }
                prop_assert_eq!(leaves(&loadout, &remains), expected.clone());
            }
        }
    }

    /// A loadout holding a rifle magazine in the rifle, with a rig of `slots` by kind and
    /// `inventory` in the pack.
    fn kinded_player(slots: &[(&str, u32)], inventory: Vec<Item>) -> Loadout {
        use crate::test_support::{mag_in_slot, rig_spec};

        let rig = Rig::new(id("rig"), &rig_spec(slots));
        Loadout::new(rifle_with(mag_in_slot("held", "rifle", "rifle")), rig)
            .with_inventory(inventory)
    }

    #[test]
    fn a_rifle_magazine_with_only_a_pistol_slot_free_is_refused_and_unchanged() {
        let rifle_mag = Item::Magazine(crate::test_support::mag_in_slot("m", "rifle", "rifle"));
        let mut loadout = kinded_player(&[("pistol", 1)], vec![rifle_mag]);
        let before = loadout.clone();

        let moved = loadout.move_item(None, Source::Inventory(0), Place::MagazineSlots);

        assert_eq!(moved, Err(MoveRefused::NoFreeSlot));
        assert_eq!(loadout, before);
    }

    #[test]
    fn a_magazine_goes_to_a_free_slot_of_its_kind_past_a_free_slot_of_another() {
        let pistol_mag = Item::Magazine(crate::test_support::mag_in_slot("p", "pistol", "pistol"));
        let rifle_mag = Item::Magazine(crate::test_support::mag_in_slot("r", "rifle", "rifle"));
        let mut loadout =
            kinded_player(&[("pistol", 1), ("rifle", 1)], vec![rifle_mag, pistol_mag]);

        let rifle_moved = loadout.move_item(None, Source::Inventory(0), Place::MagazineSlots);
        let pistol_moved = loadout.move_item(None, Source::Inventory(0), Place::MagazineSlots);

        assert_eq!(rifle_moved, Ok(()));
        assert_eq!(pistol_moved, Ok(()));
        assert_eq!(
            slot_items(&loadout),
            [Some("p".to_owned()), Some("r".to_owned())],
            "the pistol slot is first, and the rifle magazine did not take it"
        );
        assert_eq!(loadout.inventory(), []);
    }

    #[test]
    fn a_rifle_magazine_from_a_rifle_slot_goes_back_to_it_when_only_a_pistol_slot_is_otherwise_free()
     {
        let mut loadout = kinded_player(&[("pistol", 1), ("rifle", 1)], vec![]);
        loadout
            .rig
            .stow(crate::test_support::mag_in_slot("r", "rifle", "rifle"))
            .expect("the rifle slot is free");
        let before = loadout.clone();

        let moved = loadout.move_item(None, Source::MagazineSlot(1), Place::MagazineSlots);

        assert_eq!(moved, Ok(()));
        assert_eq!(loadout, before);
    }

    #[test]
    fn a_rifle_magazine_in_a_pistol_slot_cannot_move_to_a_pistol_only_free_slot() {
        let mut loadout = kinded_player(&[("pistol", 1), ("rifle", 1)], vec![]);
        let rifle = |name| crate::test_support::mag_in_slot(name, "rifle", "rifle");
        loadout.rig.put(0, rifle("odd"));
        loadout.rig.put(1, rifle("r"));
        let before = loadout.clone();

        let moved = loadout.move_item(None, Source::MagazineSlot(0), Place::MagazineSlots);

        assert_eq!(moved, Err(MoveRefused::NoFreeSlot));
        assert_eq!(loadout, before);
    }

    #[test]
    fn a_body_carried_rifle_magazine_is_refused_when_only_a_pistol_slot_is_free() {
        let rifle_mag = Item::Magazine(crate::test_support::mag_in_slot("m", "rifle", "rifle"));
        let mut loadout = kinded_player(&[("pistol", 1)], vec![]);
        let mut remains = body(None, vec![rifle_mag]);
        let (before, remains_before) = (loadout.clone(), remains.clone());

        let moved = loadout.move_item(
            Some(&mut remains),
            Source::BodyCarried(0),
            Place::MagazineSlots,
        );

        assert_eq!(moved, Err(MoveRefused::NoFreeSlot));
        assert_eq!(loadout, before);
        assert_eq!(remains, remains_before);
    }

    #[test]
    fn a_magazine_moved_between_slots_keeps_to_its_kind() {
        let mut loadout = kinded_player(&[("pistol", 1), ("rifle", 2)], vec![]);
        let rifle = |name| crate::test_support::mag_in_slot(name, "rifle", "rifle");
        loadout.rig.stow(rifle("a")).expect("a rifle slot is free");
        loadout.rig.stow(rifle("b")).expect("a rifle slot is free");
        let before = loadout.clone();

        let moved = loadout.move_item(None, Source::MagazineSlot(1), Place::MagazineSlots);

        assert_eq!(moved, Ok(()));
        assert_eq!(loadout, before, "it goes back to the first free rifle slot");
    }

    #[test]
    fn reload_takes_a_fitting_magazine_from_any_kind_of_slot() {
        use crate::test_support::mag_in_slot;

        let mut loadout = kinded_player(&[("pistol", 1), ("rifle", 1)], vec![]);
        loadout
            .rig
            .stow(mag_in_slot("pistol_slot_fresh", "rifle", "pistol"))
            .expect("the pistol slot is free");

        let can = loadout.can_reload();
        let reloaded = loadout.reload();

        assert!(can);
        assert!(reloaded);
        assert_eq!(rifle_item(&loadout), "pistol_slot_fresh");
        assert_eq!(
            loadout.rig().magazines().count(),
            1,
            "the spent one took the slot"
        );
    }
}
