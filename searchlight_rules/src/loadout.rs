//! One player's carried state: the rifle in hand, the rig that feeds it, the backpack worn and
//! the inventory of everything else.

use crate::inventory::{Item, MoveRefused, Place, Remains, Source, WearRefused};
use crate::item::{ItemDef, ItemId, ItemKind, RigSpec};
use crate::magazine::Magazine;
use crate::rifle::{Rifle, Shot};
use crate::rig::Rig;

/// What a successful wear changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Worn {
    /// The rig was swapped.
    Rig,
    /// The backpack was swapped or put on.
    Backpack,
}

impl Worn {
    /// What wearing `item` would swap, or `None` when it is not a rig or backpack by its
    /// definition through `lookup`. [`Loadout::wear`] wears exactly the items this names.
    pub fn of<'a>(item: &Item, lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>) -> Option<Self> {
        Gear::of(item, lookup).map(|gear| match gear {
            Gear::Rig(..) => Self::Rig,
            Gear::Backpack(_) => Self::Backpack,
        })
    }
}

/// An inventory item that can be worn, with what wearing it needs.
enum Gear<'i, 'd> {
    Rig(&'i ItemId, &'d RigSpec),
    Backpack(&'i ItemId),
}

impl<'i, 'd> Gear<'i, 'd> {
    fn of(item: &'i Item, lookup: impl Fn(&ItemId) -> Option<&'d ItemDef>) -> Option<Self> {
        match item {
            Item::Rig(id) => match &lookup(id)?.kind {
                ItemKind::Rig(spec) => Some(Self::Rig(id, spec)),
                _ => None,
            },
            Item::Other(id) => match &lookup(id)?.kind {
                ItemKind::Backpack(_) => Some(Self::Backpack(id)),
                _ => None,
            },
            _ => None,
        }
    }
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
        let is_magazine = self
            .holds_magazine(body.as_deref(), from)
            .ok_or(MoveRefused::NoSuchItem)?;
        if to == Place::MagazineSlots {
            if !is_magazine {
                return Err(MoveRefused::NotAMagazine);
            }
            let leaves_a_slot = matches!(from, Source::MagazineSlot(_));
            if self.rig.free_slots() == 0 && !leaves_a_slot {
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

    /// Wears the rig or backpack at `from` in the inventory, looking definitions up through
    /// `lookup`. The worn one of its kind goes to `from`; a replaced rig's magazines leave its
    /// slots, in slot order, for the end of the inventory, and the new rig starts empty. With
    /// no backpack worn, the inventory entry is removed and nothing goes back.
    ///
    /// # Errors
    ///
    /// A [`WearRefused`] when `from` holds nothing or holds something that is not a rig or
    /// backpack. A refused wear changes nothing.
    pub fn wear<'a>(
        &mut self,
        lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
        from: usize,
    ) -> Result<Worn, WearRefused> {
        let item = self.inventory.get(from).ok_or(WearRefused::NoSuchItem)?;
        match Gear::of(item, lookup).ok_or(WearRefused::NotWearable)? {
            Gear::Rig(id, spec) => {
                let new_rig = Rig::new(id.clone(), spec);
                let mut old_rig = std::mem::replace(&mut self.rig, new_rig);
                let magazines = old_rig.take_all();
                self.inventory[from] = Item::Rig(old_rig.item().clone());
                self.inventory
                    .extend(magazines.into_iter().map(Item::Magazine));
                Ok(Worn::Rig)
            }
            Gear::Backpack(id) => {
                let id = id.clone();
                match self.backpack.replace(id) {
                    Some(old) => self.inventory[from] = Item::Other(old),
                    None => {
                        self.inventory.remove(from);
                    }
                }
                Ok(Worn::Backpack)
            }
        }
    }

    /// Whether `from` holds an item, and if so whether it is a magazine.
    fn holds_magazine(&self, body: Option<&Remains>, from: Source) -> Option<bool> {
        let in_slot = |slots: &[Option<Magazine>], index: usize| {
            slots.get(index).and_then(Option::as_ref).map(|_| true)
        };
        match from {
            Source::Inventory(index) => self
                .inventory
                .get(index)
                .map(|item| matches!(item, Item::Magazine(_))),
            Source::MagazineSlot(index) => in_slot(self.rig.slots(), index),
            Source::BodyCarried(index) => body?
                .carried
                .get(index)
                .map(|item| matches!(item, Item::Magazine(_))),
            Source::BodyMagazineSlot(index) => in_slot(body?.rig.as_ref()?.slots(), index),
            Source::BodyRig => body?.rig.as_ref().map(|_| false),
            Source::BodyBackpack => body?.backpack.as_ref().map(|_| false),
        }
    }

    /// Removes and returns what `from` holds. Call it only for a source
    /// [`Self::holds_magazine`] found an item in: an index past the end of a list panics.
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
    use crate::item::{ItemId, RigSpec};
    use crate::magazine::Magazine;
    use crate::rifle::{Rifle, Shot};
    use crate::rig::Rig;
    use crate::stats::{Stat, equipment_contributions, resolve};
    use crate::test_support::{id, mag_of, rifle_well};
    use proptest::prelude::*;

    fn rifle_with(magazine: Magazine) -> Rifle {
        Rifle::loaded(rifle_well(), magazine).expect("the test magazine fits the rifle")
    }

    /// A loadout with `magazine` in the rifle and one rig slot per `layout` entry.
    fn loadout(magazine: Magazine, layout: &[Option<Magazine>]) -> Loadout {
        let mut rig = Rig::new(
            id("rig"),
            &RigSpec {
                magazine_slots: u32::try_from(layout.len()).unwrap(),
            },
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
            &RigSpec {
                magazine_slots: u32::try_from(layout.len()).unwrap(),
            },
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

    fn def(value: serde_json::Value) -> ItemDef {
        serde_json::from_value(value).expect("the test definition is valid")
    }

    fn rig_def(slots: u32) -> ItemDef {
        def(serde_json::json!({
            "class": "gear", "size": [3, 3], "weight": 0.0, "rig": {"magazine_slots": slots}
        }))
    }

    fn pack_def(stats: serde_json::Value) -> ItemDef {
        def(serde_json::json!({
            "class": "gear", "size": [3, 3], "weight": 0.0, "backpack": {}, "stats": stats
        }))
    }

    fn weighing(kg: f32) -> ItemDef {
        def(serde_json::json!({"class": "tool", "size": [1, 1], "weight": kg}))
    }

    /// Definitions for the ids a wear test names; any id not listed has none.
    fn defs(listed: &[(&str, ItemDef)]) -> std::collections::BTreeMap<ItemId, ItemDef> {
        listed
            .iter()
            .map(|(name, def)| (id(name), def.clone()))
            .collect()
    }

    fn wear_defs() -> std::collections::BTreeMap<ItemId, ItemDef> {
        defs(&[
            ("rig", rig_def(3)),
            ("rig_five", rig_def(5)),
            ("rig_two", rig_def(2)),
            ("pack", pack_def(serde_json::json!([]))),
            ("bandage", weighing(0.1)),
        ])
    }

    /// A player wearing the 3-slot `rig` and the backpack `pack`, holding `inventory`.
    fn wearer(slots: &[Option<&str>], inventory: Vec<Item>) -> Loadout {
        player(slots, inventory)
    }

    #[test]
    fn wearing_a_five_slot_rig_puts_the_three_slot_in_its_inventory_place() {
        let defs = wear_defs();
        let mut loadout = wearer(
            &[None, None, None],
            vec![bandage(), Item::Rig(id("rig_five")), bandage()],
        );

        let worn = loadout.wear(|item| defs.get(item), 1);

        assert_eq!(worn, Ok(Worn::Rig));
        assert_eq!(loadout.rig().item(), &id("rig_five"));
        assert_eq!(loadout.rig().slots().len(), 5);
        assert_eq!(
            loadout.inventory(),
            [bandage(), Item::Rig(id("rig")), bandage()]
        );
    }

    #[test]
    fn a_rig_swap_appends_the_old_rigs_magazines_in_slot_order_and_leaves_the_new_rig_empty() {
        let defs = wear_defs();
        let mut loadout = wearer(
            &[Some("a"), None, Some("b")],
            vec![bandage(), Item::Rig(id("rig_five")), Item::Other(id("rag"))],
        );

        let worn = loadout.wear(|item| defs.get(item), 1);

        assert_eq!(worn, Ok(Worn::Rig));
        assert_eq!(
            loadout.inventory(),
            [
                bandage(),
                Item::Rig(id("rig")),
                Item::Other(id("rag")),
                mag_item("a"),
                mag_item("b"),
            ]
        );
        assert_eq!(slot_items(&loadout), [None, None, None, None, None]);
        assert_eq!(loadout.rig().free_slots(), 5);
    }

    #[test]
    fn a_rig_swap_over_capacity_still_un_equips_every_magazine() {
        let mut defs = wear_defs();
        defs.insert(id("anvil"), weighing(500.0));
        let lookup = |item: &ItemId| defs.get(item).or_else(|| defs.get(&id("bandage")));
        let mut loadout = wearer(
            &[Some("a"), Some("b"), None],
            vec![Item::Other(id("anvil")), Item::Rig(id("rig_two"))],
        );
        let capacity = resolve(
            Stat::CarryCapacity,
            30.0,
            0.0,
            &equipment_contributions(&loadout, lookup).expect("worn gear is defined"),
        );
        assert!(
            crate::load::load(&loadout, &id("rifle"), lookup).expect("all defined") > capacity,
            "the player starts over capacity"
        );

        let worn = loadout.wear(lookup, 1);

        assert_eq!(worn, Ok(Worn::Rig));
        assert_eq!(
            loadout.inventory(),
            [
                Item::Other(id("anvil")),
                Item::Rig(id("rig")),
                mag_item("a"),
                mag_item("b"),
            ]
        );
    }

    #[test]
    fn after_wearing_a_ten_kg_backpack_capacity_resolves_to_35() {
        let defs = defs(&[
            ("rig", rig_def(3)),
            (
                "pack5",
                pack_def(serde_json::json!([{"stat": "carry_capacity", "add": 5.0}])),
            ),
            (
                "pack10",
                pack_def(serde_json::json!([{"stat": "carry_capacity", "add": 10.0}])),
            ),
        ]);
        let mut loadout = wearer(&[None], vec![Item::Other(id("pack10"))]);
        loadout = loadout.with_backpack(id("pack5"));
        let capacity = |loadout: &Loadout| {
            let worn = equipment_contributions(loadout, |item| defs.get(item))
                .expect("worn gear is defined");
            resolve(Stat::CarryCapacity, 25.0, 0.0, &worn)
        };
        assert_eq!(capacity(&loadout), 30.0);

        let worn = loadout.wear(|item| defs.get(item), 0);

        assert_eq!(worn, Ok(Worn::Backpack));
        assert_eq!(capacity(&loadout), 35.0);
    }

    #[test]
    fn wearing_a_twenty_percent_backpack_resolves_30_to_36() {
        let defs = defs(&[
            ("rig", rig_def(3)),
            (
                "frame",
                pack_def(serde_json::json!([{"stat": "carry_capacity", "scale": 1.2}])),
            ),
        ]);
        let mut loadout = Loadout::new(
            rifle_with(mag_of("held", "rifle", 30, 10)),
            Rig::new(id("rig"), &RigSpec { magazine_slots: 3 }),
        )
        .with_inventory(vec![Item::Other(id("frame"))]);

        let worn = loadout.wear(|item| defs.get(item), 0);

        assert_eq!(worn, Ok(Worn::Backpack));
        let contributions =
            equipment_contributions(&loadout, |item| defs.get(item)).expect("worn gear is defined");
        let capacity = resolve(Stat::CarryCapacity, 30.0, 0.0, &contributions);
        assert!((capacity - 36.0).abs() < 1e-4, "{capacity}");
    }

    #[test]
    fn a_backpack_swap_keeps_the_inventory_and_puts_the_old_pack_in_its_place() {
        let defs = defs(&[
            ("rig", rig_def(3)),
            ("pack", pack_def(serde_json::json!([]))),
            ("pack_big", pack_def(serde_json::json!([]))),
        ]);
        let mut loadout = wearer(
            &[Some("a"), None, Some("b")],
            vec![bandage(), Item::Other(id("pack_big")), bandage()],
        );
        let rifle_before = loadout.rifle().clone();
        let rig_before = loadout.rig().clone();

        let worn = loadout.wear(|item| defs.get(item), 1);

        assert_eq!(worn, Ok(Worn::Backpack));
        assert_eq!(loadout.backpack(), Some(&id("pack_big")));
        assert_eq!(
            loadout.inventory(),
            [bandage(), Item::Other(id("pack")), bandage()]
        );
        assert_eq!(loadout.rig(), &rig_before);
        assert_eq!(loadout.rifle(), &rifle_before);
    }

    #[test]
    fn wearing_a_magazine_is_refused_and_changes_nothing() {
        let defs = wear_defs();
        let items = [
            mag_item("m"),
            bandage(),
            Item::Weapon(rifle_with(full("in_rifle"))),
            Item::Rounds(RoundStack {
                round: id("ball"),
                count: 30,
            }),
            Item::Other(id("no_such_definition")),
            Item::Rig(id("no_such_rig_definition")),
            Item::Rig(id("pack")),
            Item::Other(id("rig_five")),
        ];
        for item in items {
            let mut loadout = wearer(&[Some("a")], vec![item.clone()]);
            let before = loadout.clone();

            let worn = loadout.wear(|item| defs.get(item), 0);

            assert_eq!(worn, Err(WearRefused::NotWearable), "{item:?}");
            assert_eq!(loadout, before, "{item:?}");
        }
    }

    #[test]
    fn wearing_from_an_index_holding_nothing_is_refused_and_changes_nothing() {
        let defs = wear_defs();
        for index in [1, 7] {
            let mut loadout = wearer(&[Some("a")], vec![Item::Rig(id("rig_five"))]);
            let before = loadout.clone();

            let worn = loadout.wear(|item| defs.get(item), index);

            assert_eq!(worn, Err(WearRefused::NoSuchItem), "{index}");
            assert_eq!(loadout, before, "{index}");
        }
    }

    #[test]
    fn wearing_a_backpack_with_none_worn_leaves_nothing_behind() {
        let defs = wear_defs();
        let mut loadout = Loadout::new(
            rifle_with(mag_of("held", "rifle", 30, 10)),
            Rig::new(id("rig"), &RigSpec { magazine_slots: 3 }),
        )
        .with_inventory(vec![bandage(), Item::Other(id("pack"))]);

        let worn = loadout.wear(|item| defs.get(item), 1);

        assert_eq!(worn, Ok(Worn::Backpack));
        assert_eq!(loadout.backpack(), Some(&id("pack")));
        assert_eq!(loadout.inventory(), [bandage()]);
    }

    #[test]
    fn a_rig_swap_leaves_the_rifles_magazine_in_the_rifle() {
        let defs = wear_defs();
        let mut loadout = wearer(
            &[Some("a"), Some("b"), None],
            vec![Item::Rig(id("rig_five"))],
        );
        let rifle_before = loadout.rifle().clone();

        let worn = loadout.wear(|item| defs.get(item), 0);

        assert_eq!(worn, Ok(Worn::Rig));
        assert_eq!(loadout.rifle(), &rifle_before);
    }

    /// Definitions for the ids the conservation property wears: by the number after `i`.
    struct ConservationDefs {
        rigs: Vec<ItemDef>,
        pack: ItemDef,
        plain: ItemDef,
    }

    impl ConservationDefs {
        fn new() -> Self {
            Self {
                rigs: (0..5).map(rig_def).collect(),
                pack: pack_def(serde_json::json!([])),
                plain: weighing(0.0),
            }
        }

        fn lookup(&self, item: &ItemId) -> Option<&ItemDef> {
            match item.as_str() {
                "rig" => Some(&self.rigs[3]),
                "pack" | "body_pack" => Some(&self.pack),
                name => {
                    let n: usize = name.strip_prefix('i')?.parse().ok()?;
                    match n % 3 {
                        0 => Some(&self.plain),
                        1 => Some(&self.rigs[n % 5]),
                        _ => Some(&self.pack),
                    }
                }
            }
        }
    }

    proptest! {
        #[test]
        fn any_wear_and_move_sequence_keeps_every_item_exactly_once(
            kinds in proptest::collection::vec(0u8..5, 0..6),
            body_kinds in proptest::collection::vec(0u8..5, 0..4),
            slots in proptest::collection::vec(any::<bool>(), 0..4),
            ops in proptest::collection::vec((any::<bool>(), 0usize..7, 0u8..6, 0u8..3), 0..14),
        ) {
            let defs = ConservationDefs::new();
            let names: Vec<String> = (0..slots.len()).map(|n| format!("s{n}")).collect();
            let layout: Vec<Option<&str>> = slots
                .iter()
                .zip(&names)
                .map(|(held, name)| held.then_some(name.as_str()))
                .collect();
            let inventory = kinds.iter().enumerate().map(|(n, k)| inventory_item(*k, n)).collect();
            let mut loadout = player(&layout, inventory);
            let mut remains = Remains {
                rig: None,
                backpack: Some(id("body_pack")),
                carried: body_kinds
                    .iter()
                    .enumerate()
                    .map(|(n, k)| inventory_item(*k, 100 + n))
                    .collect(),
            };
            let expected = leaves(&loadout, &remains);

            for (wear, index, kind, place) in ops {
                let (loadout_before, remains_before) = (loadout.clone(), remains.clone());
                let refused = if wear {
                    loadout.wear(|item| defs.lookup(item), index).is_err()
                } else {
                    loadout
                        .move_item(Some(&mut remains), source_of(kind, index), place_of(place))
                        .is_err()
                };

                if refused {
                    prop_assert_eq!(&loadout, &loadout_before);
                    prop_assert_eq!(&remains, &remains_before);
                }
                prop_assert_eq!(leaves(&loadout, &remains), expected.clone());
            }
        }
    }
}
