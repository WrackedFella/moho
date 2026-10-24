use crate::support::{Scene, floor_prim, write_gltf};
use glam::Vec3;
use moho_level::load;
use moho_physics::{CharacterBody, CharacterCommand, HitTag, MoveSettings, PhysicsWorld};
use moho_render_api::WorldMeshId;

#[test]
fn character_dropped_on_loaded_floor_stands_on_it() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    let (_dir, path) = write_gltf(&scene);
    let level = load(&path).expect("level loads");
    let mut world = PhysicsWorld::new();
    for (id, level_mesh) in level.meshes.iter().enumerate() {
        world.set_world_mesh(WorldMeshId(id as u64), &level_mesh.mesh, HitTag(0));
    }
    let capsule_centre_above_base = CharacterBody::default().standing.hull.length / 2.0;
    let settings = MoveSettings {
        max_speed: 4.0,
        ground_accel: 40.0,
        friction: 4.0,
        stop_speed: 1.0,
        air_accel: 10.0,
        air_speed_cap: 0.8,
        gravity: Some(18.0),
        jump_speed: 8.0,
        max_velocity: 90.0,
        autohop: false,
    };
    let handle = world.add_character(Vec3::new(0.0, 2.0 + capsule_centre_above_base, 0.0));

    for _ in 0..120 {
        world
            .move_character(handle, &CharacterCommand::default(), &settings, 1.0 / 60.0)
            .expect("character exists");
        world.step(1.0 / 60.0);
    }

    let character = world.character(handle).expect("character exists");
    let base = character.position.y - capsule_centre_above_base;
    assert!(character.grounded, "character should be grounded");
    assert!(base.abs() < 0.1, "capsule base at {base}, expected 0");
}
