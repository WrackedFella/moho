#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::RigSpec;
    use crate::magazine::Magazine;
    use crate::test_support::{id, mag_of};

    fn rig_of(slots: u32) -> Rig {
        Rig::new(
            id("rig_chest"),
            &RigSpec {
                magazine_slots: slots,
            },
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
    fn put_places_a_magazine_in_the_named_empty_slot() {
        let mut rig = rig_of(3);

        rig.put(2, named("a"));

        assert_eq!(items(&rig), [None, None, some("a")]);
        assert_eq!(rig.free_slots(), 2);
    }
}
