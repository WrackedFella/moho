//! The rapier collide-and-slide behind [`crate::movement::step`].

use crate::movement::{Ground, Slide, WALKABLE_NORMAL_Y};
use glam::Vec3;
use rapier3d::control::{
    CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::{ColliderHandle, Pose, QueryPipeline, Ray, Shape};

/// How far below a grounded landing to look for the surface it landed on, in metres.
const GROUND_PROBE: f32 = 0.1;

/// How far past the capsule's lowest point the ground ray reaches, in metres.
const GROUND_REACH_MARGIN: f32 = 0.3;

/// Surfaces facing down at least this much (`normal.y` below its negative) are ceilings, not risers.
const CEILING_NORMAL_Y: f32 = 0.3;

/// Extra downward distance when settling onto a step top, in metres.
const STEP_SNAP: f32 = 0.05;

/// Horizontal motion left over below this many metres is not stepped up.
const STEP_MIN_PROGRESS: f32 = 1e-3;

/// A shape cast that reports any contact within `distance`, measured from surface to surface.
fn cast_within(distance: f32) -> ShapeCastOptions {
    ShapeCastOptions {
        max_time_of_impact: distance,
        target_distance: 0.0,
        stop_at_penetration: false,
        compute_impact_geometry_on_penetration: true,
    }
}

/// A surface a grounded character may step up onto: too steep to walk on, not a ceiling.
fn is_riser(normal: Vec3) -> bool {
    normal.y < WALKABLE_NORMAL_Y && normal.y > -CEILING_NORMAL_Y
}

/// Whether the controller grounded a falling slide without reporting the walkable ground.
fn landed_without_ground_hit(grounded: bool, asked: Vec3, hits: &[(ColliderHandle, Vec3)]) -> bool {
    grounded
        && asked.y < 0.0
        && !hits
            .iter()
            .any(|&(_, normal)| normal.y >= WALKABLE_NORMAL_Y)
}

/// The direction and length of a step-up's reach across, at least `min_width`, or `None` when
/// too little horizontal motion is `remaining` to step.
fn step_reach(remaining: Vec3, min_width: f32) -> Option<(Vec3, f32)> {
    let length = remaining.length();
    (length >= STEP_MIN_PROGRESS).then(|| (remaining / length, length.max(min_width)))
}

/// The world as one character's collide-and-slide sees it.
pub(super) struct Slider<'a> {
    pub controller: &'a KinematicCharacterController,
    pub queries: &'a QueryPipeline<'a>,
    pub shape: &'a dyn Shape,
    pub dt: f32,
}

impl Slider<'_> {
    /// Move the capsule from `start` by `asked` and report what it touched.
    ///
    /// A grounded character blocked by a riser retries from on top of it, and a landing the
    /// controller detected without a hit gets the surface it landed on.
    pub(super) fn slide(&self, start: &Pose, asked: Vec3, was_grounded: bool) -> Slide {
        let mut hits = Vec::new();
        let plain = self.controller.move_shape(
            self.dt,
            self.queries,
            self.shape,
            start,
            asked,
            |collision| {
                hits.push((collision.handle, collision.hit.normal1));
            },
        );

        if was_grounded && hits.iter().any(|&(_, normal)| is_riser(normal)) {
            if let Some(translation) = self.step_up(start, asked, plain.translation) {
                return Slide {
                    translation,
                    grounded: true,
                    ground: self.ground_under(start.translation + translation),
                    hits,
                };
            }
        }

        if landed_without_ground_hit(plain.grounded, asked, &hits) {
            hits.extend(self.probe_landing(start, plain.translation));
        }
        Slide {
            translation: plain.translation,
            grounded: plain.grounded,
            // Only a walker reads its ground, and only a character that started grounded walks.
            ground: was_grounded
                .then(|| self.ground_under(start.translation + plain.translation))
                .flatten(),
            hits,
        }
    }

    /// The walkable surface straight under `centre`, or `None` when a ray cast down from it
    /// finds none within the capsule's half-height plus a margin.
    ///
    /// A ray, unlike a shape cast, ignores the capsule's rounded base, so it reads the face the
    /// centre stands over rather than a ledge edge the base merely overlaps. The gap is how far
    /// the capsule hangs above that face's plane, measured on the ray.
    pub(super) fn ground_under(&self, centre: Vec3) -> Option<Ground> {
        let capsule = self
            .shape
            .as_capsule()
            .expect("character colliders are capsules");
        let reach = capsule.half_height() + capsule.radius + GROUND_REACH_MARGIN;
        let ray = Ray::new(centre, -Vec3::Y);
        let (_, hit) = self.queries.cast_ray_and_get_normal(&ray, reach, true)?;
        (hit.normal.y >= WALKABLE_NORMAL_Y).then(|| Ground {
            normal: hit.normal,
            gap: hit.time_of_impact - (capsule.half_height() + capsule.radius / hit.normal.y),
        })
    }

    /// The surface under a character the controller grounded without reporting a hit.
    ///
    /// The controller detects ground a few centimetres early, so a landing there would carry
    /// no impact.
    fn probe_landing(&self, start: &Pose, translation: Vec3) -> Option<(ColliderHandle, Vec3)> {
        let landed_at = Pose::from_parts(start.translation + translation, start.rotation);
        let (handle, hit) =
            self.queries
                .cast_shape(&landed_at, -Vec3::Y, self.shape, cast_within(GROUND_PROBE))?;
        Some((handle, hit.normal1))
    }

    /// Retry a blocked slide from on top of the obstacle (Source's `StepMove`).
    ///
    /// The controller's own autostep fails on float noise when the capsule stops exactly its
    /// skin offset from a riser, so this lifts the capsule, carries the remaining horizontal
    /// motion across (at least the autostep's minimum width, so the capsule's rounded base ends
    /// over the step top), and settles it down. Returns the whole tick's translation, or `None`
    /// when that does not end grounded.
    fn step_up(&self, start: &Pose, asked: Vec3, blocked: Vec3) -> Option<Vec3> {
        let autostep = self.controller.autostep?;
        let (CharacterLength::Absolute(max_height), CharacterLength::Absolute(min_width)) =
            (autostep.max_height, autostep.min_width)
        else {
            return None;
        };
        let (dir, len) = step_reach((asked - blocked).with_y(0.0), min_width)?;

        let stopped = Pose::from_parts(start.translation + blocked, start.rotation);
        let lift = self
            .queries
            .cast_shape(&stopped, Vec3::Y, self.shape, cast_within(max_height))
            .map_or(max_height, |(_, hit)| hit.time_of_impact);
        if lift <= 0.0 {
            return None;
        }

        let lifted = Pose::from_parts(stopped.translation + Vec3::Y * lift, stopped.rotation);
        // Like rapier's own stair check: the lifted capsule must clear the whole reach, so a
        // riser taller than the lift is not climbed by riding up its rounded edge.
        if self
            .queries
            .cast_shape(&lifted, dir, self.shape, cast_within(len))
            .is_some()
        {
            return None;
        }
        let across = self.shift(&lifted, dir * len);
        let over = Pose::from_parts(lifted.translation + across.translation, lifted.rotation);
        let down = self.shift(&over, -Vec3::Y * (lift + STEP_SNAP));
        if !down.grounded {
            return None;
        }

        Some(blocked + Vec3::Y * lift + across.translation + down.translation)
    }

    /// Collide-and-slide from `from` without recording hits.
    fn shift(&self, from: &Pose, by: Vec3) -> EffectiveCharacterMovement {
        self.controller
            .move_shape(self.dt, self.queries, self.shape, from, by, |_| {})
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::TEST_TAG;
    use super::super::test_support::{quad_mesh, slope_mesh, step_box_mesh};
    use super::super::PhysicsWorld;
    use super::*;
    use moho_render_api::{WorldMesh, WorldMeshId};
    use rapier3d::prelude::{QueryFilter, SharedShape};

    fn hit(normal: Vec3) -> (ColliderHandle, Vec3) {
        (ColliderHandle::invalid(), normal)
    }

    #[test]
    fn walls_and_steep_slopes_are_risers() {
        assert!(is_riser(Vec3::NEG_Z));
        assert!(is_riser(Vec3::new(0.0, WALKABLE_NORMAL_Y - 1e-3, -0.7)));
        assert!(is_riser(Vec3::new(0.0, -CEILING_NORMAL_Y + 1e-3, -0.95)));
    }

    #[test]
    fn walkable_ground_and_ceilings_are_not_risers() {
        assert!(!is_riser(Vec3::Y));
        assert!(!is_riser(Vec3::new(
            0.0,
            WALKABLE_NORMAL_Y,
            -WALKABLE_NORMAL_Y
        )));
        assert!(!is_riser(Vec3::new(0.0, -CEILING_NORMAL_Y, -0.95)));
        assert!(!is_riser(Vec3::NEG_Y));
    }

    #[test]
    fn a_grounded_fall_without_a_ground_hit_needs_the_probe() {
        let falling = Vec3::new(0.0, -0.1, 0.0);
        assert!(landed_without_ground_hit(true, falling, &[]));
        assert!(landed_without_ground_hit(
            true,
            falling,
            &[hit(Vec3::NEG_Z)]
        ));
    }

    #[test]
    fn a_reported_ground_hit_a_level_move_or_staying_airborne_needs_no_probe() {
        let falling = Vec3::new(0.0, -0.1, 0.0);
        assert!(!landed_without_ground_hit(true, falling, &[hit(Vec3::Y)]));
        assert!(!landed_without_ground_hit(true, Vec3::Z, &[]));
        assert!(!landed_without_ground_hit(false, falling, &[]));
        assert!(!landed_without_ground_hit(false, Vec3::Z, &[]));
    }

    #[test]
    fn step_reach_is_at_least_the_minimum_width_along_the_remaining_motion() {
        assert_eq!(
            step_reach(Vec3::new(0.0, 0.0, 0.1), 0.2),
            Some((Vec3::Z, 0.2))
        );
        assert_eq!(
            step_reach(Vec3::new(0.5, 0.0, 0.0), 0.2),
            Some((Vec3::X, 0.5))
        );
    }

    #[test]
    fn step_reach_needs_the_minimum_progress() {
        assert_eq!(
            step_reach(Vec3::X * STEP_MIN_PROGRESS, 0.2),
            Some((Vec3::X, 0.2))
        );
        assert_eq!(step_reach(Vec3::X * (STEP_MIN_PROGRESS * 0.99), 0.2), None);
        assert_eq!(step_reach(Vec3::ZERO, 0.2), None);
    }

    /// Level ground at height `y` for z in [`z_from`, `z_to`].
    fn ground(y: f32, z_from: f32, z_to: f32) -> WorldMesh {
        quad_mesh(
            &[
                [-10.0, y, z_from],
                [10.0, y, z_from],
                [10.0, y, z_to],
                [-10.0, y, z_to],
            ],
            &[0, 2, 1, 0, 3, 2],
        )
    }

    /// A 0.3 m high, 5 cm thin wall at z = 2 over `beyond`, stepped up from a standing capsule
    /// touching it, with 1 m of motion left.
    fn step_up_over_thin_wall(beyond: &WorldMesh) -> Option<Vec3> {
        let mut world = PhysicsWorld::new();
        world.set_world_mesh(WorldMeshId(0), &ground(0.0, -10.0, 2.05), TEST_TAG);
        world.set_world_mesh(WorldMeshId(1), &step_box_mesh(2.0, 0.05, 0.3), TEST_TAG);
        world.set_world_mesh(WorldMeshId(2), beyond, TEST_TAG);
        world.step(1.0 / 60.0);
        let queries = world.broad_phase.as_query_pipeline(
            world.narrow_phase.query_dispatcher(),
            &world.rigid_body_set,
            &world.collider_set,
            QueryFilter::default(),
        );
        let capsule = SharedShape::capsule_y(0.85, 0.3);
        let slider = Slider {
            controller: &world.character_controller,
            queries: &queries,
            shape: capsule.as_ref(),
            dt: 1.0 / 60.0,
        };
        let touching = Pose::from_translation(Vec3::new(0.0, 1.16, 1.69));
        slider.step_up(&touching, Vec3::Z, Vec3::ZERO)
    }

    #[test]
    fn step_up_carries_over_a_thin_low_wall_onto_the_floor_behind() {
        let moved = step_up_over_thin_wall(&ground(0.0, 2.05, 10.0)).expect("stepped over");

        assert!(moved.z > 0.7, "{moved:?}");
        assert!(
            moved.y.abs() < 0.02,
            "settled back to floor height: {moved:?}"
        );
    }

    #[test]
    fn step_up_over_a_thin_wall_onto_a_drop_is_refused() {
        assert_eq!(step_up_over_thin_wall(&ground(-1.0, 2.05, 10.0)), None);
    }

    /// What `ground_under` reports for a capsule centred at `centre` over `meshes`.
    fn ground_under_centre(meshes: &[WorldMesh], centre: Vec3) -> Option<Vec3> {
        let mut world = PhysicsWorld::new();
        for (id, mesh) in meshes.iter().enumerate() {
            world.set_world_mesh(WorldMeshId(id as u64), mesh, TEST_TAG);
        }
        world.step(1.0 / 60.0);
        let queries = world.broad_phase.as_query_pipeline(
            world.narrow_phase.query_dispatcher(),
            &world.rigid_body_set,
            &world.collider_set,
            QueryFilter::default(),
        );
        let capsule = SharedShape::capsule_y(0.85, 0.3);
        let slider = Slider {
            controller: &world.character_controller,
            queries: &queries,
            shape: capsule.as_ref(),
            dt: 1.0 / 60.0,
        };
        slider.ground_under(centre).map(|ground| ground.normal)
    }

    /// A plane through the origin rising toward +Z at `degrees`.
    fn slope(degrees: f32) -> WorldMesh {
        let rise = degrees.to_radians().tan();
        slope_mesh(-10.0, -10.0 * rise, 10.0, 10.0 * rise)
    }

    #[test]
    fn ground_under_reports_the_face_below_the_centre() {
        let angle = 25.0_f32.to_radians();
        let tilted = Vec3::new(0.0, angle.cos(), -angle.sin());

        let flat = ground_under_centre(&[ground(0.0, -10.0, 10.0)], Vec3::new(0.0, 1.2, 0.0));
        let on_slope = ground_under_centre(&[slope(25.0)], Vec3::new(0.0, 1.27, 0.0));
        let at_reach = ground_under_centre(&[slope(40.0)], Vec3::new(1.0, 1.4, 0.0));
        let over_ledge = ground_under_centre(&[ground(0.0, -10.0, 1.0)], Vec3::new(0.0, 1.2, 0.5));

        assert!(flat.expect("flat ground below").distance(Vec3::Y) < 1e-3);
        assert!(
            on_slope.expect("slope below").distance(tilted) < 1e-3,
            "{on_slope:?}"
        );
        assert!(at_reach.is_some(), "a 40 degree face is walkable");
        assert!(over_ledge.is_some(), "the ray finds the ledge's top");
    }

    #[test]
    fn ground_under_is_none_past_a_ledge_and_on_steep_faces() {
        let ledge = || vec![ground(0.0, -10.0, 1.0)];
        let level = || vec![ground(0.0, -10.0, 10.0)];
        let cases: Vec<(&str, Vec<WorldMesh>, Vec3)> = vec![
            ("well past a ledge", ledge(), Vec3::new(0.0, 1.2, 1.5)),
            (
                "just past a ledge, where a shape cast would see its rounded edge",
                ledge(),
                Vec3::new(0.0, 1.2, 1.07),
            ),
            (
                "over a 50 degree face",
                vec![slope(50.0)],
                Vec3::new(0.0, 1.2, 0.0),
            ),
            (
                "beyond the capsule's reach",
                level(),
                Vec3::new(0.0, 1.6, 0.0),
            ),
            ("over nothing", Vec::new(), Vec3::new(0.0, 1.2, 0.0)),
        ];

        for (case, meshes, centre) in cases {
            assert_eq!(ground_under_centre(&meshes, centre), None, "{case}");
        }
    }
}
