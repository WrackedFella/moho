//! The simulation's tuning, with the rifle's numbers taken from its definition.

use moho_physics::{CharacterBody, Hull, HullShape, MoveSettings, Stature};
use searchlight_rules::{BodyPart, BodyProfile, Kit, LeanTuning};

use crate::brain::EnemyTuning;
use crate::sim::SimConfig;

/// The player's and every target's body: Source's 0.81 m wide, 1.83 m tall capsule, eye at
/// 1.63 m.
pub const SHIPPED_BODY: CharacterBody = CharacterBody {
    standing: Stature {
        hull: Hull {
            shape: HullShape::Capsule,
            radius: 0.405,
            length: 1.83,
        },
        eye_height: 1.63,
    },
};

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
            gravity: None,
            jump_speed: 6.5,
            max_velocity: 90.0,
            autohop: false,
        },
        gravity: 18.0,
        character_body: SHIPPED_BODY,
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
        lean: LeanTuning {
            shift: 0.35,
            roll: 12.0_f32.to_radians(),
            blend_seconds: 0.25,
            speed_factor: 0.5,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::SearchlightCommand;
    use crate::sim::SearchlightSim;
    use crate::test_support::{DT, arena_with, mag, rifle_from};
    use glam::Vec3;
    use moho_physics::{Hull, HullShape, Stature};
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

    fn shipped_config() -> SimConfig {
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 1 });
        rig.stow(mag(17, 17)).unwrap();
        tuning(Kit {
            weapon: RifleDef {
                well: MagazineWell::new("test"),
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                range: 100.0,
            },
            loadout: Loadout::new(rifle_from(mag(17, 17)), rig),
        })
    }

    #[test]
    fn the_shipped_body_is_sources_0_81_m_wide_1_83_m_tall_capsule_with_its_eye_at_1_63_m() {
        let config = shipped_config();

        assert_eq!(
            config.character_body,
            CharacterBody {
                standing: Stature {
                    hull: Hull {
                        shape: HullShape::Capsule,
                        radius: 0.405,
                        length: 1.83,
                    },
                    eye_height: 1.63,
                },
            }
        );
    }

    #[test]
    fn the_player_and_a_target_settle_on_a_flat_floor_at_the_shipped_heights() {
        let arena = arena_with(
            &[],
            Vec3::new(0.0, 0.0, -10.0),
            &[Vec3::new(0.0, 0.0, 10.0)],
            &[Vec3::new(0.0, 0.0, 6.0)],
        );
        let mut sim = SearchlightSim::start(arena, shipped_config());

        for _ in 0..90 {
            sim.tick(&SearchlightCommand::default(), DT);
        }

        let player = sim.player();
        let target = sim.enemy_state(0).expect("the enemy is alive");
        assert!(player.grounded, "{player:?}");
        assert!((player.position.y - 0.915).abs() < 0.03, "{player:?}");
        assert!(
            (sim.player_eye().y - 1.63).abs() < 0.03,
            "the player's eye is 1.63 m above the floor, got {}",
            sim.player_eye().y
        );
        assert!((target.position.y - 0.915).abs() < 0.03, "{target:?}");
    }

    #[test]
    fn the_shipped_lean_is_0_35_m_and_12_degrees() {
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 1 });
        rig.stow(mag(17, 17)).unwrap();
        let kit = Kit {
            weapon: RifleDef {
                well: MagazineWell::new("test"),
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                range: 100.0,
            },
            loadout: Loadout::new(rifle_from(mag(17, 17)), rig),
        };

        let config = tuning(kit);

        assert_eq!(config.lean.shift, 0.35);
        assert!((config.lean.roll - 12.0_f32.to_radians()).abs() < 1e-6);
        assert_eq!(config.lean.blend_seconds, 0.25);
        assert_eq!(config.lean.speed_factor, 0.5);
    }
}
