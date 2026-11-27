//! Loads the player's attribute sheet from the content roots.

use moho_core::content::{ContentError, ContentRoots};
use searchlight_rules::PlayerDef;

/// The id of the player's sheet in `player.toml`.
pub const PLAYER_ID: &str = "player";

/// Loads the `player` table of `player.toml` through `roots`.
///
/// # Errors
///
/// A [`ContentError`] naming the file and id when the table is missing or malformed.
pub fn load(_roots: &ContentRoots) -> Result<PlayerDef, ContentError> {
    todo!("load_kind the player sheet and require the player id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_root;
    use moho_core::content::load_kind;
    use searchlight_rules::{ItemDef, ItemId, ItemStat, LoadThresholds, Stat, Verb};

    fn roots_with(player: Option<&str>) -> (tempfile::TempDir, ContentRoots) {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = player {
            std::fs::write(dir.path().join("player.toml"), text).unwrap();
        }
        let roots = ContentRoots::base(dir.path()).unwrap();
        (dir, roots)
    }

    const SHEET: &str = r#"
[player]
strength = 10.0
carry_capacity_per_strength = 3.0
movement_speed = 1.0
stamina_pool = 5.0
slowed_scale = 0.7
[player.thresholds]
slowed = 0.6
no_sprint = 0.8
cannot_move = 1.0
"#;

    #[test]
    fn the_shipped_player_sheet_gives_30_kg_of_capacity_and_the_outline_thresholds() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let player = load(&roots).expect("the shipped player sheet loads");

        assert_eq!(player.strength, 10.0);
        assert_eq!(player.carry_capacity_per_strength, 3.0);
        assert_eq!(player.movement_speed, 1.0);
        assert_eq!(player.slowed_scale, 0.7);
        assert_eq!(
            player.thresholds,
            LoadThresholds {
                slowed: 0.6,
                no_sprint: 0.8,
                cannot_move: 1.0
            }
        );
        assert_eq!(player.baseline(Stat::CarryCapacity), 30.0);
    }

    #[test]
    fn a_sheet_in_a_content_folder_loads() {
        let (_dir, roots) = roots_with(Some(SHEET));

        let player = load(&roots).expect("a valid sheet loads");

        assert_eq!(player.stamina_pool, 5.0);
    }

    #[test]
    fn a_sheet_with_an_unknown_key_is_refused_naming_it() {
        let sheet = SHEET.replace("slowed_scale = 0.7", "slowed_scale = 0.7\nbogus = 1");
        let (_dir, roots) = roots_with(Some(&sheet));

        let error = load(&roots).expect_err("an unknown key is refused");

        assert!(error.to_string().contains("bogus"), "{error}");
    }

    #[test]
    fn a_folder_with_no_player_table_is_an_error_naming_the_id() {
        let (_dir, roots) = roots_with(None);

        let error = load(&roots).expect_err("no sheet, no player");

        assert!(error.to_string().contains(PLAYER_ID), "{error}");
    }

    #[test]
    fn the_shipped_backpack_adds_to_carry_capacity() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");
        let items = load_kind::<ItemDef>(&roots, "items").expect("the shipped items load");

        let pack = items
            .get(ItemId::new("backpack_small").as_str())
            .expect("the shipped content has a small backpack");

        assert!(
            pack.stats.iter().any(|entry| matches!(
                entry,
                ItemStat {
                    stat: Stat::CarryCapacity,
                    verb: Verb::Add(kg)
                } if *kg > 0.0
            )),
            "{:?}",
            pack.stats
        );
    }
}
