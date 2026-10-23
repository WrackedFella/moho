//! The simulation's tuning, with the rifle's numbers taken from its definition.

use moho_physics::MoveSettings;
use searchlight_rules::{BodyPart, BodyProfile, Magazine, MagazineError, Rifle, RifleDef};

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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_root;
    use crate::kit::{START_KIT_ID, load};
    use crate::test_support::{mag, rifle_from};
    use moho_core::content::ContentRoots;
    use searchlight_rules::{
        FireBehaviour, ItemId, Kit, Loadout, MagazineWell, RifleDef, Rig, RigSpec,
    };

    #[test]
    fn every_rifle_number_comes_from_the_kits_weapon() {
        let weapon = RifleDef {
            well: MagazineWell::new("test"),
            damage: 41,
            fire: FireBehaviour::SemiAuto,
            cooldown_ticks: 9,
            reload_ticks: 77,
            charge_ticks: 13,
            range: 55.5,
        };
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 2 });
        rig.stow(mag(17, 17)).unwrap();
        let loadout = Loadout::new(rifle_from(mag(17, 17)), rig);
        let kit = Kit {
            weapon,
            loadout: loadout.clone(),
        };

        let config = tuning(kit);

        assert_eq!(config.damage, 41);
        assert_eq!(config.cooldown_ticks, 9);
        assert_eq!(config.reload_ticks, 77);
        assert_eq!(config.charge_ticks, 13);
        assert!((config.range - 55.5).abs() < f32::EPSILON);
        assert_eq!(config.loadout, loadout);
    }

    #[test]
    fn the_shipped_kit_matches_todays_tuning() {
        let roots = ContentRoots::base(&content_root()).expect("the shipped content folder");
        let kit = load(&roots, START_KIT_ID).expect("the shipped start kit loads");

        let config = tuning(kit);

        assert_eq!(config.loadout.rifle().magazine().capacity(), 30);
        assert!(config.loadout.rifle().is_chambered());
        assert_eq!(config.loadout.rig().magazines().count(), 3);
        assert_eq!(config.damage, 30);
        assert_eq!(config.cooldown_ticks, 6);
        assert_eq!(config.reload_ticks, 120);
        assert_eq!(config.charge_ticks, 30);
        assert!((config.range - 100.0).abs() < f32::EPSILON);
    }
}
