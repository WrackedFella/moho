//! The simulation's tuning, with the rifle's numbers taken from its definition.

use moho_physics::MoveSettings;
use searchlight_rules::{BodyPart, BodyProfile, Kit};

use crate::brain::EnemyTuning;
use crate::sim::SimConfig;

/// Builds the simulation tuning around the starting `kit`: its weapon's numbers and its loadout.
pub fn tuning(kit: Kit) -> SimConfig {
    let Kit { weapon, loadout } = kit;
    SimConfig {
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
        range: weapon.range,
        damage: weapon.damage,
        cooldown_ticks: weapon.cooldown_ticks,
        reload_ticks: weapon.reload_ticks,
        charge_ticks: weapon.charge_ticks,
        body: BodyProfile {
            max_health: 100,
            head_status: 30,
            arm_status: 60,
            leg_status: 60,
        },
        loadout,
        enemy: EnemyTuning::default(),
        enemy_aim_at: BodyPart::Torso,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{mag, rifle_from};
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
}
