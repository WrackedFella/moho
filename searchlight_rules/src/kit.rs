//! Starting kits: a rig, a rifle and magazines named by item id, resolved against item data.

use crate::ballistics::RoundStats;
use crate::inventory::{Item, Remains, RoundStack};
use crate::item::{ItemDef, ItemId, ItemKind, SlotKind};
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

/// An item in a kit's inventory: a magazine takes `round` (and `rounds`, full when absent),
/// a round takes `count`, anything else takes none of the three.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitItem {
    /// The item.
    pub item: ItemId,
    /// For a magazine, the round item stacked in it.
    #[serde(default)]
    pub round: Option<ItemId>,
    /// For a magazine, rounds held; a full magazine when absent.
    #[serde(default)]
    pub rounds: Option<u32>,
    /// For a round, how many.
    #[serde(default)]
    pub count: Option<u32>,
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
    /// The backpack item worn.
    #[serde(default)]
    pub backpack: Option<ItemId>,
    /// What is carried outside the hands, the rig and the backpack, in order.
    #[serde(default)]
    pub carried: Vec<KitItem>,
}

/// A resolved kit: the weapon's numbers and what the player carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Kit {
    /// The weapon's definition.
    pub weapon: RifleDef,
    /// The rifle and rig, ready to play.
    pub loadout: Loadout,
    /// The ballistics and damage of every round the kit names.
    pub rounds: BTreeMap<ItemId, RoundStats>,
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
    /// More magazines of one kind than the rig has slots of that kind.
    #[error("{magazines} {kind} magazines do not fit the {slots} {kind} slots of rig \"{rig}\"")]
    RigOverfilled {
        /// The rig item.
        rig: ItemId,
        /// The kind of slot the magazines take.
        kind: SlotKind,
        /// The rig's slots of that kind; 0 when it has none.
        slots: u32,
        /// Magazines of that kind the kit stows.
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
    /// A carried entry names fields its item does not take, or lacks one it needs.
    #[error("the entry for item \"{id}\" has fields that do not fit its item")]
    EntryFields {
        /// The entry's item.
        id: ItemId,
    },
}

impl KitDef {
    /// Resolves the kit's ids through `lookup` as the gear of a dead enemy: its rig with the
    /// magazines stowed, its backpack, and its carried items with its rifle, and the magazine
    /// in it, first. Also returns the ballistics and damage of every round the kit names.
    ///
    /// # Errors
    ///
    /// The [`KitError`]s of [`Self::resolve`], and [`KitError::EntryFields`] for a carried
    /// entry whose fields do not fit its item.
    pub fn resolve_remains<'a>(
        &self,
        lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
    ) -> Result<(Remains, BTreeMap<ItemId, RoundStats>), KitError> {
        let parts = self.resolve_parts(lookup)?;
        let mut carried = vec![Item::Weapon(parts.rifle)];
        carried.extend(parts.carried);
        let remains = Remains {
            rig: Some(parts.rig),
            backpack: self.backpack.clone(),
            carried,
        };
        Ok((remains, parts.rounds))
    }

    /// Resolves the kit's ids through `lookup`.
    ///
    /// # Errors
    ///
    /// A [`KitError`] naming the offending item: unknown, of the wrong kind, a round of
    /// another caliber, too many rounds, a loaded magazine the weapon cannot take, or more
    /// magazines of a kind than the rig has slots of it. Rig magazines that do not fit the weapon are allowed.
    pub fn resolve<'a>(
        &self,
        lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
    ) -> Result<Kit, KitError> {
        let parts = self.resolve_parts(lookup)?;
        let mut loadout = Loadout::new(parts.rifle, parts.rig).with_inventory(parts.carried);
        if let Some(backpack) = &self.backpack {
            loadout = loadout.with_backpack(backpack.clone());
        }
        Ok(Kit {
            weapon: parts.weapon,
            loadout,
            rounds: parts.rounds,
        })
    }

    /// The pieces both [`Self::resolve`] and [`Self::resolve_remains`] build from.
    fn resolve_parts<'a>(
        &self,
        lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
    ) -> Result<Parts, KitError> {
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

        if let Some(backpack) = &self.backpack
            && !matches!(find(backpack)?.kind, ItemKind::Backpack(_))
        {
            return Err(wrong(backpack, "backpack"));
        }

        let mut rounds = BTreeMap::new();
        let build = |rounds: &mut BTreeMap<ItemId, RoundStats>,
                     magazine: &ItemId,
                     round: &ItemId,
                     held: Option<u32>|
         -> Result<Magazine, KitError> {
            let ItemKind::Magazine(spec) = &find(magazine)?.kind else {
                return Err(wrong(magazine, "magazine"));
            };
            let ItemKind::Round(spec_round) = &find(round)?.kind else {
                return Err(wrong(round, "round"));
            };
            if spec_round.caliber != spec.caliber {
                return Err(KitError::RoundDoesNotFit {
                    round: round.clone(),
                    magazine: magazine.clone(),
                });
            }
            let count = held.unwrap_or(spec.capacity);
            let filled = Magazine::filled(magazine.clone(), spec, round, count).map_err(|_| {
                KitError::OverCapacity {
                    magazine: magazine.clone(),
                    rounds: count,
                    capacity: spec.capacity,
                }
            })?;
            rounds.insert(round.clone(), spec_round.stats());
            Ok(filled)
        };

        let loaded = &self.loaded;
        let in_hand = build(&mut rounds, &loaded.magazine, &loaded.round, loaded.rounds)?;
        let rifle = Rifle::loaded(weapon.well.clone(), in_hand).map_err(|_| {
            KitError::MagazineDoesNotFit {
                magazine: loaded.magazine.clone(),
                weapon: self.weapon.clone(),
            }
        })?;
        let mut rig = Rig::new(self.rig.clone(), rig_spec);
        for entry in &self.magazines {
            let spare = build(&mut rounds, &entry.magazine, &entry.round, entry.rounds)?;
            rig.stow(spare).map_err(|refused| {
                let kind = refused.slot().clone();
                let magazines = self
                    .magazines
                    .iter()
                    .filter_map(
                        |listed| match lookup(&listed.magazine).map(|def| &def.kind) {
                            Some(ItemKind::Magazine(spec)) => Some(&spec.slot),
                            _ => None,
                        },
                    )
                    .filter(|listed_kind| **listed_kind == kind)
                    .count();
                KitError::RigOverfilled {
                    rig: self.rig.clone(),
                    slots: rig_spec.magazine_slots.get(&kind).copied().unwrap_or(0),
                    kind,
                    magazines,
                }
            })?;
        }

        let mut carried = Vec::with_capacity(self.carried.len());
        for entry in &self.carried {
            let fields = || KitError::EntryFields {
                id: entry.item.clone(),
            };
            let item = match &find(&entry.item)?.kind {
                ItemKind::Magazine(_) => {
                    let (Some(round), None) = (&entry.round, entry.count) else {
                        return Err(fields());
                    };
                    Item::Magazine(build(&mut rounds, &entry.item, round, entry.rounds)?)
                }
                ItemKind::Round(spec_round) => {
                    let (None, None, Some(count)) = (&entry.round, entry.rounds, entry.count)
                    else {
                        return Err(fields());
                    };
                    rounds.insert(entry.item.clone(), spec_round.stats());
                    Item::Rounds(RoundStack {
                        round: entry.item.clone(),
                        count,
                    })
                }
                _ => {
                    if entry.round.is_some() || entry.rounds.is_some() || entry.count.is_some() {
                        return Err(fields());
                    }
                    Item::Other(entry.item.clone())
                }
            };
            carried.push(item);
        }
        Ok(Parts {
            weapon: weapon.clone(),
            rifle,
            rig,
            carried,
            rounds,
        })
    }
}

/// What a kit resolves to before it is a player's loadout or an enemy's remains.
struct Parts {
    weapon: RifleDef,
    rifle: Rifle,
    rig: Rig,
    carried: Vec<Item>,
    rounds: BTreeMap<ItemId, RoundStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::Ballistics;
    use crate::inventory::{Item, RoundStack};
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
                    "well": "rifle", "fire": "semi_auto", "cooldown_ticks": 6,
                    "reload_ticks": 120, "charge_ticks": 30
                }
            },
            "mag_rifle": {
                "class": "gear", "size": [1, 2],
                "magazine": {"well": "rifle", "caliber": "5.56x45", "capacity": 30, "slot": "rifle"}
            },
            "mag_pistol": {
                "class": "gear", "size": [1, 1],
                "magazine": {"well": "pistol", "caliber": "9mm", "capacity": 15, "slot": "rifle"}
            },
            "round_ball": {
                "class": "consumable", "size": [1, 1],
                "round": {
                    "caliber": "5.56x45", "muzzle_velocity": 940.0, "drag": 1.0, "damage": 30
                }
            },
            "round_heavy": {
                "class": "consumable", "size": [1, 1],
                "round": {
                    "caliber": "5.56x45", "muzzle_velocity": 800.0, "drag": 2.5, "damage": 55
                }
            },
            "round_9mm": {
                "class": "consumable", "size": [1, 1],
                "round": {"caliber": "9mm", "muzzle_velocity": 400.0, "drag": 3.0, "damage": 20}
            },
            "rig_chest": {"class": "gear", "size": [3, 2], "rig": {"magazine_slots": {"rifle": 3}}},
            "crowbar": {"class": "tool", "size": [1, 3], "condition": 100}
        });
        let Value::Object(defs) = defs else {
            unreachable!("the literal above is an object");
        };
        defs.into_iter()
            .map(|(name, mut def)| {
                def["weight"] = json!(1.0);
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
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
            }
        );
    }

    #[test]
    fn a_resolved_kit_carries_the_stats_of_every_round_it_names() {
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
                RoundStats {
                    ballistics: Ballistics {
                        muzzle_velocity: 940.0,
                        drag: 1.0,
                    },
                    damage: 30,
                },
            ),
            (
                id("round_heavy"),
                RoundStats {
                    ballistics: Ballistics {
                        muzzle_velocity: 800.0,
                        drag: 2.5,
                    },
                    damage: 55,
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
                KitError::RigOverfilled { rig, kind, slots: 3, magazines: 4 }
                    if *rig == id("rig_chest") && kind.as_str() == "rifle"
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

    fn loot_items() -> HashMap<ItemId, ItemDef> {
        let mut items = items();
        for (name, def) in [
            (
                "backpack_small",
                json!({"class": "gear", "size": [3, 3], "weight": 1.5, "backpack": {}}),
            ),
            (
                "bandage",
                json!({"class": "consumable", "size": [1, 1], "weight": 0.1, "uses": 1}),
            ),
        ] {
            let def = serde_json::from_value(def)
                .unwrap_or_else(|e| panic!("fixture item {name} should load: {e}"));
            items.insert(ItemId::new(name), def);
        }
        items
    }

    fn with_loot(backpack: Value, carried: Value) -> KitDef {
        kit_def(json!({
            "rig": "rig_chest",
            "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
            "magazines": [{"magazine": "mag_rifle", "round": "round_ball", "rounds": 12}],
            "backpack": backpack,
            "carried": carried
        }))
    }

    fn resolve_loot(def: &KitDef) -> Result<Kit, KitError> {
        let items = loot_items();
        def.resolve(|item| items.get(item))
    }

    fn remains_of(def: &KitDef) -> Result<(Remains, BTreeMap<ItemId, RoundStats>), KitError> {
        let items = loot_items();
        def.resolve_remains(|item| items.get(item))
    }

    #[test]
    fn a_kit_resolves_its_backpack_and_carried_items_into_the_inventory() {
        let def = with_loot(
            json!("backpack_small"),
            json!([
                {"item": "bandage"},
                {"item": "mag_rifle", "round": "round_ball", "rounds": 10},
                {"item": "round_ball", "count": 20},
                {"item": "mag_rifle", "round": "round_heavy"}
            ]),
        );

        let kit = resolve_loot(&def).expect("the kit resolves");

        assert_eq!(kit.loadout.backpack(), Some(&id("backpack_small")));
        let inventory = kit.loadout.inventory();
        assert_eq!(inventory.len(), 4, "{inventory:?}");
        assert_eq!(inventory[0], Item::Other(id("bandage")));
        let held = |round: &str, rounds: u32| {
            Item::Magazine(
                Magazine::filled(
                    id("mag_rifle"),
                    &crate::test_support::spec("rifle", 30),
                    &id(round),
                    rounds,
                )
                .expect("the rounds fit"),
            )
        };
        assert_eq!(inventory[1], held("round_ball", 10));
        assert_eq!(
            inventory[2],
            Item::Rounds(RoundStack {
                round: id("round_ball"),
                count: 20
            })
        );
        assert_eq!(
            inventory[3],
            held("round_heavy", 30),
            "a magazine with no rounds listed is full"
        );
    }

    #[test]
    fn a_kit_naming_a_non_backpack_as_backpack_is_refused() {
        for offender in ["crowbar", "rig_chest", "bandage"] {
            let def = with_loot(json!(offender), json!([]));

            let resolved = resolve_loot(&def).expect_err("not a backpack");
            let remains = remains_of(&def).expect_err("not a backpack");

            for error in [resolved, remains] {
                assert!(
                    matches!(
                        &error,
                        KitError::WrongKind { id: wrong, expected: "backpack" }
                            if *wrong == id(offender)
                    ),
                    "{offender}: {error:?}"
                );
            }
        }
    }

    #[test]
    fn a_kit_naming_an_unknown_backpack_or_carried_item_is_refused() {
        let cases = [
            with_loot(json!("nope"), json!([])),
            with_loot(json!("backpack_small"), json!([{"item": "nope"}])),
        ];
        for def in cases {
            let error = resolve_loot(&def).expect_err("an unknown item");

            assert!(
                matches!(&error, KitError::UnknownItem { id: unknown } if *unknown == id("nope")),
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_carried_entry_whose_fields_do_not_fit_its_item_is_refused() {
        let cases = [
            (
                "a magazine with a count",
                json!({"item": "mag_rifle", "round": "round_ball", "count": 3}),
            ),
            ("a magazine with no round", json!({"item": "mag_rifle"})),
            (
                "a round with a round",
                json!({"item": "round_ball", "round": "round_ball", "count": 3}),
            ),
            (
                "a round with rounds",
                json!({"item": "round_ball", "rounds": 3, "count": 3}),
            ),
            ("a round with no count", json!({"item": "round_ball"})),
            (
                "a tool with a count",
                json!({"item": "crowbar", "count": 1}),
            ),
            (
                "a tool with a round",
                json!({"item": "crowbar", "round": "round_ball"}),
            ),
            (
                "a tool with rounds",
                json!({"item": "crowbar", "rounds": 1}),
            ),
            (
                "a bandage with a count",
                json!({"item": "bandage", "count": 2}),
            ),
        ];
        for (label, entry) in cases {
            let offender = entry["item"].as_str().expect("an item id").to_owned();
            let def = with_loot(json!("backpack_small"), json!([entry]));

            let resolved = resolve_loot(&def).expect_err(label);
            let remains = remains_of(&def).expect_err(label);

            for error in [resolved, remains] {
                assert!(
                    matches!(&error, KitError::EntryFields { id: named } if *named == id(&offender)),
                    "{label}: {error:?}"
                );
                assert!(error.to_string().contains(&offender), "{label}: {error}");
            }
        }
    }

    #[test]
    fn a_carried_entry_with_a_wrong_kind_round_or_too_many_rounds_is_refused() {
        let wrong_round = with_loot(
            json!("backpack_small"),
            json!([{"item": "mag_rifle", "round": "crowbar"}]),
        );
        let too_many = with_loot(
            json!("backpack_small"),
            json!([{"item": "mag_rifle", "round": "round_ball", "rounds": 31}]),
        );

        let kind = resolve_loot(&wrong_round).expect_err("a crowbar is not a round");
        let capacity = resolve_loot(&too_many).expect_err("31 rounds do not fit 30");

        assert!(
            matches!(&kind, KitError::WrongKind { id: wrong, expected: "round" } if *wrong == id("crowbar")),
            "{kind:?}"
        );
        assert!(
            matches!(
                &capacity,
                KitError::OverCapacity {
                    rounds: 31,
                    capacity: 30,
                    ..
                }
            ),
            "{capacity:?}"
        );
    }

    #[test]
    fn a_kit_resolves_into_remains_with_its_rig_backpack_and_rifle_first() {
        let def = with_loot(json!("backpack_small"), json!([{"item": "bandage"}]));

        let (remains, rounds) = remains_of(&def).expect("the enemy kit resolves");

        let rig = remains.rig.as_ref().expect("the enemy wears a rig");
        assert_eq!(rig.item(), &id("rig_chest"));
        assert_eq!(rig.slots().len(), 3);
        let on_rig: Vec<u32> = rig.magazines().map(Magazine::rounds).collect();
        assert_eq!(on_rig, [12]);
        assert_eq!(remains.backpack, Some(id("backpack_small")));
        assert_eq!(remains.carried.len(), 2, "{:?}", remains.carried);
        let Item::Weapon(rifle) = &remains.carried[0] else {
            panic!("the rifle comes first, got {:?}", remains.carried[0]);
        };
        assert_eq!(rifle.magazine().rounds(), 29);
        assert!(rifle.is_chambered());
        assert_eq!(remains.carried[1], Item::Other(id("bandage")));
        assert_eq!(
            rounds.keys().collect::<Vec<_>>(),
            [&id("round_ball")],
            "the kit names one round"
        );
    }

    #[test]
    fn remains_carry_the_ballistics_of_every_round_in_the_kit() {
        let def = kit_def(json!({
            "rig": "rig_chest", "weapon": "rifle",
            "loaded": {"magazine": "mag_rifle", "round": "round_ball"},
            "magazines": [{"magazine": "mag_rifle", "round": "round_heavy", "rounds": 12}],
            "carried": [{"item": "round_9mm", "count": 5}]
        }));

        let (_, rounds) = remains_of(&def).expect("the enemy kit resolves");

        assert_eq!(
            rounds.get(&id("round_heavy")),
            Some(&RoundStats {
                ballistics: Ballistics {
                    muzzle_velocity: 800.0,
                    drag: 2.5
                },
                damage: 55
            })
        );
        assert_eq!(
            rounds.get(&id("round_9mm")),
            Some(&RoundStats {
                ballistics: Ballistics {
                    muzzle_velocity: 400.0,
                    drag: 3.0
                },
                damage: 20
            }),
            "a round only carried loose"
        );
        assert_eq!(rounds.len(), 3);
    }

    /// The fixture items with a rig of one rifle slot and two pistol slots, and magazines
    /// that name the kind of slot they take.
    fn kinded_items() -> HashMap<ItemId, ItemDef> {
        let mut items = items();
        for (name, def) in [
            (
                "rig_chest",
                json!({"class": "gear", "size": [3, 2], "weight": 1.0,
                    "rig": {"magazine_slots": {"rifle": 1, "pistol": 2}}}),
            ),
            (
                "mag_rifle",
                json!({"class": "gear", "size": [1, 2], "weight": 1.0,
                    "magazine": {"well": "rifle", "caliber": "5.56x45", "capacity": 30,
                        "slot": "rifle"}}),
            ),
            (
                "mag_pistol",
                json!({"class": "gear", "size": [1, 1], "weight": 1.0,
                    "magazine": {"well": "pistol", "caliber": "9mm", "capacity": 15,
                        "slot": "pistol"}}),
            ),
        ] {
            let def = serde_json::from_value(def)
                .unwrap_or_else(|e| panic!("fixture item {name} should load: {e}"));
            items.insert(ItemId::new(name), def);
        }
        items
    }

    fn resolve_kinded(def: &KitDef) -> Result<Kit, KitError> {
        let items = kinded_items();
        def.resolve(|item| items.get(item))
    }

    #[test]
    fn a_kit_stowing_more_magazines_than_their_kinds_slots_is_refused() {
        let def = start(json!([
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ]));

        let error = resolve_kinded(&def).expect_err("one rifle slot cannot hold two magazines");

        assert!(
            matches!(&error, KitError::RigOverfilled { rig, .. } if *rig == id("rig_chest")),
            "{error:?}"
        );
        assert!(error.to_string().contains("rig_chest"), "{error}");
    }

    #[test]
    fn an_overfilled_kind_reports_that_kinds_slots_and_magazines() {
        let def = start(json!([
            {"magazine": "mag_pistol", "round": "round_9mm"},
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_rifle", "round": "round_ball"}
        ]));

        let error = resolve_kinded(&def).expect_err("one rifle slot cannot hold two magazines");

        assert!(
            matches!(
                &error,
                KitError::RigOverfilled { rig, kind, slots: 1, magazines: 2 }
                    if *rig == id("rig_chest") && kind.as_str() == "rifle"
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "2 rifle magazines do not fit the 1 rifle slots of rig \"rig_chest\""
        );
    }

    #[test]
    fn a_magazine_kind_the_rig_lacks_reports_zero_slots() {
        let mut items = kinded_items();
        let rifle_only = json!({"class": "gear", "size": [3, 2], "weight": 1.0,
            "rig": {"magazine_slots": {"rifle": 1}}});
        items.insert(
            id("rig_chest"),
            serde_json::from_value(rifle_only).expect("the fixture rig loads"),
        );
        let def = start(json!([{"magazine": "mag_pistol", "round": "round_9mm"}]));

        let error = def
            .resolve(|item| items.get(item))
            .expect_err("the rig has no pistol slot");

        assert!(
            matches!(
                &error,
                KitError::RigOverfilled { rig, kind, slots: 0, magazines: 1 }
                    if *rig == id("rig_chest") && kind.as_str() == "pistol"
            ),
            "{error:?}"
        );
    }

    #[test]
    fn interleaved_kinds_each_take_the_first_free_slot_of_their_kind() {
        let def = start(json!([
            {"magazine": "mag_pistol", "round": "round_9mm", "rounds": 5},
            {"magazine": "mag_rifle", "round": "round_ball", "rounds": 7},
            {"magazine": "mag_pistol", "round": "round_9mm", "rounds": 6}
        ]));

        let kit = resolve_kinded(&def).expect("each kind fits its slots");

        let rounds: Vec<Option<u32>> = kit
            .loadout
            .rig()
            .slots()
            .iter()
            .map(|slot| slot.as_ref().map(Magazine::rounds))
            .collect();
        assert_eq!(rounds, [Some(5), Some(6), Some(7)]);
    }

    #[test]
    fn a_kit_stowing_each_kind_within_its_slots_is_accepted_in_slot_order() {
        let def = start(json!([
            {"magazine": "mag_rifle", "round": "round_ball"},
            {"magazine": "mag_pistol", "round": "round_9mm"},
            {"magazine": "mag_pistol", "round": "round_9mm", "rounds": 5}
        ]));

        let kit = resolve_kinded(&def).expect("each kind fits its slots");

        let held: Vec<Option<&str>> = kit
            .loadout
            .rig()
            .slots()
            .iter()
            .map(|slot| slot.as_ref().map(|m| m.item().as_str()))
            .collect();
        assert_eq!(
            held,
            [Some("mag_pistol"), Some("mag_pistol"), Some("mag_rifle")],
            "pistol slots sort before the rifle slot"
        );
        assert_eq!(kit.loadout.rig().free_slots(), 0);
    }
}
