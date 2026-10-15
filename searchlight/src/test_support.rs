//! Shared builders for the crate's tests: small hand-made worlds and tuning values.

use glam::Vec3;
use moho_input::Action;
use moho_input::action_map::ActionMap;
use moho_input::bindings::Binding;
use moho_physics::{MoveSettings, PhysicsWorld};
use moho_render_api::{WorldMesh, WorldMeshId};
use searchlight_rules::{BodyProfile, Magazine, Rifle};

use crate::input::FpsAction;
use crate::sim::SimConfig;
use crate::tag::SurfaceTag;

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

pub(crate) fn mag(capacity: u32, rounds: u32) -> Magazine {
    Magazine::new(capacity, rounds).expect("test rounds fit the capacity")
}

pub(crate) fn test_move_settings() -> MoveSettings {
    MoveSettings {
        max_speed: 4.0,
        ground_accel: 40.0,
        friction: 4.0,
        stop_speed: 1.0,
        air_accel: 10.0,
        air_speed_cap: 0.8,
        gravity: 18.0,
        jump_speed: 8.0,
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

/// Tuning with a 3-tick cooldown, 10-tick reload, 30 damage and the given rifle and spares.
pub(crate) fn test_config(rifle: Rifle, spares: Vec<Magazine>) -> SimConfig {
    SimConfig {
        move_settings: test_move_settings(),
        eye_height: 0.7,
        range: 100.0,
        damage: 30,
        cooldown_ticks: 3,
        reload_ticks: 10,
        body: test_profile(),
        rifle,
        spares,
    }
}

/// Presses the first default binding of `action` on `map`.
pub(crate) fn press(map: &mut ActionMap<FpsAction>, action: FpsAction) {
    match action.default_bindings()[0] {
        Binding::Key(key) => map.key(key, true),
        Binding::Mouse(button) => map.mouse_button(button, true),
        Binding::Pad(_) => panic!("the first default binding of {action:?} is a pad input"),
    }
}
