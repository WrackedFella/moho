//! The simulation's tuning, with the rifle's numbers taken from its definition.

use std::collections::{BTreeMap, BTreeSet};

use moho_physics::{CharacterBody, Hull, HullShape, MoveSettings, Stature};
use searchlight_rules::{
    AccuracyTuning, BodyPart, BodyProfile, Carried, Grade, ItemId, Kit, LeanTuning, LoadThresholds,
    PlayerDef, Remains, RoundStats, loadout_items, remains_items,
};

use moho_core::content::ContentRoots;

use crate::brain::EnemyTuning;
use crate::kit::{ENEMY_KIT_ID, KitLoadError, load_catalogue, load_enemy_for_grade};
use crate::sim::{CarryConfig, SimConfig};

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
    crouched: None,
    prone: None,
    lean: moho_physics::Lean {
        upright: moho_physics::LeanReach {
            distance: 0.0,
            roll: 0.0,
        },
        prone: moho_physics::LeanReach {
            distance: 0.0,
            roll: 0.0,
        },
        head_radius: 0.0,
    },
};

/// The player's attribute sheet as `content/player.toml` ships it, for a config built with no
/// content roots at hand.
pub const SHIPPED_PLAYER: PlayerDef = PlayerDef {
    strength: 10.0,
    carry_capacity_per_strength: 3.0,
    movement_speed: 1.0,
    stamina_pool: 5.0,
    slowed_scale: 0.7,
    thresholds: LoadThresholds {
        slowed: 0.6,
        no_sprint: 0.8,
        cannot_move: 1.0,
    },
};

/// Ticks the rig strip stays up after a reload or switch.
pub const STRIP_FADE_TICKS: u32 = 180;

/// Ticks a search of a dead enemy takes: 1.5 s.
pub const SEARCH_TICKS: u32 = 90;

/// How far from the eye a body can be searched, in metres: the eye stands 1.63 m over the floor
/// point a body lies at, so this reaches about 1.9 m along the floor.
pub const SEARCH_REACH: f32 = 2.5;

/// How close to the use ray a body's point must pass for the body to be in view, in metres.
pub const BODY_RADIUS: f32 = 0.8;

/// Builds the simulation tuning around the starting `kit`: its weapon's numbers and its loadout.
///
/// The carry config's item definitions are weightless placeholders, one per item the loadout
/// holds, so load is 0 here. Only tests build a config without content; the shipped game uses
/// [`shipped`], whose definitions come from the item catalogue.
#[cfg(test)]
pub fn tuning(kit: Kit) -> SimConfig {
    let weapon = rifle_item();
    let items = weightless(&distinct(loadout_items(&kit.loadout, &weapon)));
    build(
        kit,
        CarryConfig {
            player: SHIPPED_PLAYER,
            items,
            weapon,
        },
    )
}

/// The tuning around the starting `kit` with `carry` as its carry config.
fn build(kit: Kit, carry: CarryConfig) -> SimConfig {
    let Kit {
        weapon,
        loadout,
        rounds,
    } = kit;
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
            crouch_ticks: 0,
            prone_ticks: 0,
            lean_ticks: 0,
        },
        gravity: 18.0,
        character_body: SHIPPED_BODY,
        rounds,
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
        accuracy: AccuracyTuning {
            walk_spread: 0.03,
            settle_seconds: 0.35,
        },
        search_ticks: SEARCH_TICKS,
        search_reach: SEARCH_REACH,
        body_radius: BODY_RADIUS,
        enemy_remains: Remains::default(),
        grade_remains: BTreeMap::new(),
        carry,
    }
}

/// The item id of the rifle the player holds, in [`tuning`]'s carry config.
#[cfg(test)]
fn rifle_item() -> ItemId {
    ItemId::new("rifle")
}

/// The distinct item ids among `carried`, in id order.
fn distinct(carried: Vec<Carried>) -> Vec<ItemId> {
    let ids: BTreeSet<ItemId> = carried.into_iter().map(|(id, _)| id).collect();
    ids.into_iter().collect()
}

/// A weightless plain definition for each of `ids`.
#[cfg(test)]
fn weightless(ids: &[ItemId]) -> BTreeMap<ItemId, searchlight_rules::ItemDef> {
    ids.iter()
        .map(|id| (id.clone(), crate::test_support::weighing(0.0)))
        .collect()
}

/// The shipped game's tuning: the kit `kit` of the content `roots`, with the catalogue's
/// definitions of everything it carries and `player.toml` as its carry config.
///
/// # Errors
///
/// A [`KitLoadError`] when the kit, the items or the player sheet cannot be loaded.
pub fn shipped(roots: &ContentRoots, kit: &str) -> Result<SimConfig, KitLoadError> {
    assemble(roots, kit, None)
}

/// [`shipped`] with the kit `enemy_kit` as what every enemy leaves, its items defined by the
/// catalogue too.
///
/// # Errors
///
/// As [`shipped`], and for the enemy kit.
pub fn shipped_with_enemy(
    roots: &ContentRoots,
    kit: &str,
    enemy_kit: &str,
) -> Result<SimConfig, KitLoadError> {
    assemble(roots, kit, Some(enemy_kit))
}

fn assemble(
    roots: &ContentRoots,
    kit: &str,
    enemy_kit: Option<&str>,
) -> Result<SimConfig, KitLoadError> {
    let catalogue = load_catalogue(roots, kit)?;
    let player = crate::player::load(roots)?;
    let enemy = enemy_kit
        .map(|enemy_kit| crate::kit::load_enemy(roots, enemy_kit))
        .transpose()?;
    // Grades follow the default enemy kit unless the kits define one of their own; a custom
    // enemy kit is every enemy's remains as it stands.
    let graded = match (&enemy, enemy_kit) {
        (Some((default, _)), Some(ENEMY_KIT_ID)) => graded_remains(roots, default)?,
        _ => BTreeMap::new(),
    };
    let mut carried = loadout_items(&catalogue.kit.loadout, &catalogue.weapon);
    if let Some((remains, _)) = &enemy {
        carried.extend(remains_items(remains, &catalogue.weapon));
    }
    for (remains, _) in graded.values() {
        carried.extend(remains_items(remains, &catalogue.weapon));
    }
    let items = catalogue.define(&distinct(carried))?;
    let config = build(
        catalogue.kit,
        CarryConfig {
            player,
            items,
            weapon: catalogue.weapon,
        },
    );
    let mut config = match enemy {
        Some((remains, rounds)) => with_enemy(config, remains, rounds),
        None => config,
    };
    for (grade, (remains, rounds)) in graded {
        for (round, stats) in rounds {
            config.rounds.entry(round).or_insert(stats);
        }
        config.grade_remains.insert(grade, remains);
    }
    Ok(config)
}

/// An enemy kit resolved: what it leaves, and the stats of the rounds it names.
type ResolvedEnemy = (Remains, BTreeMap<ItemId, RoundStats>);

/// What each grade whose kit differs from `default` leaves, with its rounds' stats.
fn graded_remains(
    roots: &ContentRoots,
    default: &Remains,
) -> Result<BTreeMap<Grade, ResolvedEnemy>, KitLoadError> {
    let mut graded = BTreeMap::new();
    for grade in Grade::LADDER {
        let resolved = load_enemy_for_grade(roots, grade)?;
        if resolved.0 != *default {
            graded.insert(grade, resolved);
        }
    }
    Ok(graded)
}

/// `config` with `remains` as what every enemy leaves when it dies, and `rounds` joined to
/// the round stats the sim flies and scores rounds with, so a looted round still fires.
fn with_enemy(
    mut config: SimConfig,
    remains: Remains,
    rounds: BTreeMap<ItemId, RoundStats>,
) -> SimConfig {
    for (round, stats) in rounds {
        config.rounds.entry(round).or_insert(stats);
    }
    config.enemy_remains = remains;
    config
}

/// [`with_enemy`] for a config built by [`tuning`], where items of `remains` the carry config
/// does not define get weightless placeholders.
#[cfg(test)]
pub fn with_enemy_kit(
    mut config: SimConfig,
    remains: Remains,
    rounds: BTreeMap<ItemId, RoundStats>,
) -> SimConfig {
    let weapon = config.carry.weapon.clone();
    for (id, def) in weightless(&distinct(remains_items(&remains, &weapon))) {
        config.carry.items.entry(id).or_insert(def);
    }
    with_enemy(config, remains, rounds)
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
        Ballistics, FireBehaviour, ItemId, Kit, Loadout, MagazineWell, RifleDef, Rig, RigSpec,
        RoundStats,
    };
    use std::collections::BTreeMap;

    #[test]
    fn every_rifle_number_comes_from_the_kits_weapon() {
        let weapon = RifleDef {
            well: MagazineWell::new("test"),
            fire: FireBehaviour::SemiAuto,
            cooldown_ticks: 9,
            reload_ticks: 77,
            charge_ticks: 13,
        };
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 2 });
        rig.stow(mag(17, 17)).unwrap();
        let loadout = Loadout::new(rifle_from(mag(17, 17)), rig);
        let rounds = BTreeMap::from([(
            ItemId::new("test_round"),
            RoundStats {
                ballistics: Ballistics {
                    muzzle_velocity: 812.0,
                    drag: 1.5,
                },
                damage: 41,
            },
        )]);
        let kit = Kit {
            weapon,
            loadout: loadout.clone(),
            rounds: rounds.clone(),
        };

        let config = tuning(kit);

        assert_eq!(config.cooldown_ticks, 9);
        assert_eq!(config.reload_ticks, 77);
        assert_eq!(config.charge_ticks, 13);
        assert_eq!(config.loadout, loadout);
        assert_eq!(config.rounds, rounds);
    }

    fn shipped_config() -> SimConfig {
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 1 });
        rig.stow(mag(17, 17)).unwrap();
        tuning(Kit {
            weapon: RifleDef {
                well: MagazineWell::new("test"),
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
            },
            loadout: Loadout::new(rifle_from(mag(17, 17)), rig),
            rounds: BTreeMap::new(),
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
                crouched: None,
                prone: None,
                lean: moho_physics::Lean::default(),
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
    fn the_shipped_start_kit_carries_a_finite_positive_load_that_is_fine() {
        let roots = ContentRoots::base(&crate::content_root()).expect("the shipped content");

        let config = shipped(&roots, crate::kit::START_KIT_ID).expect("the shipped tuning loads");

        let kg = searchlight_rules::load(&config.loadout, &config.carry.weapon, |id| {
            config.carry.items.get(id)
        })
        .expect("every item in the start kit has a definition");
        let capacity = config
            .carry
            .player
            .baseline(searchlight_rules::Stat::CarryCapacity);
        assert!(kg.is_finite() && kg > 0.0, "{kg}");
        assert_eq!(
            searchlight_rules::load_state(kg, capacity, &config.carry.player.thresholds),
            searchlight_rules::LoadState::Fine,
            "{kg} kg of {capacity}"
        );
    }

    #[test]
    fn the_shipped_config_dresses_veterans_in_the_assault_rig() {
        use searchlight_rules::Grade;
        let roots = ContentRoots::base(&crate::content_root()).expect("the shipped content");

        let config = shipped_with_enemy(&roots, crate::kit::START_KIT_ID, crate::kit::ENEMY_KIT_ID)
            .expect("the shipped tuning loads");

        let rig_of = |grade: Grade| {
            let remains = config
                .grade_remains
                .get(&grade)
                .unwrap_or(&config.enemy_remains);
            remains.rig.as_ref().map(|rig| rig.item().clone())
        };
        assert_eq!(rig_of(Grade::Veteran), Some(ItemId::new("rig_assault")));
        assert_eq!(rig_of(Grade::Rookie), Some(ItemId::new("rig_chest")));
        for remains in config.grade_remains.values().chain([&config.enemy_remains]) {
            for (item, _) in remains_items(remains, &config.carry.weapon) {
                assert!(
                    config.carry.items.contains_key(&item),
                    "{item} is missing from the carry table"
                );
            }
        }
    }

    #[test]
    fn the_shipped_lean_is_0_35_m_and_12_degrees() {
        let mut rig = Rig::new(ItemId::new("test_rig"), &RigSpec { magazine_slots: 1 });
        rig.stow(mag(17, 17)).unwrap();
        let kit = Kit {
            weapon: RifleDef {
                well: MagazineWell::new("test"),
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
            },
            loadout: Loadout::new(rifle_from(mag(17, 17)), rig),
            rounds: BTreeMap::new(),
        };

        let config = tuning(kit);

        assert_eq!(config.lean.shift, 0.35);
        assert!((config.lean.roll - 12.0_f32.to_radians()).abs() < 1e-6);
        assert_eq!(config.lean.blend_seconds, 0.25);
        assert_eq!(config.lean.speed_factor, 0.5);
    }
}
