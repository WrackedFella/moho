//! Loads the player's attribute sheet from the content roots.

use moho_core::content::{ContentError, ContentRoots, load_kind};
use searchlight_rules::PlayerDef;

/// The id of the player's sheet in `player.toml`.
pub const PLAYER_ID: &str = "player";

/// Loads the `player` table of `player.toml` through `roots`.
///
/// # Errors
///
/// A [`ContentError`] naming the file and id when the table is missing or malformed.
pub fn load(roots: &ContentRoots) -> Result<PlayerDef, ContentError> {
    let sheets = load_kind::<PlayerDef>(roots, "player")?;
    sheets.require(PLAYER_ID).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_root;
    use crate::tuning::SHIPPED_PLAYER;
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

    const SHEET: &str = r"
[player]
strength = 8.0
carry_capacity_per_strength = 2.5
movement_speed = 1.2
stamina_pool = 7.0
slowed_scale = 0.55
[player.thresholds]
slowed = 0.5
no_sprint = 0.7
cannot_move = 0.9
";

    #[test]
    fn the_shipped_player_sheet_matches_the_shipped_tuning_constant() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");

        let player = load(&roots).expect("the shipped player sheet loads");

        assert_eq!(player, SHIPPED_PLAYER);
    }

    #[test]
    fn a_sheet_in_a_content_folder_loads_whole() {
        let (_dir, roots) = roots_with(Some(SHEET));

        let player = load(&roots).expect("a valid sheet loads");

        assert_eq!(
            player,
            PlayerDef {
                strength: 8.0,
                carry_capacity_per_strength: 2.5,
                movement_speed: 1.2,
                stamina_pool: 7.0,
                slowed_scale: 0.55,
                thresholds: LoadThresholds {
                    slowed: 0.5,
                    no_sprint: 0.7,
                    cannot_move: 0.9,
                },
            }
        );
    }

    #[test]
    fn a_sheet_with_an_unknown_key_is_refused_naming_it() {
        let sheet = SHEET.replace("slowed_scale = 0.55", "slowed_scale = 0.55\nbogus = 1");
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
