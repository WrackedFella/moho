//! The rapier collide-and-slide behind [`crate::movement::step`].

use crate::movement::{Ground, Hull, HullShape, Slide, STEP_HEIGHT, WALKABLE_NORMAL_Y};
use glam::Vec3;
use rapier3d::control::{
    CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::parry::query::ShapeCastHit;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::parry::shape::Cylinder;
use rapier3d::prelude::{ColliderHandle, Pose, QueryPipeline, Ray, Shape};

/// How far below a grounded landing to look for the surface it landed on, in metres.
const GROUND_PROBE: f32 = 0.1;

/// Half the height of the thin cylinder whose flat base reads the ground, in metres.
const FLAT_BASE_HALF_HEIGHT: f32 = 0.005;

/// How far above and below the ground contact the face-normal ray reaches, in metres.
const FACE_PROBE: f32 = 0.05;

/// How far the face-normal ray starts inside the contact point, in metres.
const EDGE_INSET: f32 = 0.01;

/// Surfaces facing down at least this much (`normal.y` below its negative) are ceilings, not risers.
const CEILING_NORMAL_Y: f32 = 0.3;

/// Extra downward distance when settling onto a step top, in metres.
const STEP_SNAP: f32 = 0.05;

/// Horizontal motion left over below this many metres is not stepped up.
const STEP_MIN_PROGRESS: f32 = 1e-3;

/// The least speed, in m/s, at which a walker resting on an edge outside its flat base moves off it.
#[cfg(test)]
const ROLL_OFF_SPEED: f32 = 1.0;

/// How far to shift a walker on an edge, in metres, beyond its own `outward` motion this tick:
/// what [`ROLL_OFF_SPEED`] over `dt` lacks, at most `clearance`.
#[cfg(test)]
fn top_up(_outward: f32, _clearance: f32, _dt: f32) -> f32 {
    // TODO: stub for the roll-off tests; the implementation returns the clamped shortfall.
    0.0
}

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

/// A character hull as the ground query sees it: a core disc swept by a sphere.
///
/// A capsule is a point swept by its radius; a round cylinder is its flat base disc swept by
/// its rounding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Footing {
    /// Radius of the core disc.
    disc: f32,
    /// Radius of the sphere swept over the core.
    sweep: f32,
    /// How far below the hull's centre the core's bottom sits.
    core_bottom: f32,
}

impl Footing {
    /// The footing of a character `hull`.
    pub(super) fn of(hull: Hull) -> Self {
        let half_length = hull.length / 2.0;
        match hull.shape {
            HullShape::Capsule => Self {
                disc: 0.0,
                sweep: hull.radius,
                core_bottom: half_length - hull.radius,
            },
            HullShape::RoundCylinder { rounding } => Self {
                disc: hull.radius - rounding,
                sweep: rounding,
                core_bottom: half_length - rounding,
            },
        }
    }
}

/// The world as one character's collide-and-slide sees it.
pub(super) struct Slider<'a> {
    pub controller: &'a KinematicCharacterController,
    pub queries: &'a QueryPipeline<'a>,
    pub shape: &'a dyn Shape,
    pub footing: Footing,
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
                    ground: self.ground_below(start.translation + translation),
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
                .then(|| self.ground_below(start.translation + plain.translation))
                .flatten(),
            hits,
        }
    }

    /// The walkable surface the capsule centred at `centre` stands on, within [`STEP_HEIGHT`]
    /// of it, or `None` when there is none.
    ///
    /// Two casts straight down. A thin flat-based cylinder reads the face under the capsule:
    /// unlike the capsule's rounded base it reports a face normal on step edges and across
    /// seams, and its radius, tied to the walkable angle, is the perch margin. The capsule
    /// itself, cast down to its skin offset, gives the gap: how far it can drop before it
    /// touches anything.
    pub(super) fn ground_below(&self, centre: Vec3) -> Option<Ground> {
        let footing = self.footing;
        let CharacterLength::Absolute(skin) = self.controller.offset else {
            panic!("the world builds its character controller with an absolute skin offset");
        };

        let base_radius =
            footing.disc + footing.sweep * (1.0 - WALKABLE_NORMAL_Y * WALKABLE_NORMAL_Y).sqrt();
        let base = Cylinder::new(FLAT_BASE_HALF_HEIGHT, base_radius);
        let base_at = Pose::from_translation(centre - Vec3::Y * footing.core_bottom);
        let (collider, face) = self.queries.cast_shape(
            &base_at,
            -Vec3::Y,
            &base,
            cast_within(footing.sweep + STEP_HEIGHT),
        )?;
        let normal = self.face_normal(collider, &face, centre);
        if normal.y < WALKABLE_NORMAL_Y {
            return None;
        }

        let at = Pose::from_translation(centre);
        let skinned = ShapeCastOptions {
            target_distance: skin,
            ..cast_within(STEP_HEIGHT)
        };
        let gap = match self.queries.cast_shape(&at, -Vec3::Y, self.shape, skinned) {
            Some((_, drop)) => drop.time_of_impact,
            // Near a mesh boundary the skinned cast can miss though the flat base hit.
            None => {
                match self
                    .queries
                    .cast_shape(&at, -Vec3::Y, self.shape, cast_within(STEP_HEIGHT))
                {
                    Some((_, drop)) => {
                        let cosine = drop.normal1.y.max(WALKABLE_NORMAL_Y);
                        (drop.time_of_impact - skin / cosine).max(0.0)
                    }
                    None => (face.time_of_impact + FLAT_BASE_HALF_HEIGHT - footing.sweep - skin)
                        .max(0.0),
                }
            }
        };
        Some(Ground {
            normal,
            gap,
            collider,
            point: face.witness1,
        })
    }

    /// The face normal of the ground the flat base touched at `hit`, or the cast's own normal
    /// when neither lookup finds one.
    ///
    /// A shape cast that touches a mesh edge or a triangle boundary reports a normal tilted
    /// toward the shape, and its search noise tilts even a flat floor's by a few millionths;
    /// either would give a walker a spurious vertical speed along its ground. The hit
    /// triangle's own normal is exact. When the cast touches a riser's top edge the riser
    /// ties with the tread for first contact and its normal is not walkable, so a ray down onto
    /// the contact, nudged toward the cast's axis `centre` so an exact edge contact lands on the
    /// tread, reads the tread instead.
    fn face_normal(&self, collider: ColliderHandle, hit: &ShapeCastHit, centre: Vec3) -> Vec3 {
        let Some(found) = self.queries.colliders.get(collider) else {
            return hit.normal1;
        };
        let triangle = found
            .shape()
            .as_trimesh()
            .and_then(|mesh| mesh.triangle(hit.subshape1).normal())
            .map(|normal| found.position().rotation * normal)
            .map(|world| if world.y < 0.0 { -world } else { world })
            .filter(|normal| normal.y >= WALKABLE_NORMAL_Y);
        triangle
            .or_else(|| {
                let inward = (centre - hit.witness1).with_y(0.0).normalize_or_zero();
                let origin = hit.witness1 + inward * EDGE_INSET + Vec3::Y * FACE_PROBE;
                let ray = Ray::new(origin, -Vec3::Y);
                match self
                    .queries
                    .cast_ray_and_get_normal(&ray, 2.0 * FACE_PROBE, true)
                {
                    Some((handle, ray_hit)) if handle == collider => Some(ray_hit.normal),
                    _ => None,
                }
            })
            .unwrap_or(hit.normal1)
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
    use super::super::test_support::{
        quad_mesh, seam_floor_meshes, slope_mesh, step_box_mesh, wall_mesh,
    };
    use super::super::PhysicsWorld;
    use super::*;
    use crate::movement::CharacterBody;
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
            footing: Footing::of(CharacterBody::default().standing.hull),
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

    fn ground_below_centre(meshes: &[WorldMesh], centre: Vec3) -> Option<Vec3> {
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
            footing: Footing::of(CharacterBody::default().standing.hull),
            dt: 1.0 / 60.0,
        };
        slider.ground_below(centre).map(|ground| ground.normal)
    }

    fn slope(degrees: f32) -> WorldMesh {
        let rise = degrees.to_radians().tan();
        slope_mesh(-10.0, -10.0 * rise, 10.0, 10.0 * rise)
    }

    #[test]
    fn ground_below_reports_the_face_below_the_centre() {
        let angle = 25.0_f32.to_radians();
        let tilted = Vec3::new(0.0, angle.cos(), -angle.sin());

        let flat = ground_below_centre(&[ground(0.0, -10.0, 10.0)], Vec3::new(0.0, 1.2, 0.0));
        let on_slope = ground_below_centre(&[slope(25.0)], Vec3::new(0.0, 1.27, 0.0));
        let steep_walkable = ground_below_centre(&[slope(40.0)], Vec3::new(1.0, 1.4, 0.0));
        let over_ledge = ground_below_centre(&[ground(0.0, -10.0, 1.0)], Vec3::new(0.0, 1.2, 0.5));

        assert!(flat.expect("flat ground below").distance(Vec3::Y) < 1e-3);
        assert!(
            on_slope.expect("slope below").distance(tilted) < 1e-3,
            "{on_slope:?}"
        );
        assert!(steep_walkable.is_some(), "a 40 degree face is walkable");
        assert!(over_ledge.is_some(), "the ray finds the ledge's top");
    }

    #[test]
    fn ground_below_is_none_on_steep_faces_beyond_the_step_height_and_over_nothing() {
        let level = || vec![ground(0.0, -10.0, 10.0)];
        let cases: Vec<(&str, Vec<WorldMesh>, Vec3)> = vec![
            (
                "over a 50 degree face",
                vec![slope(50.0)],
                Vec3::new(0.0, 1.2, 0.0),
            ),
            (
                "beyond the step height",
                level(),
                Vec3::new(0.0, 1.15 + 0.55, 0.0),
            ),
            ("over nothing", Vec::new(), Vec3::new(0.0, 1.2, 0.0)),
        ];

        for (case, meshes, centre) in cases {
            assert_eq!(ground_below_centre(&meshes, centre), None, "{case}");
        }
    }

    /// Runs `check` with a slider for a standard character capsule over `meshes`.
    fn with_slider<T>(meshes: &[WorldMesh], check: impl FnOnce(&Slider) -> T) -> T {
        with_slider_and_colliders(meshes, |slider, _| check(slider))
    }

    /// Like [`with_slider`], also giving the collider of each mesh, in order.
    fn with_slider_and_colliders<T>(
        meshes: &[WorldMesh],
        check: impl FnOnce(&Slider, &[ColliderHandle]) -> T,
    ) -> T {
        let mut world = PhysicsWorld::new();
        let mut colliders = Vec::new();
        for (id, mesh) in meshes.iter().enumerate() {
            world.set_world_mesh(WorldMeshId(id as u64), mesh, TEST_TAG);
            colliders.push(
                world
                    .world_mesh_collider(WorldMeshId(id as u64))
                    .expect("mesh registered"),
            );
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
            footing: Footing::of(CharacterBody::default().standing.hull),
            dt: 1.0 / 60.0,
        };
        check(&slider, &colliders)
    }

    /// A character centre's height 2 cm above where a capsule rests on level ground at y = 0.
    const HOVER_Y: f32 = 1.17;

    #[test]
    fn ground_below_finds_a_ledge_under_the_flat_base_with_the_centre_over_the_void() {
        let ledge = [ground(0.0, -10.0, 1.0)];

        for past in [0.1_f32, 0.2] {
            let centre = Vec3::new(0.0, HOVER_Y, 1.0 + past);

            let (found, colliders) = with_slider_and_colliders(&ledge, |slider, colliders| {
                (slider.ground_below(centre), colliders.to_vec())
            });

            let found = found.unwrap_or_else(|| panic!("{past} m past the edge: no ground"));
            assert!(found.normal.distance(Vec3::Y) < 1e-3, "{past}: {found:?}");
            assert_eq!(found.collider, colliders[0], "{past}: {found:?}");
            assert!(found.point.y.abs() < 0.02, "{past}: {found:?}");
            assert!(found.point.z <= 1.0 + 1e-3, "{past}: {found:?}");
            assert!(
                (-1e-3..0.15).contains(&found.gap),
                "{past}: the capsule hangs just above the edge: {found:?}"
            );
        }
    }

    #[test]
    fn ground_below_finds_the_tread_just_behind_a_nosing_edge() {
        let ledge = [ground(0.0, -10.0, 1.0)];

        for z in [0.975_f32, 0.985, 0.995] {
            for y in [1.17_f32, 1.2] {
                let centre = Vec3::new(0.0, y, z);

                let found = with_slider(&ledge, |slider| slider.ground_below(centre))
                    .unwrap_or_else(|| panic!("z {z}, y {y}: no ground over the tread"));

                assert!(
                    found.normal.distance(Vec3::Y) < 1e-3,
                    "z {z}, y {y}: {found:?}"
                );
                // The drop stops a 0.0115 skin short of the tread.
                let drop = y - 1.15 - 0.0115;
                assert!(
                    (found.gap - drop).abs() < 0.005,
                    "z {z}, y {y}: gap should be about {drop}: {found:?}"
                );
            }
        }
    }

    #[test]
    fn ground_below_reads_a_downward_wound_floor_as_walkable() {
        let floor_facing_down = quad_mesh(
            &[
                [-10.0, 0.0, -10.0],
                [10.0, 0.0, -10.0],
                [10.0, 0.0, 1.0],
                [-10.0, 0.0, 1.0],
            ],
            &[0, 1, 2, 0, 2, 3],
        );
        // The centre is past the floor's edge, so the face-normal ray nudged toward it misses
        // and only the triangle's own (flipped) normal can answer.
        let centre = Vec3::new(0.0, HOVER_Y, 1.2);

        let found = with_slider(&[floor_facing_down], |slider| slider.ground_below(centre))
            .expect("a floor wound the other way is still ground");

        // The cast's own normal here is tilted by about 1e-4; only the triangle's exact normal,
        // flipped upward, is vertical to this tolerance.
        assert!(found.normal.distance(Vec3::Y) < 1e-6, "{found:?}");
    }

    #[test]
    fn ground_below_finds_no_ledge_past_the_flat_base() {
        let ledge = [ground(0.0, -10.0, 1.0)];
        let centre = Vec3::new(0.0, HOVER_Y, 1.25);

        let found = with_slider(&ledge, |slider| slider.ground_below(centre));

        assert_eq!(found, None);
    }

    /// An upper step with its top at 0.3 m ending in a nosing at z = 1, and the lower floor
    /// at y = 0 beyond it.
    fn nosing_over_a_lower_step() -> [WorldMesh; 2] {
        [ground(0.0, 1.0, 10.0), step_box_mesh(-3.0, 4.0, 0.3)]
    }

    /// Where the capsule centre stands on the upper step's top.
    const UPPER_STEP_REST_Y: f32 = 0.3 + 1.15;

    #[test]
    fn ground_below_sees_the_lower_step_past_a_nosing() {
        let centre = Vec3::new(0.0, UPPER_STEP_REST_Y, 1.25);

        let (found, colliders) =
            with_slider_and_colliders(&nosing_over_a_lower_step(), |slider, colliders| {
                (slider.ground_below(centre), colliders.to_vec())
            });

        let found = found.expect("the lower step is under the flat base");
        assert!(found.normal.distance(Vec3::Y) < 1e-3, "{found:?}");
        assert_eq!(found.collider, colliders[0], "the lower floor: {found:?}");
        assert!(found.point.y.abs() < 0.02, "{found:?}");
    }

    #[test]
    fn ground_gap_stops_where_the_capsule_meets_the_nosing() {
        let centre = Vec3::new(0.0, UPPER_STEP_REST_Y, 1.25);

        let found = with_slider(&nosing_over_a_lower_step(), |slider| {
            slider.ground_below(centre)
        })
        .expect("the lower step is under the flat base");

        // The rim's sphere (0.3 m, plus the 0.0115 skin) meets the nosing 0.25 m to the side:
        // it drops 0.3 - sqrt(0.3115^2 - 0.25^2) = 0.114 m, not the 0.3 m to the lower step.
        assert!(
            (0.10..0.13).contains(&found.gap),
            "the capsule can drop only until its rim meets the nosing: {found:?}"
        );
    }

    #[test]
    fn ground_below_reads_a_vertical_normal_on_a_step_edge() {
        let upper_step = [step_box_mesh(-3.0, 4.0, 0.3)];
        let centre = Vec3::new(0.0, UPPER_STEP_REST_Y, 1.1);

        let (found, colliders) = with_slider_and_colliders(&upper_step, |slider, colliders| {
            (slider.ground_below(centre), colliders.to_vec())
        });

        let found = found.expect("the step's top is under the flat base");
        assert!(
            found.normal.distance(Vec3::Y) < 1e-3,
            "the face normal, not the tilted edge normal: {found:?}"
        );
        assert_eq!(found.collider, colliders[0], "{found:?}");
        assert!((found.point.y - 0.3).abs() < 0.02, "{found:?}");
        assert!(found.point.z <= 1.0 + 1e-3, "{found:?}");
    }

    #[test]
    fn ground_below_bridges_a_seam_between_two_meshes() {
        let seam = seam_floor_meshes(0.025);
        let centre = Vec3::new(0.0, HOVER_Y, 0.0);

        let found = with_slider(&seam, |slider| slider.ground_below(centre))
            .expect("the capsule spans the 2.5 cm seam");

        assert!(found.normal.distance(Vec3::Y) < 1e-3, "{found:?}");
        assert!(found.point.y.abs() < 0.02, "{found:?}");
        assert!((-0.01..0.05).contains(&found.gap), "{found:?}");
    }

    #[test]
    fn ground_gap_is_how_far_the_capsule_drops_before_touching() {
        const SKIN: f32 = 0.0115;
        for degrees in [25.0_f32, 40.0] {
            let cos = degrees.to_radians().cos();
            let resting = 0.85 + 0.3 / cos;
            for (lift, expected) in [(0.0, 0.0), (0.05, 0.05 - SKIN / cos)] {
                let centre = Vec3::new(0.0, resting + lift, 0.0);

                let ground = with_slider(&[slope(degrees)], |slider| slider.ground_below(centre))
                    .expect("walkable face below");

                assert!(
                    (ground.gap - expected).abs() < 1e-3,
                    "{degrees} degrees, held {lift} m up, expected a drop of {expected}: {ground:?}"
                );
            }
        }
    }

    #[test]
    fn slide_reports_the_ground_where_the_capsule_ends_not_where_it_started() {
        let ledge = [ground(0.0, -10.0, 1.0)];
        let over_the_ledge = Pose::from_translation(Vec3::new(0.0, 1.17, 0.9));

        let slid = with_slider(&ledge, |slider| {
            slider.slide(&over_the_ledge, Vec3::new(0.0, 0.0, 0.5), true)
        });

        assert!(slid.translation.z > 0.4, "{slid:?}");
        assert_eq!(slid.ground, None, "the end is past the ledge: {slid:?}");
    }

    #[test]
    fn slide_reports_the_ground_on_top_of_a_step_it_climbed() {
        let meshes = [ground(0.0, -10.0, 10.0), step_box_mesh(2.0, 6.0, 0.45)];
        let pressed_against_the_riser = Pose::from_translation(Vec3::new(0.0, 1.17, 1.67));

        let slid = with_slider(&meshes, |slider| {
            slider.slide(&pressed_against_the_riser, Vec3::new(0.0, 0.0, 0.5), true)
        });

        assert!(slid.translation.y > 0.4, "climbed the step: {slid:?}");
        let ground = slid.ground.expect("standing on the step top");
        assert!(ground.normal.distance(Vec3::Y) < 1e-3, "{ground:?}");
        assert!(ground.gap.abs() < 0.05, "{ground:?}");
    }

    /// A walker idle on the nosing of [`nosing_over_a_lower_step`], its centre 0.25 m past it and
    /// its rim a skin's width from it, slid for one tick with `extra` meshes in the world.
    fn idle_slide_on_the_nosing(extra: Option<WorldMesh>) -> Slide {
        let mut meshes = nosing_over_a_lower_step().to_vec();
        meshes.extend(extra);
        let on_the_nosing = Pose::from_translation(Vec3::new(
            0.0,
            0.3 + 0.85 + (0.3115_f32 * 0.3115 - 0.25 * 0.25).sqrt(),
            1.25,
        ));

        with_slider(&meshes, |slider| {
            slider.slide(&on_the_nosing, Vec3::ZERO, true)
        })
    }

    #[test]
    fn the_roll_off_stops_at_a_wall() {
        // The capsule's front, plus its skin offset, is 1.5615; the wall is 4 mm beyond that.
        let free = idle_slide_on_the_nosing(None);
        let walled = idle_slide_on_the_nosing(Some(wall_mesh(1.5655)));

        assert!(
            (free.translation.z - ROLL_OFF_SPEED / 60.0).abs() < 1e-3,
            "an idle walker on the edge is shifted outward by the roll-off speed: {free:?}"
        );
        assert!(!free.grounded, "{free:?}");
        assert!(
            walled.translation.z < 0.0045,
            "the wall blocks the shift: {walled:?}"
        );
        assert!(walled.hits.is_empty(), "{walled:?}");
    }

    #[test]
    fn the_top_up_fills_only_what_the_walker_lacks() {
        let dt = 1.0 / 60.0;
        let wanted = ROLL_OFF_SPEED * dt;

        let own_motion_at_speed = top_up(wanted, 1.0, dt);
        let own_motion_above_speed = top_up(wanted * 2.0, 1.0, dt);
        let idle = top_up(0.0, 1.0, dt);
        let half_way = top_up(wanted / 2.0, 1.0, dt);
        let moving_inward = top_up(-0.01, 1.0, dt);
        let capped = top_up(0.0, 0.005, dt);

        assert_eq!(own_motion_at_speed, 0.0);
        assert_eq!(own_motion_above_speed, 0.0);
        assert!((idle - wanted).abs() < 1e-6, "{idle}");
        assert!((half_way - wanted / 2.0).abs() < 1e-6, "{half_way}");
        assert!(
            (moving_inward - (wanted + 0.01)).abs() < 1e-6,
            "{moving_inward}"
        );
        assert_eq!(capped, 0.005);
    }
}
