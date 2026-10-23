#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ItemDef, ItemId, MagazineWell};
    use crate::test_support::id;
    use crate::weapon::{FireBehaviour, RifleDef};
    use serde_json::{Value, json};
    use std::collections::HashMap;

    fn items() -> HashMap<ItemId, ItemDef> {
        let defs = json!({
            "rifle": {
                "class": "weapon", "size": [2, 5], "condition": 100,
                "weapon": {
                    "well": "rifle", "damage": 30, "fire": "semi_auto", "cooldown_ticks": 6,
                    "reload_ticks": 120, "charge_ticks": 30, "range": 100.0
                }
            },
            "mag_rifle": {
                "class": "gear", "size": [1, 2],
                "magazine": {"well": "rifle", "caliber": "5.56x45", "capacity": 30}
            },
            "mag_pistol": {
                "class": "gear", "size": [1, 1],
                "magazine": {"well": "pistol", "caliber": "9mm", "capacity": 15}
            },
            "round_ball": {"class": "consumable", "size": [1, 1], "round": {"caliber": "5.56x45"}},
            "round_9mm": {"class": "consumable", "size": [1, 1], "round": {"caliber": "9mm"}},
            "rig_chest": {"class": "gear", "size": [3, 2], "rig": {"magazine_slots": 3}},
            "crowbar": {"class": "tool", "size": [1, 3], "condition": 100}
        });
        let Value::Object(defs) = defs else {
            unreachable!("the literal above is an object");
        };
        defs.into_iter()
            .map(|(name, def)| {
                let def = serde_json::from_value(def)
                    .unwrap_or_else(|e| panic!("fixture item {name} should load: {e}"));
                (ItemId::new(name), def)
            })
            .collect()
    }

    fn kit_def(value: Value) -> KitDef {
        serde_json::from_value(value).expect("the fixture kit deserializes")
    }

    fn start(magazines: Value) -> KitDef {
        kit_def(json!({
            "rig": "rig_chest",
            "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
            "magazines": magazines
        }))
    }

    fn two_full() -> Value {
        json!([
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ])
    }

    fn resolve(def: &KitDef) -> Result<Kit, KitError> {
        let items = items();
        def.resolve(|item| items.get(item))
    }

    fn error_of(def: &KitDef) -> KitError {
        match resolve(def) {
            Ok(_) => panic!("the kit should have been refused"),
            Err(error) => error,
        }
    }

    #[test]
    fn a_kit_resolves_to_a_chambered_rifle_and_a_rig_with_one_free_slot() {
        let kit = resolve(&start(two_full())).expect("the starting kit resolves");

        let loadout = &kit.loadout;
        assert!(loadout.rifle().is_chambered());
        assert_eq!(loadout.rifle().chamber(), Some(&id("round_ball")));
        assert_eq!(loadout.rifle().magazine().capacity(), 30);
        assert_eq!(loadout.rifle().magazine().rounds(), 29);
        assert_eq!(loadout.rig().item(), &id("rig_chest"));
        assert_eq!(loadout.rig().slots().len(), 3);
        assert_eq!(loadout.rig().free_slots(), 1);
        assert_eq!(loadout.rig().magazines().count(), 2);
        assert!(loadout.rig().magazines().all(|m| m.rounds() == 30));
    }

    #[test]
    fn a_resolved_kit_carries_the_weapon_definition() {
        let kit = resolve(&start(two_full())).expect("the starting kit resolves");

        assert_eq!(
            kit.weapon,
            RifleDef {
                well: MagazineWell::new("rifle"),
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                range: 100.0,
            }
        );
    }

    #[test]
    fn rig_magazines_are_stowed_in_listed_order_from_the_first_slot() {
        let def = start(json!([
            {"magazine": "mag_rifle", "round": "round_ball", "rounds": 5},
            {"magazine": "mag_rifle", "round": "round_ball", "rounds": 6}
        ]));

        let kit = resolve(&def).expect("the kit resolves");

        let rounds: Vec<Option<u32>> = kit
            .loadout
            .rig()
            .slots()
            .iter()
            .map(|slot| slot.as_ref().map(|m| m.rounds()))
            .collect();
        assert_eq!(rounds, [Some(5), Some(6), None]);
    }

    #[test]
    fn a_short_fill_holds_only_the_listed_rounds() {
        let def = kit_def(json!({
            "rig": "rig_chest",
            "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball", "rounds": 10}
        }));

        let kit = resolve(&def).expect("a kit with no spare magazines resolves");

        assert_eq!(kit.loadout.rifle().magazine().rounds(), 9);
        assert!(kit.loadout.rifle().is_chambered());
        assert_eq!(kit.loadout.rig().free_slots(), 3);
    }

    #[test]
    fn a_kit_with_more_magazines_than_rig_slots_is_refused() {
        let four = json!([
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ]);

        let error = error_of(&start(four));

        assert!(
            matches!(
                &error,
                KitError::RigOverfilled { rig, slots: 3, magazines: 4 } if *rig == id("rig_chest")
            ),
            "{error:?}"
        );
        assert!(error.to_string().contains("rig_chest"), "{error}");
    }

    #[test]
    fn a_kit_with_exactly_as_many_magazines_as_slots_is_accepted() {
        let three = json!([
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ]);

        let kit = resolve(&start(three)).expect("three magazines fit three slots");

        assert_eq!(kit.loadout.rig().free_slots(), 0);
    }

    #[test]
    fn an_unknown_item_id_is_refused_naming_it() {
        let cases = [
            json!({"rig": "nope", "weapon": "rifle",
                   "loaded": {"magazine": "mag_rifle", "round": "round_ball"}}),
            json!({"rig": "rig_chest", "weapon": "nope",
                   "loaded": {"magazine": "mag_rifle", "round": "round_ball"}}),
            json!({"rig": "rig_chest", "weapon": "rifle",
                   "loaded": {"magazine": "nope", "round": "round_ball"}}),
            json!({"rig": "rig_chest", "weapon": "rifle",
                   "loaded": {"magazine": "mag_rifle", "round": "nope"}}),
            json!({"rig": "rig_chest", "weapon": "rifle",
                   "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
                   "magazines": [{"magazine": "nope", "round": "round_ball"}]}),
        ];
        for case in cases {
            let error = error_of(&kit_def(case.clone()));

            assert!(
                matches!(&error, KitError::UnknownItem { id: unknown } if *unknown == id("nope")),
                "{case}: {error:?}"
            );
            assert!(error.to_string().contains("nope"), "{error}");
        }
    }

    #[test]
    fn an_item_of_the_wrong_kind_is_refused_naming_it() {
        let cases = [
            (
                "crowbar",
                "rig",
                json!({"rig": "crowbar", "weapon": "rifle",
                "loaded": {"magazine": "mag_rifle", "round": "round_ball"}}),
            ),
            (
                "mag_rifle",
                "weapon",
                json!({"rig": "rig_chest", "weapon": "mag_rifle",
                "loaded": {"magazine": "mag_rifle", "round": "round_ball"}}),
            ),
            (
                "round_ball",
                "magazine",
                json!({"rig": "rig_chest", "weapon": "rifle",
                "loaded": {"magazine": "round_ball", "round": "round_ball"}}),
            ),
            (
                "rig_chest",
                "round",
                json!({"rig": "rig_chest", "weapon": "rifle",
                "loaded": {"magazine": "mag_rifle", "round": "rig_chest"}}),
            ),
            (
                "crowbar",
                "magazine",
                json!({"rig": "rig_chest", "weapon": "rifle",
                "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
                "magazines": [{"magazine": "crowbar", "round": "round_ball"}]}),
            ),
        ];
        for (offender, expected, case) in cases {
            let error = error_of(&kit_def(case));

            assert!(
                matches!(
                    &error,
                    KitError::WrongKind { id: wrong, expected: kind }
                        if *wrong == id(offender) && *kind == expected
                ),
                "{offender} as {expected}: {error:?}"
            );
            assert!(error.to_string().contains(offender), "{error}");
        }
    }

    #[test]
    fn a_round_of_another_caliber_is_refused() {
        let def = kit_def(json!({"rig": "rig_chest", "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_9mm"}}));

        let error = error_of(&def);

        assert!(
            matches!(
                &error,
                KitError::RoundDoesNotFit { round, magazine }
                    if *round == id("round_9mm") && *magazine == id("mag_rifle")
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_stowed_magazine_with_a_round_of_another_caliber_is_refused() {
        let def = start(json!([{"magazine": "mag_rifle", "round": "round_9mm"}]));

        let error = error_of(&def);

        assert!(
            matches!(
                &error,
                KitError::RoundDoesNotFit { round, magazine }
                    if *round == id("round_9mm") && *magazine == id("mag_rifle")
            ),
            "{error:?}"
        );
    }

    #[test]
    fn more_rounds_than_the_magazine_holds_is_refused() {
        let loaded = kit_def(json!({"rig": "rig_chest", "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball", "rounds": 31}}));
        let stowed = start(json!([
            {"magazine": "mag_rifle", "round": "round_ball", "rounds": 31}
        ]));

        for (label, def) in [("loaded", loaded), ("stowed", stowed)] {
            let error = error_of(&def);

            assert!(
                matches!(
                    &error,
                    KitError::OverCapacity { magazine, rounds: 31, capacity: 30 }
                        if *magazine == id("mag_rifle")
                ),
                "{label}: {error:?}"
            );
        }
    }

    #[test]
    fn the_loaded_magazine_must_fit_the_weapon() {
        let def = kit_def(json!({"rig": "rig_chest", "weapon": "rifle",
            "loaded": {"magazine": "mag_pistol", "round": "round_9mm"}}));

        let error = error_of(&def);

        assert!(
            matches!(
                &error,
                KitError::MagazineDoesNotFit { magazine, weapon }
                    if *magazine == id("mag_pistol") && *weapon == id("rifle")
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_rig_magazine_that_does_not_fit_the_weapon_is_allowed() {
        let def = start(json!([
            {"magazine": "mag_pistol", "round": "round_9mm"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ]));

        let kit = resolve(&def).expect("rig magazines need not fit the weapon");

        assert_eq!(kit.loadout.rig().magazines().count(), 2);
        assert!(kit.loadout.can_reload());
    }

    #[test]
    fn a_kit_with_an_unknown_field_is_refused() {
        let result = serde_json::from_value::<KitDef>(json!({
            "rig": "rig_chest", "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
            "magzines": []
        }));

        let text = result.expect_err("a misspelt field is refused").to_string();
        assert!(text.contains("magzines"), "{text}");
    }
}
