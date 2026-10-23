//! The simulation's tuning, with the rifle's numbers taken from its definition.

use moho_physics::MoveSettings;
use searchlight_rules::{
    BodyPart, BodyProfile, LeanTuning, Magazine, MagazineError, Rifle, RifleDef,
};

use crate::brain::EnemyTuning;
use crate::sim::SimConfig;

const SPARE_MAGAZINES: usize = 3;

/// Builds the simulation tuning around `rifle`.
///
/// # Errors
///
/// [`MagazineError`] when the definition's magazine cannot be built.
pub fn tuning(rifle: &RifleDef) -> Result<SimConfig, MagazineError> {
    let full = Magazine::new(rifle.magazine_capacity, rifle.magazine_capacity)?;
    Ok(SimConfig {
        move_settings: MoveSettings {
            max_speed: 6.0,
            ground_accel: 80.0,
            friction: 10.0,
            stop_speed: 1.0,
            air_accel: 20.0,
            air_speed_cap: 1.0,
            gravity: 18.0,
            jump_speed: 6.5,
        },
        eye_height: 0.7,
        range: rifle.range,
        damage: rifle.damage,
        cooldown_ticks: rifle.cooldown_ticks,
        reload_ticks: rifle.reload_ticks,
        charge_ticks: rifle.charge_ticks,
        body: BodyProfile {
            max_health: 100,
            head_status: 30,
            arm_status: 60,
            leg_status: 60,
        },
        rifle: Rifle::loaded(full.clone()),
        spares: vec![full; SPARE_MAGAZINES],
        enemy: EnemyTuning::default(),
        enemy_aim_at: BodyPart::Torso,
        lean: LeanTuning {
            shift: 0.35,
            roll: 12.0_f32.to_radians(),
            blend_seconds: 0.25,
            speed_factor: 0.5,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RIFLE_ID, content_root};
    use moho_core::content::{ContentError, ContentRoots, load_kind};
    use moho_core::prefs::Prefs;
    use searchlight_rules::FireBehaviour;

    fn load_rifle(weapons: &str) -> (tempfile::TempDir, Result<RifleDef, ContentError>) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("weapons.toml"), weapons).unwrap();
        let roots = ContentRoots::base(dir.path()).unwrap();
        let rifle = load_kind::<RifleDef>(&roots, "weapons")
            .and_then(|definitions| definitions.require(RIFLE_ID).cloned());
        (dir, rifle)
    }

    const CAPACITY_20: &str = "[rifle]\nmagazine_capacity = 20\ndamage = 30\nfire = \"semi_auto\"\ncooldown_ticks = 6\nreload_ticks = 120\ncharge_ticks = 30\nrange = 100.0\n";

    #[test]
    fn a_rifle_defined_with_capacity_20_gives_full_magazines_of_20() {
        let (_dir, rifle) = load_rifle(CAPACITY_20);
        let rifle = rifle.unwrap();

        let config = tuning(&rifle).unwrap();

        assert_eq!(config.rifle.magazine().capacity(), 20);
        assert!(config.rifle.is_chambered());
        assert_eq!(config.rifle.magazine().rounds(), 19);
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
            charge_ticks: 13,
            range: 55.5,
        };

        let config = tuning(&def).unwrap();

        assert_eq!(config.damage, 41);
        assert_eq!(config.cooldown_ticks, 9);
        assert_eq!(config.reload_ticks, 77);
        assert_eq!(config.charge_ticks, 13);
        assert!((config.range - 55.5).abs() < f32::EPSILON);
        assert_eq!(config.rifle.magazine().capacity(), 17);
        assert!(config.rifle.is_chambered());
        assert_eq!(config.rifle.magazine().rounds(), 16);
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
        assert_eq!(config.charge_ticks, 30);
        assert!((config.range - 100.0).abs() < f32::EPSILON);
    }

    fn rifle_toml(capacity: u32, damage: u32) -> String {
        format!(
            "[rifle]\nmagazine_capacity = {capacity}\ndamage = {damage}\nfire = \"semi_auto\"\ncooldown_ticks = 6\nreload_ticks = 120\ncharge_ticks = 30\nrange = 100.0\n"
        )
    }

    fn write_root(dir: &std::path::Path, weapons: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("weapons.toml"), weapons).unwrap();
    }

    fn rifle_with_prefs(
        base: &std::path::Path,
        mods: &std::path::Path,
        prefs_text: &str,
    ) -> Result<RifleDef, ContentError> {
        let (prefs, issues) = Prefs::parse(prefs_text);
        assert_eq!(issues, vec![]);
        let roots = ContentRoots::base(base)?.with_mods(mods, prefs.content_mods())?;
        load_kind::<RifleDef>(&roots, "weapons")
            .and_then(|definitions| definitions.require(RIFLE_ID).cloned())
    }

    #[test]
    fn mod_heavy_raises_capacity_to_40() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("content");
        let mods = dir.path().join("mods");
        write_root(&base, &rifle_toml(30, 30));
        write_root(&mods.join("heavy"), &rifle_toml(40, 30));

        let rifle = rifle_with_prefs(&base, &mods, "[content]\nmods = heavy\n").unwrap();

        let config = tuning(&rifle).unwrap();
        assert_eq!(config.rifle.magazine().capacity(), 40);
        for spare in &config.spares {
            assert_eq!(spare.capacity(), 40);
            assert_eq!(spare.rounds(), 40);
        }
    }

    #[test]
    fn a_mod_rifle_missing_a_field_does_not_inherit_it() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("content");
        let mods = dir.path().join("mods");
        write_root(&base, &rifle_toml(30, 30));
        write_root(
            &mods.join("heavy"),
            "[rifle]\ndamage = 30\nfire = \"semi_auto\"\ncooldown_ticks = 6\nreload_ticks = 120\ncharge_ticks = 30\nrange = 100.0\n",
        );

        let error = rifle_with_prefs(&base, &mods, "[content]\nmods = heavy\n").unwrap_err();

        let ContentError::Invalid { file, id, reason } = &error else {
            panic!("expected Invalid, got {error:?}");
        };
        assert_eq!(file, &mods.join("heavy").join("weapons.toml"));
        assert_eq!(id, "rifle");
        assert!(reason.contains("magazine_capacity"), "{reason}");
    }
}
