use engine_core::actors::Sphere;
use engine_core::materials::MaterialType;
use glam::Vec3;
use legion::World;
use std::fs;

#[test]
fn save_and_load_scene_roundtrip() {
    let mut world = World::default();
    // create a simple scene with two spheres
    let s1 = Sphere::new(
        Vec3::new(0.0, 0.0, 0.0),
        1.0,
        MaterialType::Lambertian {
            albedo: Vec3::new(0.1, 0.2, 0.3),
        },
    );
    let s2 = Sphere::new(
        Vec3::new(2.0, 0.0, 0.0),
        0.5,
        MaterialType::Metal {
            albedo: Vec3::new(0.7, 0.7, 0.7),
            fuzz: 0.2,
        },
    );
    world.push((s1,));
    world.push((s2,));

    let scene = engine_renderer::Scene::new();

    let tmp = tempfile::NamedTempFile::new().expect("create temp file");
    let path = tmp.path().to_path_buf();

    // save
    scene.save_to_file(&path, &world, None).expect("save ok");

    // create a fresh world and load
    let mut loaded_world = World::default();
    let mut scene2 = engine_renderer::Scene::new();
    let _camera_data = scene2
        .load_from_file(&path, &mut loaded_world)
        .expect("load ok");

    // Basic checks: worlds contain Hittable-like components (we pushed Spheres only)
    use legion::query::IntoQuery;
    let mut q = <&Sphere>::query();
    let orig_count = q.iter(&world).count();
    let loaded_count = q.iter(&loaded_world).count();
    assert_eq!(
        orig_count, loaded_count,
        "scene roundtrip should preserve entity count"
    );

    // Cleanup
    let _ = fs::remove_file(path);
}
