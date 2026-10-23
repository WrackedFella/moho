//! Loads the starting kit from item and kit definitions in the content roots.

use moho_core::content::{ContentError, ContentRoots, load_kind};
use searchlight_rules::{ItemDef, ItemId, Kit, KitDef, KitError};
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

/// Loads `items` and `kits` through `roots` and resolves the kit `kit`.
///
/// # Errors
///
/// [`KitLoadError::Content`] when a file or entry is malformed or `kit` is undefined;
/// [`KitLoadError::Kit`] when the kit names an unknown or unsuitable item.
pub fn load(roots: &ContentRoots, kit: &str) -> Result<Kit, KitLoadError> {
    let items = load_kind::<ItemDef>(roots, "items")?;
    let kits = load_kind::<KitDef>(roots, "kits")?;
    kits.require(kit)?
        .resolve(|id: &ItemId| items.get(id.as_str()))
        .map_err(|source| KitLoadError::Kit {
            kit: kit.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_root;
    use moho_core::content::{ContentError, ContentRoots, load_kind};
    use searchlight_rules::{ItemClass, ItemDef, ItemId, ItemKind, KitError, MagazineWell, Shot};
    use std::path::Path;

    const RIFLE: &str = r#"
[rifle]
class = "weapon"
size = [2, 5]
condition = 100
[rifle.weapon]
well = "rifle"
damage = 30
fire = "semi_auto"
cooldown_ticks = 6
reload_ticks = 120
charge_ticks = 30
range = 100.0
"#;

    const OTHER_ITEMS: &str = r#"
[mag_rifle]
class = "gear"
size = [1, 2]
magazine = { well = "rifle", caliber = "5.56x45", capacity = 30 }

[round_ball]
class = "consumable"
size = [1, 1]
round = { caliber = "5.56x45" }

[rig_chest]
class = "gear"
size = [3, 2]
rig = { magazine_slots = 3 }

[crowbar]
class = "tool"
size = [1, 3]
condition = 100
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
        assert_eq!(kit.weapon.damage, 30);
        assert_eq!(kit.weapon.cooldown_ticks, 6);
        assert_eq!(kit.weapon.reload_ticks, 120);
        assert_eq!(kit.weapon.charge_ticks, 30);
        assert!((kit.weapon.range - 100.0).abs() < f32::EPSILON);
        assert_eq!(kit.weapon.well, MagazineWell::new("rifle"));
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
magazine = { well = "rifle", caliber = "5.56x45", capacity = 20 }
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
        assert!(fired.iter().all(|shot| *shot == Shot::Fired), "{shots:?}");
        assert_eq!(dry, [Shot::Dry]);
    }
}
