use super::{CharacterHandle, PhysicsWorld};
use glam::Vec3;
use rapier3d::prelude::{Collider, ColliderHandle, QueryFilter, Ray};

/// The game's meaning for a surface (material, team, body part); the engine never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HitTag(pub u32);

/// A ray: where it starts, which way it points, how far it reaches and whom it skips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayQuery {
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_distance: f32,
    pub ignore: Option<CharacterHandle>,
}

/// The first tagged surface a ray reached; `distance` is in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub tag: HitTag,
}

/// Set on every tagged collider's `user_data`, so tag 0 differs from an untagged collider (0).
const TAGGED_BIT: u128 = 1 << 32;

pub(super) fn encode_tag(tag: HitTag) -> u128 {
    TAGGED_BIT | u128::from(tag.0)
}

fn decode_tag(user_data: u128) -> Option<HitTag> {
    if user_data & TAGGED_BIT == 0 {
        return None;
    }
    u32::try_from(user_data & u128::from(u32::MAX))
        .ok()
        .map(HitTag)
}

impl PhysicsWorld {
    /// The nearest tagged surface along `query`.
    ///
    /// Reads the world as the last `step` left it, so a tick moves characters, steps, then
    /// casts rays. Untagged colliders (movement capsules, props) never stop a ray. The
    /// direction is normalised, so its length does not matter; the normal faces back against
    /// the ray. A zero or non-finite direction, or a `max_distance` that is not positive, hits
    /// nothing.
    pub fn cast_ray(&self, query: &RayQuery) -> Option<RayHit> {
        let direction = query.direction.try_normalize()?;
        if query.max_distance.is_nan() || query.max_distance <= 0.0 {
            return None;
        }
        let ray = Ray::new(query.origin, direction);
        let tagged =
            |_: ColliderHandle, collider: &Collider| decode_tag(collider.user_data).is_some();
        let mut filter = QueryFilter::default().predicate(&tagged);
        if let Some(ignore) = query.ignore {
            filter = filter.exclude_rigid_body(ignore.0);
        }
        let queries = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.rigid_body_set,
            &self.collider_set,
            filter,
        );

        let (collider, hit) = queries.cast_ray_and_get_normal(&ray, query.max_distance, true)?;
        let tag = self.collider_tag(collider)?;
        // A trimesh reports its triangle's geometric normal whichever side the ray came from.
        let normal = if hit.normal.dot(direction) > 0.0 {
            -hit.normal
        } else {
            hit.normal
        };
        Some(RayHit {
            point: query.origin + direction * hit.time_of_impact,
            normal,
            distance: hit.time_of_impact,
            tag,
        })
    }

    /// The tag of `collider`; `None` for an untagged or removed collider.
    pub fn collider_tag(&self, collider: ColliderHandle) -> Option<HitTag> {
        decode_tag(self.collider_set.get(collider)?.user_data)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{x_wall_mesh, x_wall_mesh_reversed};
    use super::*;
    use moho_render_api::WorldMeshId;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;
    const EPS: f32 = 1e-3;

    fn along_x(max_distance: f32) -> RayQuery {
        RayQuery {
            origin: Vec3::ZERO,
            direction: Vec3::X,
            max_distance,
            ignore: None,
        }
    }

    fn assert_vec_near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < EPS,
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn world_with_wall(x: f32, tag: HitTag) -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(x), tag);
        world.step(DT);
        world
    }

    #[test]
    fn ray_reports_the_nearest_surface() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(7));
        world.set_world_mesh(WorldMeshId(2), &x_wall_mesh(10.0), HitTag(9));
        world.step(DT);

        let hit = world.cast_ray(&along_x(20.0)).expect("a wall is in range");

        assert_eq!(hit.tag, HitTag(7));
        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
        assert_vec_near(hit.point, Vec3::new(5.0, 0.0, 0.0));
        assert_vec_near(hit.normal, Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn ray_stops_at_its_maximum_distance() {
        let world = world_with_wall(10.0, HitTag(7));

        let hit = world.cast_ray(&along_x(5.0));

        assert_eq!(hit, None);
    }

    proptest! {
        #[test]
        fn ray_in_an_empty_world_hits_nothing(
            origin in prop::array::uniform3(-100.0f32..100.0),
            direction in prop::array::uniform3(-1.0f32..1.0),
            max_distance in 0.0f32..1000.0,
        ) {
            let mut world = PhysicsWorld::new();
            world.step(DT);

            let hit = world.cast_ray(&RayQuery {
                origin: Vec3::from(origin),
                direction: Vec3::from(direction),
                max_distance,
                ignore: None,
            });

            prop_assert_eq!(hit, None);
        }
    }

    #[test]
    fn movement_capsules_do_not_stop_a_ray() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(7));
        world.add_character(Vec3::new(2.0, 0.0, 0.0));
        world.step(DT);

        let hit = world
            .cast_ray(&along_x(20.0))
            .expect("the wall is behind the character");

        assert_eq!(hit.tag, HitTag(7));
        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
    }

    #[test]
    fn direction_length_does_not_change_the_distance() {
        let world = world_with_wall(5.0, HitTag(7));

        let hit = world
            .cast_ray(&RayQuery {
                direction: Vec3::new(3.0, 0.0, 0.0),
                ..along_x(20.0)
            })
            .expect("the wall is in range");

        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
        assert_vec_near(hit.point, Vec3::new(5.0, 0.0, 0.0));
    }

    #[test]
    fn zero_or_non_finite_direction_hits_nothing() {
        let world = world_with_wall(5.0, HitTag(7));
        let directions = [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(f32::INFINITY, 0.0, 0.0),
            Vec3::new(1.0, f32::NEG_INFINITY, 0.0),
            Vec3::new(1.0, 0.0, f32::NAN),
        ];

        for direction in directions {
            let hit = world.cast_ray(&RayQuery {
                direction,
                ..along_x(20.0)
            });

            assert_eq!(hit, None, "direction {direction:?}");
        }
    }

    #[test]
    fn non_positive_max_distance_hits_nothing() {
        let world = world_with_wall(5.0, HitTag(7));

        for max_distance in [0.0, -1.0, f32::NEG_INFINITY] {
            let hit = world.cast_ray(&along_x(max_distance));

            assert_eq!(hit, None, "max_distance {max_distance}");
        }
    }

    #[test]
    fn normal_faces_the_ray_whichever_way_the_wall_is_wound() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(1));
        world.step(DT);
        let front_wound = world.cast_ray(&along_x(20.0)).expect("wall in range");
        let mut reversed = PhysicsWorld::new();
        reversed.set_world_mesh(WorldMeshId(1), &x_wall_mesh_reversed(5.0), HitTag(1));
        reversed.step(DT);

        let back_wound = reversed.cast_ray(&along_x(20.0)).expect("wall in range");

        assert_vec_near(front_wound.normal, Vec3::new(-1.0, 0.0, 0.0));
        assert_vec_near(back_wound.normal, Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn replacing_a_world_mesh_replaces_its_tag() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(7));
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(9));
        world.step(DT);

        let hit = world.cast_ray(&along_x(20.0)).expect("wall in range");

        assert_eq!(hit.tag, HitTag(9));
    }

    #[test]
    fn removed_world_mesh_is_not_hit() {
        let mut world = world_with_wall(5.0, HitTag(7));
        world.remove_world_mesh(WorldMeshId(1));
        world.step(DT);

        let hit = world.cast_ray(&along_x(20.0));

        assert_eq!(hit, None);
    }

    #[test]
    fn untagged_props_do_not_stop_a_ray() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(7));
        world.add_dynamic_sphere(Vec3::new(2.0, 0.0, 0.0), 0.5);
        world.step(DT);

        let hit = world
            .cast_ray(&along_x(20.0))
            .expect("the wall is behind the sphere");

        assert_eq!(hit.tag, HitTag(7));
        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
    }

    proptest! {
        #[test]
        fn any_tag_survives_registration(tag in prop_oneof![Just(0u32), Just(u32::MAX), any::<u32>()]) {
            let world = world_with_wall(5.0, HitTag(tag));

            let hit = world.cast_ray(&along_x(20.0));
            let collider = world
                .world_mesh_collider(WorldMeshId(1))
                .expect("wall registered");

            prop_assert_eq!(hit.map(|h| h.tag), Some(HitTag(tag)));
            prop_assert_eq!(world.collider_tag(collider), Some(HitTag(tag)));
        }
    }

    #[test]
    fn collider_tag_reads_a_world_meshs_tag_and_none_for_untagged() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(5.0), HitTag(7));
        world.add_character(Vec3::new(2.0, 0.0, 0.0));
        let wall = world
            .world_mesh_collider(WorldMeshId(1))
            .expect("wall registered");
        let capsule = world
            .collider_set
            .iter()
            .map(|(handle, _)| handle)
            .find(|&handle| handle != wall)
            .expect("the character has a capsule");
        world.set_world_mesh(WorldMeshId(2), &x_wall_mesh(8.0), HitTag(9));
        let removed = world
            .world_mesh_collider(WorldMeshId(2))
            .expect("wall registered");
        world.remove_world_mesh(WorldMeshId(2));

        assert_eq!(world.collider_tag(wall), Some(HitTag(7)));
        assert_eq!(world.collider_tag(capsule), None);
        assert_eq!(world.collider_tag(removed), None);
    }

    #[test]
    fn nan_max_distance_hits_nothing() {
        let world = world_with_wall(5.0, HitTag(7));

        let hit = world.cast_ray(&along_x(f32::NAN));

        assert_eq!(hit, None);
    }

    #[test]
    fn infinite_max_distance_reaches_the_wall() {
        let world = world_with_wall(5.0, HitTag(7));

        let hit = world
            .cast_ray(&along_x(f32::INFINITY))
            .expect("an unbounded ray reaches the wall");

        assert_eq!(hit.tag, HitTag(7));
        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
    }

    #[test]
    fn non_finite_origin_hits_nothing() {
        let world = world_with_wall(5.0, HitTag(7));
        let origins = [
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(0.0, f32::NAN, 0.0),
            Vec3::new(f32::INFINITY, 0.0, 0.0),
            Vec3::new(0.0, f32::INFINITY, 0.0),
            Vec3::new(f32::NEG_INFINITY, 0.0, 0.0),
            Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
        ];

        for origin in origins {
            let hit = world.cast_ray(&RayQuery {
                origin,
                ..along_x(20.0)
            });

            assert_eq!(hit, None, "origin {origin:?}");
        }
    }

    #[test]
    fn normal_faces_a_ray_from_the_far_side() {
        let meshes = [x_wall_mesh(5.0), x_wall_mesh_reversed(5.0)];

        for mesh in meshes {
            let mut world = PhysicsWorld::new();
            world.set_world_mesh(WorldMeshId(1), &mesh, HitTag(1));
            world.step(DT);

            let hit = world
                .cast_ray(&RayQuery {
                    origin: Vec3::new(10.0, 0.0, 0.0),
                    direction: Vec3::NEG_X,
                    ..along_x(20.0)
                })
                .expect("wall in range");

            assert!(
                (hit.distance - 5.0).abs() < EPS,
                "distance {}",
                hit.distance
            );
            assert_vec_near(hit.normal, Vec3::new(1.0, 0.0, 0.0));
            assert_vec_near(hit.point, Vec3::new(5.0, 0.0, 0.0));
        }
    }

    #[test]
    fn ray_reports_the_nearest_surface_in_any_direction() {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(1), &x_wall_mesh(10.0), HitTag(9));
        world.set_world_mesh(WorldMeshId(2), &x_wall_mesh(5.0), HitTag(7));
        world.step(DT);

        let hit = world.cast_ray(&along_x(20.0)).expect("a wall is in range");

        assert_eq!(hit.tag, HitTag(7));
        assert!(
            (hit.distance - 5.0).abs() < EPS,
            "distance {}",
            hit.distance
        );
    }
}
