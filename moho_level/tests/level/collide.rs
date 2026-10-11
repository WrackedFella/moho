use crate::support::{Scene, floor_prim, write_gltf};
use glam::Vec3;
use moho_level::load;
use moho_physics::PhysicsWorld;
use moho_render_api::WorldMeshId;

#[test]
fn character_dropped_on_loaded_floor_stands_on_it() {
    let scene = Scene::with_meshes(vec![vec![floor_prim(0.0)]]);
    let (_dir, path) = write_gltf(&scene);
    let level = load(&path).expect("level loads");
    let mut world = PhysicsWorld::new();
    for (id, level_mesh) in level.meshes.iter().enumerate() {
        world.set_world_mesh(WorldMeshId(id as u64), &level_mesh.mesh);
    }
    let capsule_centre_above_base = 0.85 + 0.3;
    world.add_character(Vec3::new(0.0, 2.0 + capsule_centre_above_base, 0.0));

    for _ in 0..120 {
        world.move_character(Vec3::ZERO, 1.0 / 60.0);
        world.step(1.0 / 60.0);
    }

    let base = world.character_position().expect("character exists").y - capsule_centre_above_base;
    assert!(world.is_grounded, "character should be grounded");
    assert!(base.abs() < 0.1, "capsule base at {base}, expected 0");
}
