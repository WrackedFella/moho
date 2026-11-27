//! Load: what the player carries, set against what they can carry.

use crate::carried::loadout_items;
use crate::item::{ItemDef, ItemId};
use crate::loadout::Loadout;
use crate::stats::{Contribution, SourceLabel, Stat, UnknownItem, Verb};
use serde::Deserialize;
use thiserror::Error;

/// Where load, as a fraction of carry capacity, changes the player's state.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(try_from = "RawLoadThresholds")]
pub struct LoadThresholds {
    /// From here the player is slowed.
    pub slowed: f32,
    /// From here the player cannot sprint.
    pub no_sprint: f32,
    /// From here the player cannot move.
    pub cannot_move: f32,
}

/// Why a set of load thresholds was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum LoadThresholdsError {
    /// The thresholds are not finite and increasing from above 0.
    #[error(
        "thresholds must be finite with 0 < slowed < no_sprint < cannot_move, got {slowed}, {no_sprint}, {cannot_move}"
    )]
    Order {
        /// The slowed threshold given.
        slowed: f32,
        /// The no-sprint threshold given.
        no_sprint: f32,
        /// The cannot-move threshold given.
        cannot_move: f32,
    },
}

/// Unvalidated wire form of [`LoadThresholds`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLoadThresholds {
    slowed: f32,
    no_sprint: f32,
    cannot_move: f32,
}

impl TryFrom<RawLoadThresholds> for LoadThresholds {
    type Error = LoadThresholdsError;

    fn try_from(raw: RawLoadThresholds) -> Result<Self, Self::Error> {
        let RawLoadThresholds {
            slowed,
            no_sprint,
            cannot_move,
        } = raw;
        let finite = [slowed, no_sprint, cannot_move]
            .iter()
            .all(|x| x.is_finite());
        if finite && 0.0 < slowed && slowed < no_sprint && no_sprint < cannot_move {
            Ok(Self {
                slowed,
                no_sprint,
                cannot_move,
            })
        } else {
            Err(LoadThresholdsError::Order {
                slowed,
                no_sprint,
                cannot_move,
            })
        }
    }
}

/// How far load has taken the player's capability, each state including the ones before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadState {
    /// Below the slowed threshold.
    Fine,
    /// Moving slower.
    Slowed,
    /// Moving slower and unable to sprint.
    NoSprint,
    /// Unable to move.
    CannotMove,
}

/// Everything `loadout` carries, in kilograms: the held rifle (`weapon`) with its chambered
/// round and magazine, the worn rig and the magazines in its slots, the worn backpack, and
/// every inventory item, rounds and magazine contents by the weight of their round items.
///
/// # Errors
///
/// [`UnknownItem`] when something carried has no definition through `lookup`.
pub fn load<'a>(
    loadout: &Loadout,
    weapon: &ItemId,
    lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
) -> Result<f32, UnknownItem> {
    let mut total = 0.0;
    for (id, count) in loadout_items(loadout, weapon) {
        let def = lookup(&id).ok_or_else(|| UnknownItem { id: id.clone() })?;
        total += def.weight * count as f32;
    }
    Ok(total)
}

/// The state `load_kg` puts the player in against `capacity_kg`: each threshold is a
/// fraction of capacity and is reached at exactly that load.
#[must_use]
pub fn load_state(load_kg: f32, capacity_kg: f32, thresholds: &LoadThresholds) -> LoadState {
    if load_kg <= 0.0 {
        LoadState::Fine
    } else if load_kg >= thresholds.cannot_move * capacity_kg {
        LoadState::CannotMove
    } else if load_kg >= thresholds.no_sprint * capacity_kg {
        LoadState::NoSprint
    } else if load_kg >= thresholds.slowed * capacity_kg {
        LoadState::Slowed
    } else {
        LoadState::Fine
    }
}

/// What a load state contributes to the stats: slowed and no-sprint scale movement speed by
/// `slowed_scale`, no-sprint and cannot-move cap the stamina pool at 0, and cannot-move caps
/// movement speed at 0.
#[must_use]
pub fn load_contributions(state: LoadState, slowed_scale: f32) -> Vec<Contribution> {
    let load = |stat, verb| Contribution {
        source: SourceLabel::Load,
        stat,
        verb,
    };
    match state {
        LoadState::Fine => Vec::new(),
        LoadState::Slowed => vec![load(Stat::MovementSpeed, Verb::Scale(slowed_scale))],
        LoadState::NoSprint => vec![
            load(Stat::MovementSpeed, Verb::Scale(slowed_scale)),
            load(Stat::Stamina, Verb::Cap(0.0)),
        ],
        LoadState::CannotMove => vec![
            load(Stat::MovementSpeed, Verb::Cap(0.0)),
            load(Stat::Stamina, Verb::Cap(0.0)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Item, RoundStack};
    use crate::item::{ItemDef, RigSpec};
    use crate::rifle::Rifle;
    use crate::rig::Rig;
    use crate::stats::{PlayerDef, SourceLabel, Stat, Verb, equipment_contributions, resolve};
    use crate::test_support::{id, mag_of, rifle_well};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    const THRESHOLDS: LoadThresholds = LoadThresholds {
        slowed: 0.6,
        no_sprint: 0.8,
        cannot_move: 1.0,
    };

    fn player() -> PlayerDef {
        PlayerDef {
            strength: 10.0,
            carry_capacity_per_strength: 3.0,
            movement_speed: 1.0,
            stamina_pool: 5.0,
            slowed_scale: 0.7,
            thresholds: THRESHOLDS,
        }
    }

    fn def(value: Value) -> ItemDef {
        serde_json::from_value(value).expect("the test definition is valid")
    }

    fn weighing(kg: f32) -> ItemDef {
        def(json!({"class": "tool", "size": [1, 1], "weight": kg}))
    }

    fn pack(stats: Value) -> ItemDef {
        def(json!({
            "class": "gear", "size": [3, 3], "weight": 0.0, "backpack": {}, "stats": stats
        }))
    }

    /// Definitions for the ids a test loadout names, every one weightless except `extra`.
    fn items(extra: &[(&str, ItemDef)]) -> BTreeMap<ItemId, ItemDef> {
        let mut items: BTreeMap<ItemId, ItemDef> = ["rifle", "mag", "rig", "ball"]
            .into_iter()
            .map(|name| (id(name), weighing(0.0)))
            .collect();
        items.extend(extra.iter().map(|(name, def)| (id(name), def.clone())));
        items
    }

    fn loadout(held_rounds: u32, inventory: Vec<Item>) -> Loadout {
        let rifle = Rifle::loaded(rifle_well(), mag_of("mag", "rifle", 30, held_rounds))
            .expect("the magazine fits the rifle");
        let rig = Rig::new(id("rig"), &RigSpec { magazine_slots: 3 });
        Loadout::new(rifle, rig).with_inventory(inventory)
    }

    fn carried(items: &BTreeMap<ItemId, ItemDef>, loadout: &Loadout) -> f32 {
        load(loadout, &id("rifle"), |item| items.get(item)).expect("every item is defined")
    }

    fn ballast(name: &str) -> Item {
        Item::Other(id(name))
    }

    /// The carry capacity of `player` wearing what `loadout` wears.
    fn capacity(player: &PlayerDef, items: &BTreeMap<ItemId, ItemDef>, loadout: &Loadout) -> f32 {
        let worn = equipment_contributions(loadout, |item| items.get(item))
            .expect("every worn item is defined");
        resolve(
            Stat::CarryCapacity,
            player.baseline(Stat::CarryCapacity),
            0.0,
            &worn,
        )
    }

    #[test]
    fn load_state_at_15_20_25_30_of_30_kg() {
        let player = player();
        let expected = [
            (15.0, LoadState::Fine),
            (20.0, LoadState::Slowed),
            (25.0, LoadState::NoSprint),
            (30.0, LoadState::CannotMove),
        ];

        for (kg, state) in expected {
            let items = items(&[("ballast", weighing(kg))]);
            let loadout = loadout(0, vec![ballast("ballast")]);

            let carried = carried(&items, &loadout);
            let capacity = capacity(&player, &items, &loadout);

            assert_eq!(carried, kg);
            assert_eq!(capacity, 30.0);
            assert_eq!(
                load_state(carried, capacity, &THRESHOLDS),
                state,
                "{kg} kg of 30"
            );
        }
    }

    #[test]
    fn each_threshold_is_reached_at_exactly_its_load() {
        let at = |kg| load_state(kg, 30.0, &THRESHOLDS);

        assert_eq!(at(0.0), LoadState::Fine);
        assert_eq!(at(17.9), LoadState::Fine);
        assert_eq!(at(18.0), LoadState::Slowed);
        assert_eq!(at(23.9), LoadState::Slowed);
        assert_eq!(at(24.0), LoadState::NoSprint);
        assert_eq!(at(29.9), LoadState::NoSprint);
        assert_eq!(at(30.0), LoadState::CannotMove);
        assert_eq!(at(45.0), LoadState::CannotMove);
    }

    #[test]
    fn any_load_against_no_capacity_cannot_move() {
        assert_eq!(load_state(0.5, 0.0, &THRESHOLDS), LoadState::CannotMove);
    }

    #[test]
    fn thresholds_out_of_order_or_non_positive_are_refused() {
        let bad = [
            json!({"slowed": 0.9, "no_sprint": 0.5, "cannot_move": 1.0}),
            json!({"slowed": 0.6, "no_sprint": 0.6, "cannot_move": 1.0}),
            json!({"slowed": 0.6, "no_sprint": 0.9, "cannot_move": 0.8}),
            json!({"slowed": 0.0, "no_sprint": 0.8, "cannot_move": 1.0}),
            json!({"slowed": 0.6, "no_sprint": 0.8, "cannot_move": -1.0}),
        ];
        for thresholds in bad {
            let result = serde_json::from_value::<LoadThresholds>(thresholds.clone());

            assert!(result.is_err(), "{thresholds} was accepted: {result:?}");
        }
    }

    #[test]
    fn a_fine_player_has_no_load_contributions() {
        let contributions = load_contributions(LoadState::Fine, 0.7);

        assert_eq!(contributions, []);
    }

    fn thresholds(slowed: f64, no_sprint: f64, cannot_move: f64) -> Result<LoadThresholds, String> {
        serde_json::from_value(json!({
            "slowed": slowed, "no_sprint": no_sprint, "cannot_move": cannot_move
        }))
        .map_err(|error| error.to_string())
    }

    #[test]
    fn increasing_positive_thresholds_are_kept_as_given() {
        let loaded = thresholds(0.25, 0.5, 0.75).expect("increasing positive thresholds load");

        assert_eq!(
            loaded,
            LoadThresholds {
                slowed: 0.25,
                no_sprint: 0.5,
                cannot_move: 0.75,
            }
        );
    }

    #[test]
    fn a_zero_or_negative_slowed_threshold_is_refused_alone() {
        for slowed in [0.0, -0.1] {
            let error = thresholds(slowed, 0.5, 0.75)
                .expect_err("a slowed threshold of 0 or less is refused");

            assert!(error.contains("thresholds must be"), "{slowed}: {error}");
        }
    }

    #[test]
    fn equal_no_sprint_and_cannot_move_thresholds_are_refused() {
        let error = thresholds(0.25, 0.75, 0.75).expect_err("equal thresholds are refused");

        assert!(error.contains("thresholds must be"), "{error}");
    }

    #[test]
    fn slowed_scales_movement_speed_and_nothing_else() {
        let contributions = load_contributions(LoadState::Slowed, 0.55);

        assert_eq!(
            contributions,
            [Contribution {
                source: SourceLabel::Load,
                stat: Stat::MovementSpeed,
                verb: Verb::Scale(0.55),
            }]
        );
    }

    #[test]
    fn no_sprint_also_caps_the_stamina_pool_at_zero() {
        let contributions = load_contributions(LoadState::NoSprint, 0.55);

        let speed = Contribution {
            source: SourceLabel::Load,
            stat: Stat::MovementSpeed,
            verb: Verb::Scale(0.55),
        };
        let stamina = Contribution {
            source: SourceLabel::Load,
            stat: Stat::Stamina,
            verb: Verb::Cap(0.0),
        };
        assert_eq!(contributions.len(), 2, "{contributions:?}");
        assert!(contributions.contains(&speed), "{contributions:?}");
        assert!(contributions.contains(&stamina), "{contributions:?}");
    }

    #[test]
    fn cannot_move_caps_movement_speed_and_stamina_at_zero() {
        let contributions = load_contributions(LoadState::CannotMove, 0.7);

        let speed = Contribution {
            source: SourceLabel::Load,
            stat: Stat::MovementSpeed,
            verb: Verb::Cap(0.0),
        };
        let stamina = Contribution {
            source: SourceLabel::Load,
            stat: Stat::Stamina,
            verb: Verb::Cap(0.0),
        };
        assert_eq!(contributions.len(), 2, "{contributions:?}");
        assert!(contributions.contains(&speed), "{contributions:?}");
        assert!(contributions.contains(&stamina), "{contributions:?}");
    }

    #[test]
    fn load_sums_everything_the_player_carries_and_wears() {
        let items = items(&[
            ("rifle", weighing(3.4)),
            ("mag", weighing(0.25)),
            ("mag2", weighing(0.25)),
            ("mag3", weighing(0.25)),
            ("ball", weighing(0.012)),
            ("rig", weighing(1.2)),
            ("rig2", weighing(1.2)),
            ("pack", weighing(0.9)),
            ("bandage", weighing(0.05)),
        ]);
        let rifle = Rifle::loaded(rifle_well(), mag_of("mag", "rifle", 30, 10))
            .expect("the magazine fits the rifle");
        let mut rig = Rig::new(id("rig"), &RigSpec { magazine_slots: 3 });
        rig.stow(mag_of("mag3", "rifle", 30, 30))
            .expect("a slot is free");
        let loadout = Loadout::new(rifle, rig)
            .with_backpack(id("pack"))
            .with_inventory(vec![
                ballast("bandage"),
                Item::Rounds(RoundStack {
                    round: id("ball"),
                    count: 10,
                }),
                Item::Magazine(mag_of("mag2", "rifle", 30, 4)),
                Item::Rig(id("rig2")),
            ]);

        let total = carried(&items, &loadout);

        // rifle 3.4; held mag 0.25 and 10 rounds (9 in it, 1 chambered) 0.12; rig 1.2; slot mag
        // 0.25 and 30 rounds 0.36; pack 0.9; bandage 0.05; 10 loose rounds 0.12; inventory mag
        // 0.25 and 4 rounds 0.048; spare rig 1.2.
        assert!((total - 8.148).abs() < 1e-3, "carried {total} kg");
    }

    #[test]
    fn an_item_with_no_definition_is_an_error_naming_it() {
        let items = items(&[]);
        let cases = [
            (loadout(0, vec![ballast("mystery")]), "mystery"),
            (
                loadout(0, vec![]).with_backpack(id("ghost_pack")),
                "ghost_pack",
            ),
        ];

        for (loadout, missing) in cases {
            let result = load(&loadout, &id("rifle"), |item| items.get(item));

            assert_eq!(result, Err(UnknownItem { id: id(missing) }), "{missing}");
        }
    }

    #[test]
    fn a_backpack_adding_10_kg_turns_20_of_30_white() {
        let player = player();
        let items = items(&[
            ("ballast", weighing(20.0)),
            (
                "pack",
                pack(json!([{"stat": "carry_capacity", "add": 10.0}])),
            ),
        ]);
        let bare = loadout(0, vec![ballast("ballast")]);
        let packed = loadout(0, vec![ballast("ballast")]).with_backpack(id("pack"));

        let without = capacity(&player, &items, &bare);
        let with = capacity(&player, &items, &packed);

        assert_eq!(without, 30.0);
        assert_eq!(with, 40.0);
        assert_eq!(
            load_state(carried(&items, &bare), without, &THRESHOLDS),
            LoadState::Slowed
        );
        assert_eq!(
            load_state(carried(&items, &packed), with, &THRESHOLDS),
            LoadState::Fine
        );
    }

    #[test]
    fn a_backpack_scaling_carry_capacity_by_20_percent_gives_36_kg() {
        let player = player();
        let items = items(&[(
            "pack",
            pack(json!([{"stat": "carry_capacity", "scale": 1.2}])),
        )]);
        let packed = loadout(0, vec![]).with_backpack(id("pack"));

        let resolved = capacity(&player, &items, &packed);

        assert!((resolved - 36.0).abs() < 1e-4, "{resolved}");
    }

    #[test]
    fn a_worn_rigs_stats_count_like_a_backpacks() {
        let player = player();
        let rig = def(json!({
            "class": "gear", "size": [3, 2], "weight": 0.0, "rig": {"magazine_slots": 3},
            "stats": [{"stat": "carry_capacity", "add": 4.0}]
        }));
        let items = items(&[("rig", rig)]);
        let loadout = loadout(0, vec![]);

        let resolved = capacity(&player, &items, &loadout);

        assert_eq!(resolved, 34.0);
    }

    #[test]
    fn worn_gear_contributions_are_labelled_with_the_worn_item() {
        let items = items(&[(
            "pack",
            pack(json!([{"stat": "carry_capacity", "add": 10.0}])),
        )]);
        let loadout = loadout(0, vec![]).with_backpack(id("pack"));

        let worn = equipment_contributions(&loadout, |item| items.get(item))
            .expect("every worn item is defined");

        assert_eq!(
            worn,
            [Contribution {
                source: SourceLabel::Equipment(id("pack")),
                stat: Stat::CarryCapacity,
                verb: Verb::Add(10.0),
            }]
        );
    }

    #[test]
    fn items_carried_but_not_worn_contribute_no_stats() {
        let items = items(&[(
            "spare_pack",
            pack(json!([{"stat": "carry_capacity", "add": 10.0}])),
        )]);
        let loadout = loadout(0, vec![ballast("spare_pack")]);

        let worn = equipment_contributions(&loadout, |item| items.get(item))
            .expect("every worn item is defined");

        assert_eq!(worn, []);
    }

    #[test]
    fn a_worn_item_with_no_definition_is_an_error_naming_it() {
        let items = items(&[]);
        let loadout = loadout(0, vec![]).with_backpack(id("ghost_pack"));

        let result = equipment_contributions(&loadout, |item| items.get(item));

        assert_eq!(
            result,
            Err(UnknownItem {
                id: id("ghost_pack")
            })
        );
    }
}
