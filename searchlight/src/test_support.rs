//! Shared builders for the crate's tests: small hand-made worlds and tuning values.

use std::sync::Arc;

use std::collections::BTreeMap;

use glam::{Quat, Vec3};
use moho_input::Action;
use moho_input::action_map::ActionMap;
use moho_input::bindings::Input;
use moho_level::{LevelMarker, NavMesh};
use moho_physics::{MoveSettings, PhysicsWorld};
use moho_render_api::{WorldMesh, WorldMeshId};
use searchlight_rules::{
    BodyPart, BodyProfile, Caliber, Grade, GradeMix, ItemId, LeanTuning, Loadout, Magazine,
    MagazineSpec, MagazineWell, Rifle, Rig, RigSpec,
};

use crate::arena::{Arena, nav_profile};
use crate::brain::EnemyTuning;
use crate::input::SearchlightAction;
use crate::mode::ArenaMode;
use crate::sim::SimConfig;
use crate::tag::SurfaceTag;
use crate::tuning::SHIPPED_BODY;
use crate::wave::WaveTuning;

pub(crate) const DT: f32 = 1.0 / 60.0;

/// A closed axis-aligned box with outward-facing triangles.
pub(crate) fn box_mesh(min: Vec3, max: Vec3) -> WorldMesh {
    let positions = vec![
        [min.x, min.y, min.z],
        [max.x, min.y, min.z],
        [max.x, max.y, min.z],
        [min.x, max.y, min.z],
        [min.x, min.y, max.z],
        [max.x, min.y, max.z],
        [max.x, max.y, max.z],
        [min.x, max.y, max.z],
    ];
    let indices = vec![
        0, 3, 2, 0, 2, 1, // -z
        4, 5, 6, 4, 6, 7, // +z
        0, 4, 7, 0, 7, 3, // -x
        1, 2, 6, 1, 6, 5, // +x
        0, 1, 5, 0, 5, 4, // -y
        3, 7, 6, 3, 6, 2, // +y
    ];
    WorldMesh::new(
        positions,
        vec![[0.0, 1.0, 0.0]; 8],
        vec![1.0; 8],
        vec![[0.0; 3]; 8],
        vec![1.0; 8],
        vec![0; 8],
        indices,
    )
    .expect("a box has matching channels and whole triangles")
}

/// A world holding one `World`-tagged box per `(min, max)` pair, stepped once.
pub(crate) fn world_with(boxes: &[(Vec3, Vec3)]) -> PhysicsWorld {
    let mut world = PhysicsWorld::new();
    for (id, &(min, max)) in boxes.iter().enumerate() {
        world.set_world_mesh(
            WorldMeshId(id as u64),
            &box_mesh(min, max),
            SurfaceTag::World.into(),
        );
    }
    world.step(DT);
    world
}

/// A floor whose top is at y = 0, spanning x and z in [-10, 10], walled in 4 m high.
pub(crate) fn room() -> PhysicsWorld {
    world_with(&[
        (Vec3::new(-12.0, -1.0, -12.0), Vec3::new(12.0, 0.0, 12.0)),
        (Vec3::new(-11.0, 0.0, -11.0), Vec3::new(-10.0, 4.0, 11.0)),
        (Vec3::new(10.0, 0.0, -11.0), Vec3::new(11.0, 4.0, 11.0)),
        (Vec3::new(-11.0, 0.0, -11.0), Vec3::new(11.0, 4.0, -10.0)),
        (Vec3::new(-11.0, 0.0, 10.0), Vec3::new(11.0, 4.0, 11.0)),
    ])
}

/// A floor only, top at y = 0.
pub(crate) fn flat_floor() -> PhysicsWorld {
    world_with(&[(Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0))])
}

/// A magazine of the `test` well holding `rounds` of `test_round`.
pub(crate) fn mag(capacity: u32, rounds: u32) -> Magazine {
    let spec = MagazineSpec {
        well: MagazineWell::new("test"),
        caliber: Caliber::new("test"),
        capacity,
    };
    Magazine::filled(
        ItemId::new("test_mag"),
        &spec,
        &ItemId::new("test_round"),
        rounds,
    )
    .expect("test rounds fit the capacity")
}

/// A magazine of a well the `test` rifle does not take.
pub(crate) fn foreign_mag(capacity: u32, rounds: u32) -> Magazine {
    let spec = MagazineSpec {
        well: MagazineWell::new("foreign"),
        caliber: Caliber::new("test"),
        capacity,
    };
    Magazine::filled(
        ItemId::new("foreign_mag"),
        &spec,
        &ItemId::new("test_round"),
        rounds,
    )
    .expect("test rounds fit the capacity")
}

/// A rifle taking the `test` well, loaded from `magazine`.
pub(crate) fn rifle_from(magazine: Magazine) -> Rifle {
    Rifle::loaded(MagazineWell::new("test"), magazine).expect("test magazines fit the test well")
}

pub(crate) fn test_move_settings() -> MoveSettings {
    MoveSettings {
        max_speed: 4.0,
        ground_accel: 40.0,
        friction: 4.0,
        stop_speed: 1.0,
        air_accel: 10.0,
        air_speed_cap: 0.8,
        gravity: None,
        jump_speed: 8.0,
        max_velocity: 90.0,
        autohop: false,
        crouch_ticks: 0,
        prone_ticks: 0,
        lean_ticks: 0,
    }
}

pub(crate) fn test_profile() -> BodyProfile {
    BodyProfile {
        max_health: 100,
        head_status: 50,
        arm_status: 60,
        leg_status: 60,
    }
}

/// Tuning with a 3-tick cooldown, 10-tick reload, 30 damage and the given rifle, with
/// `spares` stowed in order on a rig that has one slot per spare.
pub(crate) fn test_config(rifle: Rifle, spares: Vec<Magazine>) -> SimConfig {
    let slots = u32::try_from(spares.len()).expect("a handful of test spares");
    let mut rig = Rig::new(
        ItemId::new("test_rig"),
        &RigSpec {
            magazine_slots: slots,
        },
    );
    for spare in spares {
        rig.stow(spare).expect("the rig has a slot per spare");
    }
    SimConfig {
        move_settings: test_move_settings(),
        gravity: 18.0,
        character_body: SHIPPED_BODY,
        range: 100.0,
        damage: 30,
        cooldown_ticks: 3,
        reload_ticks: 10,
        charge_ticks: 0,
        body: test_profile(),
        loadout: Loadout::new(rifle, rig),
        enemy: enemy_tuning(),
        enemy_aim_at: BodyPart::Torso,
        lean: LeanTuning {
            shift: 0.35,
            roll: 12.0_f32.to_radians(),
            blend_seconds: 0.25,
            speed_factor: 0.5,
        },
    }
}

/// Puts `config`'s eye `height` m above the capsule centre, the frame hit volumes are posed in.
pub(crate) fn set_eye_above_centre(config: &mut SimConfig, height: f32) {
    let standing = &mut config.character_body.standing;
    standing.eye_height = height + standing.hull.length / 2.0;
}

/// Enemy tuning with a 3-tick reaction (so the first shot is on tick 2), a 4-tick cooldown,
/// 10 damage, no aim error and a fast turn.
pub(crate) fn enemy_tuning() -> EnemyTuning {
    EnemyTuning {
        sight_range: 30.0,
        view_half_angle: 60.0_f32.to_radians(),
        reaction_ticks: 3,
        cooldown_ticks: 4,
        damage: 10,
        aim_error: 0.0,
        turn_rate: 6.0,
        patrol_speed: 0.5,
        arrive_radius: 0.5,
        fire_cone: 0.1,
        cover_range: 15.0,
        cover_ticks: 240,
        chase_ticks: 600,
        cover_hide_height: 0.9,
        near_miss_radius: 1.5,
        seed: 7,
    }
}

/// An arena of a 60 m square floor (top at y = 0) plus one `World` box per `(min, max)` pair,
/// with the given spawns and patrol route and no dummies.
pub(crate) fn arena_with(
    boxes: &[(Vec3, Vec3)],
    player: Vec3,
    enemies: &[Vec3],
    route: &[Vec3],
) -> Arc<Arena> {
    let floor = (Vec3::new(-30.0, -1.0, -30.0), Vec3::new(30.0, 0.0, 30.0));
    let meshes: Vec<WorldMesh> = std::iter::once(&floor)
        .chain(boxes)
        .map(|&(min, max)| box_mesh(min, max))
        .collect();
    let nav = Arc::new(NavMesh::build(&meshes, nav_profile()).expect("box worlds build a navmesh"));
    Arc::new(Arena {
        material_names: vec![None; meshes.len()],
        meshes,
        nav,
        nav_fallback: false,
        player_spawn: player,
        dummy_spawns: Vec::new(),
        enemy_spawns: enemies.to_vec(),
        route: route.to_vec(),
        covers: Vec::new(),
    })
}

/// Waves of experienced enemies that open with `first_size` enemies, grow by one and score 100
/// a kill.
pub(crate) fn wave_tuning(first_size: u32) -> WaveTuning {
    WaveTuning::new(first_size, 1, 100)
        .expect("a positive first wave and growth are valid")
        .with_mix(GradeMix::only(Grade::Experienced))
}

/// A level marker named `name` at `position`, with no properties.
pub(crate) fn level_marker(name: &str, position: Vec3) -> LevelMarker {
    LevelMarker {
        name: name.to_owned(),
        position,
        rotation: Quat::IDENTITY,
        properties: BTreeMap::new(),
    }
}

/// The `searchlight_level` marker whose `mode` is `value`.
pub(crate) fn mode_marker(value: &str) -> LevelMarker {
    let mut marker = level_marker("searchlight_level", Vec3::ZERO);
    marker
        .properties
        .insert("mode".to_owned(), value.to_owned());
    marker
}

/// The wave and score of a mode that runs waves.
pub(crate) trait WaveScore {
    fn wave(&self) -> u32;
    fn score(&self) -> u32;
}

impl WaveScore for ArenaMode {
    fn wave(&self) -> u32 {
        self.tally().expect("this mode runs waves").wave
    }

    fn score(&self) -> u32 {
        self.tally().expect("this mode runs waves").score
    }
}

/// Presses the first default binding of `action` on `map`.
pub(crate) fn press(map: &mut ActionMap<SearchlightAction>, action: SearchlightAction) {
    match action.default_bindings()[0].input {
        Input::Key(key) => map.key(key, true),
        Input::Mouse(button) => map.mouse_button(button, true),
        Input::Pad(_) => panic!("the first default binding of {action:?} is a pad input"),
    }
}
