#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemId, RigSpec};
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
        let mut loadout = loadout(mag_of("spent", "rifle", 30, 10), &[None, None, None]);
        let before = loadout.clone();

        assert!(!loadout.can_reload());
        let reloaded = loadout.reload();

        assert!(!reloaded);
        assert_eq!(loadout, before);
    }

    #[test]
    fn a_rig_with_no_slots_cannot_reload() {
        let mut loadout = loadout(mag_of("spent", "rifle", 30, 10), &[]);
        let before = loadout.clone();

        assert!(!loadout.can_reload());
        assert!(!loadout.reload());
        assert_eq!(loadout, before);
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
    fn a_rig_holding_only_magazines_that_do_not_fit_cannot_reload() {
        let wrong = mag_of("pistol_mag", "pistol", 15, 15);
        let mut loadout = loadout(mag_of("spent", "rifle", 30, 10), &[Some(wrong)]);
        let before = loadout.clone();

        assert!(!loadout.can_reload());
        assert!(!loadout.reload());
        assert_eq!(loadout, before);
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

        assert_eq!(shots, [Shot::Fired, Shot::Fired, Shot::Dry]);
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
}
