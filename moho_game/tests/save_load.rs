use glam::Vec3;
use moho_core::materials::MaterialType;
use moho_game::actors::Sphere;
use moho_game::scene::SceneEntities;
use moho_game::scene_persistence;

#[test]
fn scene_encode_decode_roundtrip_in_memory() {
    let mut entities = SceneEntities::default();
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
    entities.actors.spawn_sphere(s1);
    entities.actors.spawn_sphere(s2);

    let bytes = scene_persistence::encode_to_bytes(&entities, None, &[]).expect("encode ok");

    let mut loaded = SceneEntities::default();
    scene_persistence::load_from_bytes(&bytes, &mut loaded).expect("decode ok");

    let spheres = loaded.actors.spheres();
    assert_eq!(
        spheres.len(),
        2,
        "scene roundtrip should preserve entity count"
    );
    for (got, want) in spheres.iter().zip([s1, s2]) {
        assert_eq!(got.center, want.center);
        assert_eq!(got.radius, want.radius);
        assert_eq!(got.mat_ptr, want.mat_ptr);
    }
    assert!(loaded.actors.cubes().is_empty());
    assert!(loaded.chunks.is_empty());
}
