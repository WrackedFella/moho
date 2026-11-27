//! A rig: slots that carry spare magazines.

use crate::item::{ItemId, RigSpec, SlotKind};
use crate::magazine::Magazine;

/// A worn rig with a fixed number of magazine slots, each of a kind, and a cursor for the
/// next reload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rig {
    item: ItemId,
    slots: Vec<Option<Magazine>>,
    /// The kind of each slot, in slot order.
    kinds: Vec<SlotKind>,
    /// Slot the next scan starts from; may equal the slot count, as the scan wraps it.
    next: usize,
}

/// The kind of each slot a rig of `spec` has, in slot order.
fn kinds_of(spec: &RigSpec) -> Vec<SlotKind> {
    spec.magazine_slots
        .iter()
        .flat_map(|(kind, &count)| std::iter::repeat_n(kind.clone(), count as usize))
        .collect()
}

impl Rig {
    /// A rig of the item `item` with every slot free.
    #[must_use]
    pub fn new(item: ItemId, spec: &RigSpec) -> Self {
        let kinds = kinds_of(spec);
        Self {
            item,
            slots: vec![None; kinds.len()],
            kinds,
            next: 0,
        }
    }

    /// A rig of the item `item` holding `slots`, free ones as `None`.
    ///
    /// # Errors
    ///
    /// Gives `slots` back when their number is not the number of slots `spec` lists.
    pub fn from_slots(
        item: ItemId,
        spec: &RigSpec,
        slots: Vec<Option<Magazine>>,
    ) -> Result<Self, Vec<Option<Magazine>>> {
        let kinds = kinds_of(spec);
        if slots.len() != kinds.len() {
            return Err(slots);
        }
        Ok(Self {
            item,
            slots,
            kinds,
            next: 0,
        })
    }

    /// The kind of slot `slot`; `None` when out of range.
    #[must_use]
    pub fn slot_kind(&self, slot: usize) -> Option<&SlotKind> {
        self.kinds.get(slot)
    }

    /// Whether a slot of kind `kind` is free.
    pub(crate) fn has_free_slot_for(&self, kind: &SlotKind) -> bool {
        self.free_slot_for(kind).is_some()
    }

    fn free_slot_for(&self, kind: &SlotKind) -> Option<usize> {
        self.slots
            .iter()
            .zip(&self.kinds)
            .position(|(held, slot_kind)| held.is_none() && slot_kind == kind)
    }

    /// The item this rig is an instance of.
    #[must_use]
    pub fn item(&self) -> &ItemId {
        &self.item
    }

    /// The slots in order, free ones as `None`.
    #[must_use]
    pub fn slots(&self) -> &[Option<Magazine>] {
        &self.slots
    }

    /// How many slots are free.
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_none()).count()
    }

    /// The magazines on the rig, in slot order.
    pub fn magazines(&self) -> impl Iterator<Item = &Magazine> {
        self.slots.iter().flatten()
    }

    /// Puts `magazine` in the first free slot of its kind and returns its index.
    ///
    /// # Errors
    ///
    /// Gives the magazine back when no slot of its kind is free.
    pub fn stow(&mut self, magazine: Magazine) -> Result<usize, Magazine> {
        match self.free_slot_for(magazine.slot()) {
            Some(index) => {
                self.slots[index] = Some(magazine);
                Ok(index)
            }
            None => Err(magazine),
        }
    }

    /// Takes the first magazine satisfying `fits`, scanning from the cursor and wrapping,
    /// and moves the cursor past its slot. Nothing changes when none qualifies.
    pub(crate) fn take_next(
        &mut self,
        fits: impl Fn(&Magazine) -> bool,
    ) -> Option<(usize, Magazine)> {
        let count = self.slots.len();
        let index = (0..count)
            .map(|offset| (self.next + offset) % count)
            .find(|&index| self.slots[index].as_ref().is_some_and(&fits))?;
        self.next = index + 1;
        self.slots[index].take().map(|magazine| (index, magazine))
    }

    /// Takes the magazine out of `slot`; `None` when the slot is free or out of range.
    pub(crate) fn take_slot(&mut self, slot: usize) -> Option<Magazine> {
        self.slots.get_mut(slot)?.take()
    }

    /// Puts `magazine` in the free slot `slot`.
    ///
    /// # Panics
    ///
    /// When `slot` is out of range or occupied: callers put back into a slot they just
    /// freed, and overwriting would silently drop a magazine.
    pub(crate) fn put(&mut self, slot: usize, magazine: Magazine) {
        let target = &mut self.slots[slot];
        assert!(target.is_none(), "rig slot {slot} is occupied");
        *target = Some(magazine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magazine::Magazine;
    use crate::test_support::{id, mag_of};

    fn rig_of(slots: u32) -> Rig {
        Rig::new(
            id("rig_chest"),
            &crate::test_support::rig_spec(&[("rifle", slots)]),
        )
    }

    fn named(item: &str) -> Magazine {
        mag_of(item, "rifle", 30, 30)
    }

    fn items(rig: &Rig) -> Vec<Option<String>> {
        rig.slots()
            .iter()
            .map(|slot| slot.as_ref().map(|m| m.item().as_str().to_owned()))
            .collect()
    }

    fn some(item: &str) -> Option<String> {
        Some(item.to_owned())
    }

    fn rig_with(layout: &[Option<&str>]) -> Rig {
        let mut rig = rig_of(u32::try_from(layout.len()).unwrap());
        for (slot, item) in layout.iter().enumerate() {
            if let Some(item) = item {
                rig.put(slot, named(item));
            }
        }
        rig
    }

    #[test]
    fn a_new_rig_has_the_specified_number_of_empty_slots() {
        let rig = rig_of(3);

        assert_eq!(rig.item(), &id("rig_chest"));
        assert_eq!(rig.slots().len(), 3);
        assert_eq!(rig.free_slots(), 3);
        assert_eq!(rig.magazines().count(), 0);
        assert!(rig.slots().iter().all(Option::is_none));
    }

    #[test]
    fn stow_fills_the_first_free_slot_and_names_it() {
        let mut rig = rig_with(&[Some("a"), None, None]);

        let slot = rig.stow(named("b"));

        assert_eq!(slot, Ok(1));
        assert_eq!(items(&rig), [some("a"), some("b"), None]);
        assert_eq!(rig.free_slots(), 1);
    }

    #[test]
    fn stow_into_a_full_rig_gives_the_magazine_back() {
        let mut rig = rig_with(&[Some("a"), Some("b")]);
        let before = rig.clone();

        let result = rig.stow(named("c"));

        assert_eq!(result, Err(named("c")));
        assert_eq!(rig, before);
    }

    #[test]
    fn a_rig_with_no_slots_refuses_every_magazine() {
        let mut rig = rig_of(0);

        assert_eq!(rig.stow(named("a")), Err(named("a")));
        assert_eq!(rig.take_next(|_| true), None);
        assert_eq!(rig.free_slots(), 0);
    }

    #[test]
    fn magazines_iterates_the_occupied_slots_in_slot_order() {
        let rig = rig_with(&[Some("a"), None, Some("c")]);

        let order: Vec<&str> = rig.magazines().map(|m| m.item().as_str()).collect();

        assert_eq!(order, ["a", "c"]);
        assert_eq!(rig.free_slots(), 1);
    }

    #[test]
    fn take_next_walks_the_slots_in_order_leaving_each_empty() {
        let mut rig = rig_with(&[Some("a"), Some("b"), Some("c")]);

        let first = rig.take_next(|_| true);
        let second = rig.take_next(|_| true);

        assert_eq!(first, Some((0, named("a"))));
        assert_eq!(second, Some((1, named("b"))));
        assert_eq!(items(&rig), [None, None, some("c")]);
    }

    #[test]
    fn take_next_skips_empty_slots_and_magazines_that_do_not_fit() {
        let mut rig = rig_with(&[None, Some("pistol"), Some("rifle_mag")]);

        let taken = rig.take_next(|m| m.item().as_str() != "pistol");

        assert_eq!(taken, Some((2, named("rifle_mag"))));
        assert_eq!(items(&rig), [None, some("pistol"), None]);
    }

    #[test]
    fn take_next_wraps_past_the_end_to_the_start() {
        let mut rig = rig_with(&[Some("a"), Some("b"), Some("c")]);
        let at_end = rig.take_next(|m| m.item().as_str() == "c");

        let wrapped = rig.take_next(|_| true);

        assert_eq!(at_end, Some((2, named("c"))));
        assert_eq!(wrapped, Some((0, named("a"))));
    }

    #[test]
    fn take_next_continues_from_after_the_last_taken_slot_not_the_first_occupied() {
        let mut rig = rig_with(&[Some("a"), Some("b"), Some("c")]);
        let middle = rig.take_next(|m| m.item().as_str() == "b");
        assert_eq!(middle, Some((1, named("b"))));

        let next = rig.take_next(|_| true);

        assert_eq!(next, Some((2, named("c"))));
    }

    #[test]
    fn take_next_with_nothing_that_fits_changes_nothing_and_keeps_the_cursor() {
        let mut rig = rig_with(&[Some("a"), Some("b"), Some("c")]);
        let taken = rig.take_next(|m| m.item().as_str() == "b");
        assert_eq!(taken, Some((1, named("b"))));
        let before = rig.clone();

        let none = rig.take_next(|_| false);

        assert_eq!(none, None);
        assert_eq!(rig, before);
        assert_eq!(rig.take_next(|_| true), Some((2, named("c"))));
    }

    #[test]
    fn take_next_on_an_empty_rig_returns_nothing() {
        let mut rig = rig_of(3);

        assert_eq!(rig.take_next(|_| true), None);
    }

    #[test]
    fn from_slots_keeps_the_layout_when_it_matches_the_spec() {
        let spec = crate::test_support::rig_spec(&[("rifle", 3)]);

        let rig = Rig::from_slots(id("rig_chest"), &spec, vec![None, Some(named("a")), None]);

        assert_eq!(items(&rig.unwrap()), [None, some("a"), None]);
    }

    #[test]
    fn from_slots_with_the_wrong_number_of_slots_gives_them_back() {
        let spec = crate::test_support::rig_spec(&[("rifle", 2)]);

        let rig = Rig::from_slots(id("rig_chest"), &spec, vec![Some(named("a")); 3]);

        assert_eq!(rig, Err(vec![Some(named("a")); 3]));
    }

    #[test]
    fn put_places_a_magazine_in_the_named_empty_slot() {
        let mut rig = rig_of(3);

        rig.put(2, named("a"));

        assert_eq!(items(&rig), [None, None, some("a")]);
        assert_eq!(rig.free_slots(), 2);
    }

    #[test]
    #[should_panic(expected = "occupied")]
    fn put_into_an_occupied_slot_panics_rather_than_dropping_a_magazine() {
        let mut rig = rig_with(&[Some("a")]);

        rig.put(0, named("b"));
    }

    /// Slots run in the order of their kinds' names, as the table is sorted: `pistol` first.
    fn kinded_rig(slots: &[(&str, u32)]) -> Rig {
        Rig::new(id("rig_chest"), &crate::test_support::rig_spec(slots))
    }

    #[test]
    fn a_magazine_takes_the_first_free_slot_of_its_kind() {
        let cases = [("rifle", 1), ("pistol", 0)];
        for (kind, expected) in cases {
            let mut rig = kinded_rig(&[("rifle", 1), ("pistol", 1)]);

            let slot = rig.stow(crate::test_support::mag_in_slot("m", "rifle", kind));

            assert_eq!(slot, Ok(expected), "{kind}");
            assert_eq!(rig.free_slots(), 1, "{kind}");
            assert!(rig.slots()[expected].is_some(), "{kind}");
        }

        let mut rig = kinded_rig(&[("pistol", 1), ("rifle", 2)]);
        let first = rig.stow(crate::test_support::mag_in_slot("a", "rifle", "rifle"));
        let second = rig.stow(crate::test_support::mag_in_slot("b", "rifle", "rifle"));
        assert_eq!(first, Ok(1));
        assert_eq!(second, Ok(2));
    }

    #[test]
    fn a_magazine_with_no_free_slot_of_its_kind_is_given_back_and_the_rig_is_unchanged() {
        let mut rig = kinded_rig(&[("pistol", 1), ("rifle", 1)]);
        rig.stow(crate::test_support::mag_in_slot("a", "rifle", "rifle"))
            .expect("the rifle slot is free");
        let before = rig.clone();
        let another = crate::test_support::mag_in_slot("b", "rifle", "rifle");

        let result = rig.stow(another.clone());

        assert_eq!(result, Err(another));
        assert_eq!(rig, before);
        assert_eq!(rig.free_slots(), 1, "the pistol slot stays free");
    }

    #[test]
    fn kinds_are_compared_by_exact_text() {
        let mut rig = kinded_rig(&[("rifle", 1)]);
        let shouted = crate::test_support::mag_in_slot("m", "rifle", "Rifle");

        let result = rig.stow(shouted.clone());

        assert_eq!(result, Err(shouted));
        assert_eq!(rig.free_slots(), 1);
    }

    #[test]
    fn restoring_a_magazine_into_a_slot_of_another_kind_keeps_it_there() {
        let spec = crate::test_support::rig_spec(&[("pistol", 1)]);
        let rifle = crate::test_support::mag_in_slot("m", "rifle", "rifle");

        let rig = Rig::from_slots(id("rig_chest"), &spec, vec![Some(rifle.clone())])
            .expect("the slot count matches");

        assert_eq!(rig.slots(), [Some(rifle)]);
        assert_eq!(rig.free_slots(), 0);
    }

    #[test]
    fn a_magazine_of_a_kind_no_slot_has_fits_no_slot() {
        let mut rig = kinded_rig(&[("rifle", 2)]);
        let odd = crate::test_support::mag_in_slot("m", "rifle", "belt");

        let result = rig.stow(odd.clone());

        assert_eq!(result, Err(odd));
        assert_eq!(rig.free_slots(), 2);
    }
}
