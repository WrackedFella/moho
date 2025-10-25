use engine_core::actors::Sphere;
use engine_core::materials::MaterialType;
use glam::Vec3;
use legion::World;

#[test]
fn scene_encode_decode_roundtrip_in_memory() {
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

    // Encode the scene into in-memory bytes
    let bytes = scene.encode_to_bytes(&world, None).expect("encode ok");

    // create a fresh world and decode from bytes
    let mut loaded_world = World::default();
    let mut scene2 = engine_renderer::Scene::new();
    let _camera_data = scene2
        .load_from_bytes(&bytes, &mut loaded_world)
        .expect("decode ok");

    // Basic checks: worlds contain Hittable-like components (we pushed Spheres only)
    use legion::query::IntoQuery;
    let mut q = <&Sphere>::query();
    let orig_count = q.iter(&world).count();
    let loaded_count = q.iter(&loaded_world).count();
    assert_eq!(
        orig_count, loaded_count,
        "scene roundtrip should preserve entity count"
    );
}
