//! Loads the starting kit from item and kit definitions in the content roots.

use std::collections::BTreeMap;

use moho_core::content::{ContentError, ContentRoots, Definitions, load_kind};
use searchlight_rules::{
    ItemDef, ItemId, Kit, KitDef, KitError, Remains, RoundStats, check_launch,
};
use thiserror::Error;

/// The id of the kit a run starts with.
pub const START_KIT_ID: &str = "start";

/// Why a kit could not be loaded.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KitLoadError {
    /// A definitions file or id could not be loaded.
    #[error(transparent)]
    Content(#[from] ContentError),
    /// The kit's items do not make a valid kit.
    #[error("kit \"{kit}\": {source}")]
    Kit {
        /// The kit's id.
        kit: String,
        /// What is wrong with it.
        #[source]
        source: KitError,
    },
}

/// The id of the kit every enemy carries and wears.
pub const ENEMY_KIT_ID: &str = "enemy";

/// Loads `items` and `kits` through `roots` and resolves the kit `kit` as the gear of a dead
/// enemy, with the ballistics and damage of every round it names.
///
/// # Errors
///
/// As [`load`].
pub fn load_enemy(
    roots: &ContentRoots,
    kit: &str,
) -> Result<(Remains, BTreeMap<ItemId, RoundStats>), KitLoadError> {
    resolve_kit(roots, kit, |def, lookup| def.resolve_remains(lookup)).map(|(enemy, _)| enemy)
}

/// Loads `items` and `kits` through `roots` once and resolves the kit `start` as the player's
/// and the kit `enemy` as the gear of a dead enemy, with the ballistics and damage of every
/// round the enemy kit names.
///
/// The player may fire any looted round, and the round's magazine well is not known once the
/// rounds are joined, so every enemy round is checked against the player's barrel, whether
/// or not the player's weapon could take its magazine.
///
/// # Errors
///
/// As [`load_catalogue`] and [`load_enemy`]; also [`KitLoadError::Kit`] for `enemy` with
/// [`KitError::TooSlowFromWeapon`] or [`KitError::NotFiniteFromWeapon`], naming the player's
/// weapon and the round, when the player's barrel launches an enemy round too slowly or at a
/// speed that is not finite.
pub fn load_start_and_enemy(
    roots: &ContentRoots,
    start: &str,
    enemy: &str,
) -> Result<(Catalogue, Remains, BTreeMap<ItemId, RoundStats>), KitLoadError> {
    let catalogue = load_catalogue(roots, start)?;
    let (remains, rounds) = load_enemy(roots, enemy)?;
    for (round, stats) in &rounds {
        check_launch(&catalogue.weapon, &catalogue.kit.weapon, round, stats).map_err(|source| {
            KitLoadError::Kit {
                kit: enemy.to_owned(),
                source,
            }
        })?;
    }
    Ok((catalogue, remains, rounds))
}

/// Loads `items` and `kits` through `roots`, finds the kit `kit` and resolves it with `resolve`,
/// returning the result with the item definitions it resolved against.
fn resolve_kit<T>(
    roots: &ContentRoots,
    kit: &str,
    resolve: impl for<'d> FnOnce(
        &'d KitDef,
        &dyn Fn(&ItemId) -> Option<&'d ItemDef>,
    ) -> Result<T, KitError>,
) -> Result<(T, Definitions<ItemDef>), KitLoadError> {
    let items = load_kind::<ItemDef>(roots, "items")?;
    let kits = load_kind::<KitDef>(roots, "kits")?;
    let resolved =
        resolve(kits.require(kit)?, &|id: &ItemId| items.get(id.as_str())).map_err(|source| {
            KitLoadError::Kit {
                kit: kit.to_owned(),
                source,
            }
        })?;
    Ok((resolved, items))
}

/// A resolved kit with the item definitions it was resolved against and the id of its weapon.
#[derive(Debug)]
pub struct Catalogue {
    /// The resolved kit.
    pub kit: Kit,
    /// The item id of the kit's weapon.
    pub weapon: ItemId,
    items: Definitions<ItemDef>,
}

impl Catalogue {
    /// The definition of each of `ids`, by id.
    ///
    /// # Errors
    ///
    /// [`KitLoadError::Content`] naming the first id the catalogue does not define. An id a
    /// resolved kit holds is always defined, so this means `ids` came from somewhere else.
    pub fn define(&self, ids: &[ItemId]) -> Result<BTreeMap<ItemId, ItemDef>, KitLoadError> {
        ids.iter()
            .map(|id| Ok((id.clone(), self.items.require(id.as_str())?.clone())))
            .collect()
    }
}

/// Loads `items` and `kits` through `roots` and resolves the kit `kit`, keeping the item
/// definitions for [`Catalogue::define`].
///
/// # Errors
///
/// As [`load`].
pub fn load_catalogue(roots: &ContentRoots, kit: &str) -> Result<Catalogue, KitLoadError> {
    let ((resolved, weapon), items) = resolve_kit(roots, kit, |def, lookup| {
        def.resolve(lookup)
            .map(|resolved| (resolved, def.weapon.clone()))
    })?;
    Ok(Catalogue {
        kit: resolved,
        weapon,
        items,
    })
}

/// Loads `items` and `kits` through `roots` and resolves the kit `kit`.
///
/// # Errors
///
/// [`KitLoadError::Content`] when a file or entry is malformed or `kit` is undefined;
/// [`KitLoadError::Kit`] when the kit names an unknown or unsuitable item.
pub fn load(roots: &ContentRoots, kit: &str) -> Result<Kit, KitLoadError> {
    resolve_kit(roots, kit, |def, lookup| def.resolve(lookup)).map(|(kit, _)| kit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_root;
    use moho_core::content::{ContentError, ContentRoots, load_kind};
    use searchlight_rules::{
        Ballistics, ItemClass, ItemDef, ItemId, ItemKind, KitError, MagazineWell, Shot,
    };
    use std::path::Path;

    use crate::tuning::{tuning, with_enemy_kit};
    use searchlight_rules::Item;

    const RIFLE: &str = r#"
[rifle]
class = "weapon"
size = [2, 5]
condition = 100
weight = 3.0
[rifle.weapon]
well = "rifle"
fire = "semi_auto"
cooldown_ticks = 6
reload_ticks = 120
charge_ticks = 30
"#;

    const OTHER_ITEMS: &str = r#"
[mag_rifle]
class = "gear"
size = [1, 2]
magazine = { well = "rifle", caliber = "5.56x45", capacity = 30, slot = "rifle" }
weight = 0.25

[round_ball]
class = "consumable"
size = [1, 1]
round = { caliber = "5.56x45", muzzle_velocity = 940.0, drag = 1.0, damage = 30 }
weight = 0.01

[rig_chest]
class = "gear"
size = [3, 2]
rig = { magazine_slots = { rifle = 3 } }
weight = 1.0

[crowbar]
class = "tool"
size = [1, 3]
condition = 100
weight = 2.0
"#;

    fn items_toml() -> String {
        format!("{RIFLE}{OTHER_ITEMS}")
    }

    fn start_kit(magazines: usize) -> String {
        let entry = "{ magazine = \"mag_rifle\", round = \"round_ball\" }";
        let list = vec![entry; magazines].join(", ");
        format!(
            "[start]\nrig = \"rig_chest\"\nweapon = \"rifle\"\nloaded = {entry}\nmagazines = [{list}]\n"
        )
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), text).unwrap();
    }

    fn base_with(items: &str, kits: &str) -> (tempfile::TempDir, ContentRoots) {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "items.toml", items);
        write(dir.path(), "kits.toml", kits);
        let roots = ContentRoots::base(dir.path()).unwrap();
        (dir, roots)
    }

    fn with_mod(
        items: &str,
        kits: &str,
        mod_items: Option<&str>,
        mod_kits: Option<&str>,
    ) -> (tempfile::TempDir, ContentRoots) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("content");
        let mods = dir.path().join("mods");
        write(&base, "items.toml", items);
        write(&base, "kits.toml", kits);
        if let Some(text) = mod_items {
            write(&mods.join("wells"), "items.toml", text);
        }
        if let Some(text) = mod_kits {
            write(&mods.join("wells"), "kits.toml", text);
        }
        let roots = ContentRoots::base(&base)
            .unwrap()
            .with_mods(&mods, &["wells".to_owned()])
            .unwrap();
        (dir, roots)
    }

    #[test]
    fn the_shipped_start_kit_loads_with_three_full_magazines() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let kit = load(&roots, START_KIT_ID).expect("the shipped start kit loads");

        let loadout = &kit.loadout;
        assert!(loadout.rifle().is_chambered());
        assert_eq!(loadout.rifle().magazine().capacity(), 30);
        assert_eq!(loadout.rifle().magazine().rounds(), 29);
        assert_eq!(loadout.rig().slots().len(), 3);
        assert_eq!(loadout.rig().free_slots(), 0);
        assert_eq!(loadout.rig().magazines().count(), 3);
        assert!(
            loadout
                .rig()
                .magazines()
                .all(|m| m.capacity() == 30 && m.rounds() == 30)
        );
        assert_eq!(kit.weapon.cooldown_ticks, 6);
        assert_eq!(kit.weapon.reload_ticks, 120);
        assert_eq!(kit.weapon.charge_ticks, 30);
        assert_eq!(kit.weapon.well, MagazineWell::new("rifle"));
    }

    #[test]
    fn the_shipped_start_kit_carries_the_5_56_ball_round_ballistics() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let kit = load(&roots, START_KIT_ID).expect("the shipped start kit loads");

        let ball = kit
            .rounds
            .get(&ItemId::new("round_rifle_ball"))
            .expect("the start kit names the ball round");
        assert_eq!(ball.ballistics.muzzle_velocity, 940.0);
        assert_eq!(ball.ballistics.drag, 1.0);
        assert_eq!(ball.damage, 30);
        assert_eq!(kit.rounds.len(), 1);
    }

    #[test]
    fn a_weapon_that_still_names_damage_fails_the_load_naming_the_weapon_and_the_field() {
        let items = items_toml().replace(
            "well = \"rifle\"\nfire",
            "well = \"rifle\"\ndamage = 30\nfire",
        );
        let (dir, roots) = base_with(&items, &start_kit(2));

        let Err(error) = load(&roots, START_KIT_ID) else {
            panic!("a weapon that names damage loaded");
        };

        let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
            panic!("expected Invalid content, got {error:?}");
        };
        assert_eq!(file, &dir.path().join("items.toml"));
        assert_eq!(id, "rifle");
        assert!(reason.contains("damage"), "{reason}");
    }

    #[test]
    fn a_bad_barrel_factor_fails_the_load_naming_the_weapon_and_the_field() {
        for bad in ["0", "0.0", "-0.8", "nan", "inf", "-inf"] {
            let items = items_toml().replace(
                "charge_ticks = 30",
                &format!("charge_ticks = 30\nmuzzle_velocity_factor = {bad}"),
            );
            let (dir, roots) = base_with(&items, &start_kit(2));

            let Err(error) = load(&roots, START_KIT_ID) else {
                panic!("a weapon with muzzle_velocity_factor = {bad} loaded");
            };

            let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
                panic!("{bad}: expected Invalid content, got {error:?}");
            };
            assert_eq!(file, &dir.path().join("items.toml"));
            assert_eq!(id, "rifle");
            assert!(reason.contains("muzzle_velocity_factor"), "{bad}: {reason}");
        }
    }

    #[test]
    fn a_weapon_with_a_barrel_factor_loads_it_into_the_kit() {
        let items = items_toml().replace(
            "charge_ticks = 30",
            "charge_ticks = 30\nmuzzle_velocity_factor = 0.8",
        );
        let (_dir, roots) = base_with(&items, &start_kit(2));

        let kit = load(&roots, START_KIT_ID).expect("a 0.8 barrel loads");

        assert_eq!(kit.weapon.muzzle_velocity_factor.get(), 0.8);
        assert_eq!(tuning(kit).muzzle_velocity_factor.get(), 0.8);
    }

    #[test]
    fn a_bad_round_fails_the_load_naming_the_round_and_the_field() {
        let cases = [
            ("damage = 30", "damage = 0", "damage"),
            (", damage = 30", "", "damage"),
            (
                "muzzle_velocity = 940.0",
                "muzzle_velocity = 0.0",
                "muzzle_velocity",
            ),
            ("drag = 1.0", "drag = -1.0", "drag"),
        ];
        for (good, bad, field) in cases {
            let items = items_toml().replace(good, bad);
            let (dir, roots) = base_with(&items, &start_kit(2));

            let Err(error) = load(&roots, START_KIT_ID) else {
                panic!("a round with {bad} loaded");
            };

            let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
                panic!("{bad}: expected Invalid content, got {error:?}");
            };
            assert_eq!(file, &dir.path().join("items.toml"));
            assert_eq!(id, "round_ball");
            assert!(reason.contains(field), "{bad}: {reason}");
        }
    }

    #[test]
    fn a_non_finite_round_value_fails_the_load_naming_the_round_and_the_field() {
        let cases = [
            (
                "muzzle_velocity = 940.0",
                "muzzle_velocity = nan",
                "muzzle_velocity",
            ),
            (
                "muzzle_velocity = 940.0",
                "muzzle_velocity = inf",
                "muzzle_velocity",
            ),
            ("drag = 1.0", "drag = nan", "drag"),
            ("drag = 1.0", "drag = inf", "drag"),
        ];
        for (good, bad, field) in cases {
            let items = items_toml().replace(good, bad);
            let (dir, roots) = base_with(&items, &start_kit(2));

            let Err(error) = load(&roots, START_KIT_ID) else {
                panic!("a round with {bad} loaded");
            };

            let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
                panic!("{bad}: expected Invalid content, got {error:?}");
            };
            assert_eq!(file, &dir.path().join("items.toml"));
            assert_eq!(id, "round_ball");
            assert!(reason.contains(field), "{bad}: {reason}");
        }
    }

    #[test]
    fn the_shipped_tool_resolves_by_id() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let items = load_kind::<ItemDef>(&roots, "items").expect("the shipped items.toml loads");
        let tool = items
            .require("crowbar")
            .expect("the shipped items define a tool");

        assert_eq!(tool.class, ItemClass::Tool);
        assert!(matches!(tool.kind, ItemKind::Plain), "{:?}", tool.kind);
        for id in ["rifle", "mag_rifle", "round_rifle_ball", "rig_chest"] {
            assert!(items.get(id).is_some(), "the shipped items lack {id}");
        }
    }

    #[test]
    fn an_overfilled_start_kit_fails_naming_the_kit() {
        let (_dir, roots) = base_with(&items_toml(), &start_kit(4));

        let Err(error) = load(&roots, START_KIT_ID) else {
            panic!("a kit with four magazines on a three-slot rig loaded");
        };

        let text = error.to_string();
        assert!(text.contains("start"), "{text}");
        assert!(text.starts_with("kit \"start\": "), "{text}");
        assert!(
            matches!(
                &error,
                KitLoadError::Kit { kit, source: KitError::RigOverfilled { .. } } if kit == "start"
            ),
            "{error:?}"
        );
    }

    #[test]
    fn an_item_with_condition_and_uses_fails_naming_file_and_id() {
        let cases = [
            (
                items_toml().replace(
                    "[crowbar]\nclass = \"tool\"\nsize = [1, 3]\ncondition = 100",
                    "[crowbar]\nclass = \"tool\"\nsize = [1, 3]\ncondition = 100\nuses = 3",
                ),
                "crowbar",
                "uses",
            ),
            (items_toml().replace("semi_auto", "burst"), "rifle", "burst"),
        ];
        for (items, offender, mention) in cases {
            let (dir, roots) = base_with(&items, &start_kit(2));

            let Err(error) = load(&roots, START_KIT_ID) else {
                panic!("a malformed {offender} loaded");
            };

            let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
                panic!("{offender}: expected Invalid content, got {error:?}");
            };
            assert_eq!(file, &dir.path().join("items.toml"));
            assert_eq!(id, offender);
            assert!(reason.contains(mention), "{offender}: {reason}");
        }
    }

    #[test]
    fn an_unknown_kit_id_is_a_missing_id_error() {
        let (_dir, roots) = base_with(&items_toml(), &start_kit(2));

        let Err(error) = load(&roots, "nope") else {
            panic!("a kit that is not defined loaded");
        };

        assert!(
            matches!(&error, KitLoadError::Content(ContentError::MissingId { id, .. }) if id == "nope"),
            "{error:?}"
        );
    }

    #[test]
    fn a_kit_naming_an_undefined_item_fails_naming_the_kit_and_the_item() {
        let kits = start_kit(2).replace("rig_chest", "rig_missing");
        let (_dir, roots) = base_with(&items_toml(), &kits);

        let Err(error) = load(&roots, START_KIT_ID) else {
            panic!("a kit with an undefined rig loaded");
        };

        let text = error.to_string();
        assert!(text.contains("start"), "{text}");
        assert!(text.contains("rig_missing"), "{text}");
        assert!(
            matches!(
                &error,
                KitLoadError::Kit { source: KitError::UnknownItem { id }, .. }
                    if *id == ItemId::new("rig_missing")
            ),
            "{error:?}"
        );
    }

    const MOD_MAGAZINE: &str = r#"
[mag_rifle_20]
class = "gear"
size = [1, 2]
magazine = { well = "rifle", caliber = "5.56x45", capacity = 20, slot = "rifle" }
weight = 0.2
"#;

    #[test]
    fn a_mod_magazine_in_the_start_kit_empties_after_20_shots() {
        let mod_kit = "[start]\nrig = \"rig_chest\"\nweapon = \"rifle\"\nloaded = { magazine = \"mag_rifle\", round = \"round_ball\", rounds = 0 }\nmagazines = [{ magazine = \"mag_rifle_20\", round = \"round_ball\" }]\n";
        let (_dir, roots) = with_mod(
            &items_toml(),
            &start_kit(2),
            Some(MOD_MAGAZINE),
            Some(mod_kit),
        );
        let mut kit = load(&roots, START_KIT_ID).expect("the modded start kit loads");
        assert_eq!(kit.loadout.fire(), Shot::Dry);

        assert!(kit.loadout.reload());

        let shots: Vec<Shot> = (0..21).map(|_| kit.loadout.fire()).collect();
        let (fired, dry) = shots.split_at(20);
        assert!(
            fired
                .iter()
                .all(|shot| *shot == Shot::Fired(ItemId::new("round_ball"))),
            "{shots:?}"
        );
        assert_eq!(dry, [Shot::Dry]);
    }

    const LOOT_ITEMS: &str = r#"
[round_heavy]
class = "consumable"
size = [1, 1]
weight = 0.02
round = { caliber = "5.56x45", muzzle_velocity = 800.0, drag = 2.5, damage = 55 }

[bandage]
class = "consumable"
size = [1, 1]
weight = 0.05
uses = 1

[backpack_small]
class = "gear"
size = [4, 4]
weight = 0.9
backpack = {}
"#;

    const ENEMY_KIT: &str = r#"
[enemy]
rig = "rig_chest"
weapon = "rifle"
loaded = { magazine = "mag_rifle", round = "round_ball" }
magazines = [{ magazine = "mag_rifle", round = "round_heavy", rounds = 12 }]
carried = [{ item = "bandage" }]
"#;

    fn loot_content(kits: &str) -> (tempfile::TempDir, ContentRoots) {
        base_with(&format!("{}{LOOT_ITEMS}", items_toml()), kits)
    }

    #[test]
    fn the_shipped_start_kit_wears_the_small_backpack_and_carries_nothing_else() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let kit = load(&roots, START_KIT_ID).expect("the shipped start kit loads");

        assert_eq!(kit.loadout.backpack(), Some(&ItemId::new("backpack_small")));
        assert!(kit.loadout.inventory().is_empty());
    }

    #[test]
    fn the_shipped_enemy_kit_loads() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let (remains, rounds) =
            load_enemy(&roots, ENEMY_KIT_ID).expect("the shipped enemy kit loads");

        let rig = remains.rig.as_ref().expect("the enemy wears a rig");
        assert_eq!(rig.item(), &ItemId::new("rig_chest"));
        assert!(
            matches!(remains.carried.first(), Some(Item::Weapon(_))),
            "the rifle comes first: {:?}",
            remains.carried
        );
        let ball = rounds
            .get(&ItemId::new("round_rifle_ball"))
            .expect("the enemy kit names the ball round");
        assert_eq!(ball.ballistics.muzzle_velocity, 940.0);
        assert_eq!(ball.ballistics.drag, 1.0);
        assert_eq!(ball.damage, 30);
    }

    const CARBINE_AND_SLOW_ROUND: &str = r#"
[carbine]
class = "weapon"
size = [2, 4]
condition = 100
weight = 2.5
[carbine.weapon]
well = "rifle"
fire = "semi_auto"
cooldown_ticks = 6
reload_ticks = 120
charge_ticks = 30

[round_slow]
class = "consumable"
size = [1, 1]
weight = 0.01
round = { caliber = "5.56x45", muzzle_velocity = 150.0, drag = 1.0, damage = 20 }
"#;

    /// A player rifle with a 0.5 barrel and an enemy carbine with the default barrel that
    /// holds `enemy_round`.
    fn looting_content(enemy_round: &str) -> (tempfile::TempDir, ContentRoots) {
        let items = items_toml().replace(
            "charge_ticks = 30",
            "charge_ticks = 30\nmuzzle_velocity_factor = 0.5",
        );
        let enemy = format!(
            "[enemy]\nrig = \"rig_chest\"\nweapon = \"carbine\"\nloaded = {{ magazine = \"mag_rifle\", round = \"{enemy_round}\" }}\n"
        );
        base_with(
            &format!("{items}{CARBINE_AND_SLOW_ROUND}"),
            &format!("{}{enemy}", start_kit(2)),
        )
    }

    #[test]
    fn a_looted_round_the_players_barrel_slows_to_the_minimum_speed_is_refused() {
        let (_dir, roots) = looting_content("round_slow");

        let Err(error) = load_start_and_enemy(&roots, START_KIT_ID, ENEMY_KIT_ID) else {
            panic!("a 150 m/s round from a 0.5 barrel loaded");
        };

        let KitLoadError::Kit { kit, source } = &error else {
            panic!("expected a kit error, got {error:?}");
        };
        assert_eq!(kit, ENEMY_KIT_ID);
        assert!(
            matches!(
                source,
                KitError::TooSlowFromWeapon { weapon, round }
                    if *weapon == ItemId::new("rifle") && *round == ItemId::new("round_slow")
            ),
            "{source:?}"
        );
    }

    #[test]
    fn a_looted_round_the_players_barrel_overflows_is_refused() {
        let items = items_toml().replace(
            "charge_ticks = 30",
            "charge_ticks = 30\nmuzzle_velocity_factor = 1e10",
        );
        let huge = "\n[round_huge]\nclass = \"consumable\"\nsize = [1, 1]\nweight = 0.01\nround = { caliber = \"5.56x45\", muzzle_velocity = 1e30, drag = 1.0, damage = 20 }\n";
        let enemy = "[enemy]\nrig = \"rig_chest\"\nweapon = \"carbine\"\nloaded = { magazine = \"mag_rifle\", round = \"round_huge\" }\n";
        let (_dir, roots) = base_with(
            &format!("{items}{CARBINE_AND_SLOW_ROUND}{huge}"),
            &format!("{}{enemy}", start_kit(2)),
        );

        let Err(error) = load_start_and_enemy(&roots, START_KIT_ID, ENEMY_KIT_ID) else {
            panic!("a round that overflows the player's barrel loaded");
        };

        let KitLoadError::Kit { kit, source } = &error else {
            panic!("expected a kit error, got {error:?}");
        };
        assert_eq!(kit, ENEMY_KIT_ID);
        assert!(
            matches!(
                source,
                KitError::NotFiniteFromWeapon { weapon, round }
                    if *weapon == ItemId::new("rifle") && *round == ItemId::new("round_huge")
            ),
            "{source:?}"
        );
    }

    #[test]
    fn a_looted_round_fast_enough_for_the_players_barrel_is_accepted() {
        let (_dir, roots) = looting_content("round_ball");

        let (catalogue, remains, rounds) =
            load_start_and_enemy(&roots, START_KIT_ID, ENEMY_KIT_ID).expect("both kits load");

        assert_eq!(catalogue.kit.weapon.muzzle_velocity_factor.get(), 0.5);
        assert!(matches!(remains.carried.first(), Some(Item::Weapon(_))));
        assert!(rounds.contains_key(&ItemId::new("round_ball")));
    }

    #[test]
    fn the_enemy_kits_rounds_join_the_ballistics_map() {
        let start = start_kit(2).replace("round_ball", "round_heavy");
        let enemy = ENEMY_KIT.replace("round_heavy", "round_ball");
        let (_dir, roots) = loot_content(&format!("{start}{enemy}"));
        let kit = load(&roots, START_KIT_ID).expect("the start kit loads");
        let (remains, rounds) = load_enemy(&roots, ENEMY_KIT_ID).expect("the enemy kit loads");

        let config = with_enemy_kit(tuning(kit), remains.clone(), rounds);

        let expected = BTreeMap::from([
            (
                ItemId::new("round_ball"),
                RoundStats {
                    ballistics: Ballistics {
                        muzzle_velocity: 940.0,
                        drag: 1.0,
                    },
                    damage: 30,
                },
            ),
            (
                ItemId::new("round_heavy"),
                RoundStats {
                    ballistics: Ballistics {
                        muzzle_velocity: 800.0,
                        drag: 2.5,
                    },
                    damage: 55,
                },
            ),
        ]);
        assert_eq!(config.rounds, expected);
        assert_eq!(config.enemy_remains, remains);
    }

    #[test]
    fn an_undefined_enemy_kit_is_a_missing_id_error() {
        let (_dir, roots) = loot_content(&start_kit(2));

        let Err(error) = load_enemy(&roots, ENEMY_KIT_ID) else {
            panic!("an enemy kit that is not defined loaded");
        };

        assert!(
            matches!(&error, KitLoadError::Content(ContentError::MissingId { id, .. }) if id == "enemy"),
            "{error:?}"
        );
    }

    #[test]
    fn an_enemy_kit_with_a_misfitting_carried_entry_fails_naming_the_kit_and_the_item() {
        let kits = format!(
            "{}{}",
            start_kit(2),
            ENEMY_KIT.replace(
                "{ item = \"bandage\" }",
                "{ item = \"bandage\", count = 3 }"
            )
        );
        let (_dir, roots) = loot_content(&kits);

        let Err(error) = load_enemy(&roots, ENEMY_KIT_ID) else {
            panic!("a bandage with a count loaded");
        };

        let text = error.to_string();
        assert!(text.starts_with("kit \"enemy\": "), "{text}");
        assert!(text.contains("bandage"), "{text}");
        assert!(
            matches!(
                &error,
                KitLoadError::Kit { kit, source: KitError::EntryFields { id } }
                    if kit == "enemy" && *id == ItemId::new("bandage")
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_missing_negative_or_non_finite_weight_fails_the_load() {
        for bad in ["missing", "-1.0", "nan", "inf", "-inf"] {
            let weight = if bad == "missing" {
                String::new()
            } else {
                format!("weight = {bad}\n")
            };
            let items = items_toml().replace("weight = 2.0\n", &weight);
            let (dir, roots) = base_with(&items, &start_kit(2));

            let Err(error) = load(&roots, START_KIT_ID) else {
                panic!("a weight of {bad} loaded");
            };

            let KitLoadError::Content(ContentError::Invalid { file, id, reason }) = &error else {
                panic!("{bad}: expected Invalid content, got {error:?}");
            };
            assert_eq!(file, &dir.path().join("items.toml"));
            assert_eq!(id, "crowbar");
            assert!(reason.contains("weight"), "{bad}: {reason}");
        }
    }
}
