use glam::Vec3;
use legion::World;
use moho_core::materials::MaterialType;
use moho_game::actors::Sphere;
use moho_game::scene_persistence;

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

    // Encode the scene into in-memory bytes
    let bytes = scene_persistence::encode_to_bytes(&world, None, &[]).expect("encode ok");

    // create a fresh world and decode from bytes
    let mut loaded_world = World::default();
    let (_camera_data, _lights) =
        scene_persistence::load_from_bytes(&bytes, &mut loaded_world).expect("decode ok");

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
