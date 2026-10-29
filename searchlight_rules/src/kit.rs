//! Starting kits: a rig, a rifle and magazines named by item id, resolved against item data.

use crate::ballistics::Ballistics;
use crate::item::{ItemDef, ItemId, ItemKind};
use crate::loadout::Loadout;
use crate::magazine::Magazine;
use crate::rifle::Rifle;
use crate::rig::Rig;
use crate::weapon::RifleDef;
use serde::Deserialize;
use std::collections::BTreeMap;
use thiserror::Error;

/// A magazine in a kit: which magazine item, which round it holds, and how many.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitMagazine {
    /// The magazine item.
    pub magazine: ItemId,
    /// The round item stacked in it.
    pub round: ItemId,
    /// Rounds held; a full magazine when absent.
    #[serde(default)]
    pub rounds: Option<u32>,
}

/// A kit as defined in data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitDef {
    /// The rig item worn.
    pub rig: ItemId,
    /// The weapon item carried.
    pub weapon: ItemId,
    /// The magazine in the weapon.
    pub loaded: KitMagazine,
    /// Magazines on the rig, stowed in order.
    #[serde(default)]
    pub magazines: Vec<KitMagazine>,
}

/// A resolved kit: the weapon's numbers and what the player carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Kit {
    /// The weapon's definition.
    pub weapon: RifleDef,
    /// The rifle and rig, ready to play.
    pub loadout: Loadout,
    /// The ballistics of every round the kit names.
    pub rounds: BTreeMap<ItemId, Ballistics>,
}

/// Why a kit could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum KitError {
    /// An id the kit names has no item.
    #[error("unknown item \"{id}\"")]
    UnknownItem {
        /// The id.
        id: ItemId,
    },
    /// An item is not the kind its place in the kit needs.
    #[error("item \"{id}\" is not a {expected}")]
    WrongKind {
        /// The id.
        id: ItemId,
        /// The kind needed.
        expected: &'static str,
    },
    /// More magazines than the rig has slots.
    #[error("{magazines} magazines do not fit the {slots} slots of rig \"{rig}\"")]
    RigOverfilled {
        /// The rig item.
        rig: ItemId,
        /// Its slots.
        slots: u32,
        /// Magazines listed.
        magazines: usize,
    },
    /// The loaded magazine is for another well than the weapon's.
    #[error("magazine \"{magazine}\" does not fit weapon \"{weapon}\"")]
    MagazineDoesNotFit {
        /// The magazine item.
        magazine: ItemId,
        /// The weapon item.
        weapon: ItemId,
    },
    /// The round is of another caliber than the magazine's.
    #[error("round \"{round}\" does not fit magazine \"{magazine}\"")]
    RoundDoesNotFit {
        /// The round item.
        round: ItemId,
        /// The magazine item.
        magazine: ItemId,
    },
    /// More rounds than the magazine holds.
    #[error("{rounds} rounds exceed the capacity {capacity} of magazine \"{magazine}\"")]
    OverCapacity {
        /// The magazine item.
        magazine: ItemId,
        /// Rounds listed.
        rounds: u32,
        /// The magazine's capacity.
        capacity: u32,
    },
}

impl KitDef {
    /// Resolves the kit's ids through `lookup`.
    ///
    /// # Errors
    ///
    /// A [`KitError`] naming the offending item: unknown, of the wrong kind, a round of
    /// another caliber, too many rounds, a loaded magazine the weapon cannot take, or more
    /// magazines than the rig has slots. Rig magazines that do not fit the weapon are allowed.
    pub fn resolve<'a>(
        &self,
        lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
    ) -> Result<Kit, KitError> {
        let find = |id: &ItemId| lookup(id).ok_or_else(|| KitError::UnknownItem { id: id.clone() });
        let wrong = |id: &ItemId, expected| KitError::WrongKind {
            id: id.clone(),
            expected,
        };

        let ItemKind::Rig(rig_spec) = &find(&self.rig)?.kind else {
            return Err(wrong(&self.rig, "rig"));
        };
        let ItemKind::Weapon(weapon) = &find(&self.weapon)?.kind else {
            return Err(wrong(&self.weapon, "weapon"));
        };

        let build = |entry: &KitMagazine| -> Result<Magazine, KitError> {
            let ItemKind::Magazine(spec) = &find(&entry.magazine)?.kind else {
                return Err(wrong(&entry.magazine, "magazine"));
            };
            let ItemKind::Round(round) = &find(&entry.round)?.kind else {
                return Err(wrong(&entry.round, "round"));
            };
            if round.caliber != spec.caliber {
                return Err(KitError::RoundDoesNotFit {
                    round: entry.round.clone(),
                    magazine: entry.magazine.clone(),
                });
            }
            let rounds = entry.rounds.unwrap_or(spec.capacity);
            Magazine::filled(entry.magazine.clone(), spec, &entry.round, rounds).map_err(|_| {
                KitError::OverCapacity {
                    magazine: entry.magazine.clone(),
                    rounds,
                    capacity: spec.capacity,
                }
            })
        };

        let in_hand = build(&self.loaded)?;
        let rifle = Rifle::loaded(weapon.well.clone(), in_hand).map_err(|_| {
            KitError::MagazineDoesNotFit {
                magazine: self.loaded.magazine.clone(),
                weapon: self.weapon.clone(),
            }
        })?;
        let mut rig = Rig::new(self.rig.clone(), rig_spec);
        for entry in &self.magazines {
            rig.stow(build(entry)?)
                .map_err(|_| KitError::RigOverfilled {
                    rig: self.rig.clone(),
                    slots: rig_spec.magazine_slots,
                    magazines: self.magazines.len(),
                })?;
        }
        Ok(Kit {
            weapon: weapon.clone(),
            loadout: Loadout::new(rifle, rig),
            rounds: BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::Ballistics;
    use crate::item::{ItemDef, ItemId, MagazineWell};
    use crate::test_support::id;
    use crate::weapon::{FireBehaviour, RifleDef};
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, HashMap};

    fn items() -> HashMap<ItemId, ItemDef> {
        let defs = json!({
            "rifle": {
                "class": "weapon", "size": [2, 5], "condition": 100,
                "weapon": {
                    "well": "rifle", "damage": 30, "fire": "semi_auto", "cooldown_ticks": 6,
                    "reload_ticks": 120, "charge_ticks": 30
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
            "round_ball": {
                "class": "consumable", "size": [1, 1],
                "round": {"caliber": "5.56x45", "muzzle_velocity": 940.0, "drag": 1.0}
            },
            "round_heavy": {
                "class": "consumable", "size": [1, 1],
                "round": {"caliber": "5.56x45", "muzzle_velocity": 800.0, "drag": 2.5}
            },
            "round_9mm": {
                "class": "consumable", "size": [1, 1],
                "round": {"caliber": "9mm", "muzzle_velocity": 400.0, "drag": 3.0}
            },
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
            }
        );
    }

    #[test]
    fn a_resolved_kit_carries_the_ballistics_of_every_round_it_names() {
        let def = kit_def(json!({
            "rig": "rig_chest",
            "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
            "magazines": [
                {"magazine": "mag_rifle", "round": "round_heavy"},
                {"magazine": "mag_rifle", "round": "round_ball"}
            ]
        }));

        let kit = resolve(&def).expect("the kit resolves");

        let expected = BTreeMap::from([
            (
                id("round_ball"),
                Ballistics {
                    muzzle_velocity: 940.0,
                    drag: 1.0,
                },
            ),
            (
                id("round_heavy"),
                Ballistics {
                    muzzle_velocity: 800.0,
                    drag: 2.5,
                },
            ),
        ]);
        assert_eq!(kit.rounds, expected);
    }

    #[test]
    fn a_kit_names_only_the_rounds_it_carries() {
        let kit = resolve(&start(json!([]))).expect("the kit resolves");

        assert_eq!(kit.rounds.keys().collect::<Vec<_>>(), [&id("round_ball")]);
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
            .map(|slot| slot.as_ref().map(Magazine::rounds))
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
