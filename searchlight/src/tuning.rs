//! The simulation's tuning, with the rifle's numbers taken from its definition.

use searchlight_rules::{MagazineError, RifleDef};

use crate::sim::SimConfig;

/// Builds the simulation tuning around `rifle`.
///
/// # Errors
///
/// [`MagazineError`] when the definition's magazine cannot be built.
pub fn tuning(rifle: &RifleDef) -> Result<SimConfig, MagazineError> {
    let _ = rifle;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RIFLE_ID, content_root};
    use moho_core::content::{ContentError, ContentRoots, load_kind};
    use searchlight_rules::FireBehaviour;

    fn load_rifle(weapons: &str) -> (tempfile::TempDir, Result<RifleDef, ContentError>) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("weapons.toml"), weapons).unwrap();
        let roots = ContentRoots::base(dir.path()).unwrap();
        let rifle = load_kind::<RifleDef>(&roots, "weapons")
            .and_then(|definitions| definitions.require(RIFLE_ID).cloned());
        (dir, rifle)
    }

    const CAPACITY_20: &str = "[rifle]\nmagazine_capacity = 20\ndamage = 30\nfire = \"semi_auto\"\ncooldown_ticks = 6\nreload_ticks = 120\nrange = 100.0\n";

    #[test]
    fn a_rifle_defined_with_capacity_20_gives_full_magazines_of_20() {
        let (_dir, rifle) = load_rifle(CAPACITY_20);
        let rifle = rifle.unwrap();

        let config = tuning(&rifle).unwrap();

        assert_eq!(config.rifle.magazine().capacity(), 20);
        assert_eq!(config.rifle.magazine().rounds(), 20);
        assert!(!config.spares.is_empty());
        for spare in &config.spares {
            assert_eq!(spare.capacity(), 20);
            assert_eq!(spare.rounds(), 20);
        }
    }

    #[test]
    fn every_rifle_number_comes_from_the_definition() {
        let def = RifleDef {
            magazine_capacity: 17,
            damage: 41,
            fire: FireBehaviour::SemiAuto,
            cooldown_ticks: 9,
            reload_ticks: 77,
            range: 55.5,
        };

        let config = tuning(&def).unwrap();

        assert_eq!(config.damage, 41);
        assert_eq!(config.cooldown_ticks, 9);
        assert_eq!(config.reload_ticks, 77);
        assert!((config.range - 55.5).abs() < f32::EPSILON);
        assert_eq!(config.rifle.magazine().capacity(), 17);
        assert_eq!(config.rifle.magazine().rounds(), 17);
        assert_eq!(config.spares.len(), 3);
        for spare in &config.spares {
            assert_eq!(spare.capacity(), 17);
            assert_eq!(spare.rounds(), 17);
        }
    }

    #[test]
    fn a_rifle_with_an_unknown_fire_behaviour_stops_load_naming_weapons_toml_and_rifle() {
        let (dir, rifle) = load_rifle(&CAPACITY_20.replace("semi_auto", "burst"));

        let error = rifle.unwrap_err();

        assert!(matches!(error, ContentError::Invalid { .. }), "{error:?}");
        let text = error.to_string();
        let file = dir.path().join("weapons.toml").display().to_string();
        assert!(text.contains(&file), "{text}");
        assert!(text.contains("rifle"), "{text}");
        assert!(text.contains("burst"), "{text}");
    }

    #[test]
    fn the_shipped_rifle_matches_todays_tuning() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");
        let rifle = load_kind::<RifleDef>(&roots, "weapons")
            .expect("the shipped weapons.toml loads")
            .require(RIFLE_ID)
            .expect("the shipped file defines the rifle")
            .clone();

        let config = tuning(&rifle).unwrap();

        assert_eq!(config.rifle.magazine().capacity(), 30);
        assert_eq!(config.damage, 30);
        assert_eq!(config.cooldown_ticks, 6);
        assert_eq!(config.reload_ticks, 120);
        assert!((config.range - 100.0).abs() < f32::EPSILON);
    }
}
