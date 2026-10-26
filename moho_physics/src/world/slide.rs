//! The collide-and-slide behind [`crate::movement::step`], on parry shape casts.

use crate::movement::{
    CharacterQueries, Ground, HullPose, HullShape, Slide, STEP_HEIGHT, WALKABLE_NORMAL_Y,
};
use crate::world::CharacterState;
use glam::{Quat, Vec3};
use rapier3d::parry::query::ShapeCastHit;
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::parry::shape::{Ball, Cylinder};
use rapier3d::prelude::{ColliderHandle, Pose, QueryPipeline, Ray, Shape, SharedShape};

/// Gap, in metres, the slide keeps between a character's hull and what it touches.
///
/// Fixed rather than relative to the hull so a different hull keeps the same skin.
pub(super) const SKIN: f32 = 0.0115;

const STEP_MIN_WIDTH: f32 = 0.2;

const MAX_SLIDE_PLANES: usize = 5;

const MIN_MOVE: f32 = 1e-5;

/// A ground gap at or below this, in metres, is touching the ground; a slide that is not
/// rising closes it.
const GROUND_CONTACT: f32 = 0.05;

/// A ground gap below this, in metres, is left open.
const SNAP_MIN: f32 = 1e-4;

/// A cast normal this close to its triangle's normal (dot product) is that face.
const SAME_FACE_DOT: f32 = 0.9999;

/// How far, in metres, each surface met pushes the rest of the motion off it, so the next
/// cast starts outside the skin (rapier's `normal_nudge_factor`).
const NUDGE: f32 = 1e-4;

/// How far motion may enter an earlier plane, as a dot product, and still count as leaving it.
const CLIP_TOLERANCE: f32 = 1e-6;

const DEPENETRATION_PASSES: usize = 4;

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
const ROLL_OFF_SPEED: f32 = 1.0;

/// A surface whose normal faces up more than this (`normal.y`) holds a capsule up on an edge.
const HOLDING_NORMAL_Y: f32 = 1e-3;

/// How far to shift a walker on an edge, in metres, beyond its own `outward` motion this tick:
/// what [`ROLL_OFF_SPEED`] over `dt` lacks, at most `clearance`. A walker heading inward gets
/// none: the shift would fold into its translation and read as a hit on the edge it left.
fn top_up(outward: f32, clearance: f32, dt: f32) -> f32 {
    if outward < 0.0 {
        return 0.0;
    }
    (ROLL_OFF_SPEED * dt - outward).clamp(0.0, clearance)
}

/// An edge holding a walker outside its flat base: the way off it and how far to go.
#[derive(Clone, Copy, Debug)]
struct Edge {
    /// Level unit direction pointing away from the edge's face.
    outward: Vec3,
    /// The level move after which the capsule's straight drop no longer meets the edge.
    clearance: f32,
}

/// The ground under a character and, when an edge outside its flat base holds it up, that edge.
struct GroundRead {
    ground: Ground,
    edge: Option<Edge>,
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

fn cast_skinned(distance: f32) -> ShapeCastOptions {
    ShapeCastOptions {
        target_distance: SKIN,
        ..cast_within(distance)
    }
}

/// A surface a grounded character may step up onto: too steep to walk on, not a ceiling.
fn is_riser(normal: Vec3) -> bool {
    normal.y < WALKABLE_NORMAL_Y && normal.y > -CEILING_NORMAL_Y
}

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

/// `motion` clipped against the `planes` (unit normals, facing the hull) it met, in order.
///
/// Quake's `SV_FlyMove`: the part entering the last plane is removed; if the result still
/// enters an earlier plane, two planes send it along their crease and any more stop it. Motion
/// leaving a plane is kept.
fn clip_motion(motion: Vec3, planes: &[Vec3]) -> Vec3 {
    let Some((&last, earlier)) = planes.split_last() else {
        return motion;
    };
    let along = motion - last * motion.dot(last).min(0.0);
    if earlier
        .iter()
        .all(|&plane| along.dot(plane) >= -CLIP_TOLERANCE)
    {
        return along;
    }
    match planes {
        &[previous, last] => {
            let crease = previous.cross(last).normalize_or_zero();
            crease * motion.dot(crease)
        }
        _ => Vec3::ZERO,
    }
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
    /// The hull lies along the facing, so its ground casts turn with the yaw.
    lying: bool,
}

impl Footing {
    /// The footing of a character `pose`. A lying hull's core is level with its centre, swept
    /// by its radius with no core disc.
    pub(super) fn of(pose: HullPose) -> Self {
        match pose {
            HullPose::Lying { radius, .. } => Self {
                disc: 0.0,
                sweep: radius,
                core_bottom: 0.0,
                lying: true,
            },
            HullPose::Upright(hull) => {
                let half_length = hull.length / 2.0;
                match hull.shape {
                    HullShape::Capsule => Self {
                        disc: 0.0,
                        sweep: hull.radius,
                        core_bottom: half_length - hull.radius,
                        lying: false,
                    },
                    HullShape::RoundCylinder { rounding } => Self {
                        disc: hull.radius - rounding,
                        sweep: rounding,
                        core_bottom: half_length - rounding,
                        lying: false,
                    },
                }
            }
        }
    }
}

/// The world as one character's collide-and-slide sees it.
#[derive(Clone, Copy)]
pub(super) struct Slider<'a> {
    pub queries: &'a QueryPipeline<'a>,
    pub shape: &'a dyn Shape,
    pub footing: Footing,
    pub dt: f32,
}

/// The collider shape of a character `pose`; a lying hull is a capsule along the body's +Z.
pub(super) fn shape_of(pose: HullPose) -> SharedShape {
    match pose {
        HullPose::Lying { radius, length } => SharedShape::capsule_z(length / 2.0 - radius, radius),
        HullPose::Upright(hull) => {
            let half_length = hull.length / 2.0;
            match hull.shape {
                HullShape::Capsule => {
                    SharedShape::capsule_y(half_length - hull.radius, hull.radius)
                }
                HullShape::RoundCylinder { rounding } => SharedShape::round_cylinder(
                    half_length - rounding,
                    hull.radius - rounding,
                    rounding,
                ),
            }
        }
    }
}

/// The world as one character's step sees it: the collider the character holds, built into a
/// [`Slider`] for whichever hull the step asks about.
pub(super) struct WorldQueries<'a> {
    pub queries: &'a QueryPipeline<'a>,
    pub dt: f32,
    /// The posed hull `own_shape` was built from.
    pub own_pose: HullPose,
    /// The character's collider shape, reused while the step asks about `own_pose`.
    pub own_shape: &'a dyn Shape,
}

impl CharacterQueries for WorldQueries<'_> {
    /// Slides the `pose` given, which differs from the collider's shape on the tick the stance
    /// changed it, before the adapter reshapes the collider.
    fn slide(&self, from: &CharacterState, pose: HullPose, asked: Vec3) -> Slide {
        let built;
        let shape = if pose == self.own_pose {
            self.own_shape
        } else {
            built = shape_of(pose);
            built.as_ref()
        };
        let slider = Slider {
            queries: self.queries,
            shape,
            footing: Footing::of(pose),
            dt: self.dt,
        };
        let start = Pose::from_parts(from.position, Quat::from_rotation_y(from.yaw));
        slider.slide(&start, asked, from.grounded)
    }

    /// The hull at its true size: a grounded character rests a skin above the floor, so
    /// growing it by the skin would make every crouch in the open touch the floor.
    fn fits(&self, pose: HullPose, centre: Vec3, yaw: f32) -> bool {
        let at = Pose::from_parts(centre, Quat::from_rotation_y(yaw));
        let built;
        let shape = if pose == self.own_pose {
            self.own_shape
        } else {
            built = shape_of(pose);
            built.as_ref()
        };
        let hit = self.queries.intersect_shape(at, shape).next().is_some();
        !hit
    }

    /// The cast ignores the character's own hull and sensors through the pipeline's filter, and
    /// stops a skin short of what it meets.
    fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> f32 {
        let path = to - from;
        let length = path.length();
        if length == 0.0 {
            return 1.0;
        }
        let ball = Ball::new(radius);
        let hit = self.queries.cast_shape(
            &Pose::from_translation(from),
            path / length,
            &ball,
            cast_within(length),
        );
        hit.map_or(1.0, |(_, hit)| {
            ((hit.time_of_impact - SKIN) / length).clamp(0.0, 1.0)
        })
    }
}

impl Slider<'_> {
    /// Move the capsule from `start` by `asked` and report what it touched.
    ///
    /// A grounded character blocked by a riser retries from on top of it. The slide is grounded
    /// when walkable ground lies within [`GROUND_CONTACT`] where it ends, and a falling slide
    /// that lands without having met that ground records it as a hit.
    pub(super) fn slide(&self, start: &Pose, asked: Vec3, was_grounded: bool) -> Slide {
        let mut hits = Vec::new();
        // Only a hull asked to stay put is pushed out: a moving one's casts stop it at what it
        // meets, and parry's overlap test misreports some capsule and trimesh contacts.
        let plain = if asked.length() < MIN_MOVE {
            self.depenetrate(start)
        } else {
            self.collide_and_slide(start, asked, Some(&mut hits))
        };

        if was_grounded && hits.iter().any(|&(_, normal)| is_riser(normal)) {
            if let Some(translation) = self.step_up(start, asked, plain) {
                return Slide {
                    translation,
                    grounded: true,
                    ground: self.ground_below(start.translation + translation, start.rotation),
                    hits,
                };
            }
        }

        let (plain, read) = self.close_gap(start, plain, asked);
        let grounded = read
            .as_ref()
            .is_some_and(|read| read.ground.gap <= GROUND_CONTACT);
        if landed_without_ground_hit(grounded, asked, &hits) {
            if let Some(read) = &read {
                hits.push((read.ground.collider, read.ground.normal));
            }
        }
        // Only a walker rolls off an edge, and only a character that started grounded walks.
        if let Some(rolled) = read
            .as_ref()
            .filter(|_| was_grounded)
            .and_then(|read| read.edge)
            .and_then(|edge| {
                let lacking = top_up(plain.dot(edge.outward), edge.clearance, self.dt);
                (lacking > 0.0).then(|| edge.outward * lacking)
            })
        {
            let ended = Pose::from_parts(start.translation + plain, start.rotation);
            let (translation, read) =
                self.close_gap(start, plain + self.shift(&ended, rolled), asked);
            return Slide {
                translation,
                grounded: false,
                ground: read.map(|read| read.ground),
                hits,
            };
        }
        Slide {
            translation: plain,
            grounded,
            ground: read.map(|read| read.ground),
            hits,
        }
    }

    /// The ground read where a slide from `start` by `translation` ends, after lowering a slide
    /// that was not `asked` to rise onto walkable ground within [`GROUND_CONTACT`].
    /// Returns the translation and the read.
    fn close_gap(
        &self,
        start: &Pose,
        translation: Vec3,
        asked: Vec3,
    ) -> (Vec3, Option<GroundRead>) {
        let read = self.read_ground(start.translation + translation, start.rotation);
        match read.as_ref().map(|read| read.ground.gap) {
            Some(gap) if asked.y <= 0.0 && gap > SNAP_MIN && gap <= GROUND_CONTACT => {
                let lowered = translation - Vec3::Y * gap;
                (
                    lowered,
                    self.read_ground(start.translation + lowered, start.rotation),
                )
            }
            _ => (translation, read),
        }
    }

    /// Slides the hull from `from` by `by`, recording each surface met in `hits`; returns the
    /// translation.
    ///
    /// A surface too steep to walk on does not lift a motion that was not rising: the motion
    /// slides along it level, so walking into a steep face never climbs it (rapier's
    /// `handle_slopes`).
    fn collide_and_slide(
        &self,
        from: &Pose,
        by: Vec3,
        mut hits: Option<&mut Vec<(ColliderHandle, Vec3)>>,
    ) -> Vec3 {
        let mut moved = Vec3::ZERO;
        let mut remaining = by;
        let mut planes = [Vec3::ZERO; MAX_SLIDE_PLANES];
        for met in 0..MAX_SLIDE_PLANES {
            let length = remaining.length();
            if length < MIN_MOVE {
                break;
            }
            let dir = remaining / length;
            let at = Pose::from_parts(from.translation + moved, from.rotation);
            let Some((handle, hit)) =
                self.queries
                    .cast_shape(&at, dir, self.shape, cast_skinned(length))
            else {
                moved += remaining;
                break;
            };
            moved += dir * hit.time_of_impact;
            remaining -= dir * hit.time_of_impact;
            let normal = self.exact_normal(handle, &hit);
            if let Some(hits) = hits.as_deref_mut() {
                hits.push((handle, normal));
            }
            let along = remaining - normal * remaining.dot(normal).min(0.0);
            let level = normal.with_y(0.0).normalize_or_zero();
            let plane = if by.y <= 0.0 && along.y > 0.0 && is_riser(normal) && level != Vec3::ZERO {
                level
            } else {
                normal
            };
            planes[met] = plane;
            remaining = clip_motion(remaining, &planes[..=met]) + normal * NUDGE;
        }
        moved
    }

    /// The hit's normal, snapped to its triangle's face normal when the cast's own normal is
    /// that face's up to search noise.
    fn exact_normal(&self, collider: ColliderHandle, hit: &ShapeCastHit) -> Vec3 {
        self.triangle_normal(collider, hit)
            .map(|world| {
                if world.dot(hit.normal1) < 0.0 {
                    -world
                } else {
                    world
                }
            })
            .filter(|facing| facing.dot(hit.normal1) > SAME_FACE_DOT)
            .unwrap_or(hit.normal1)
    }

    /// Either-way-up normal of the touched trimesh triangle; `None` for other colliders.
    fn triangle_normal(&self, collider: ColliderHandle, hit: &ShapeCastHit) -> Option<Vec3> {
        let found = self.queries.colliders.get(collider)?;
        let normal = found
            .shape()
            .as_trimesh()?
            .triangle(hit.subshape1)
            .normal()?;
        Some(found.position().rotation * normal)
    }

    fn depenetrate(&self, from: &Pose) -> Vec3 {
        let mut pushed = Vec3::ZERO;
        for _ in 0..DEPENETRATION_PASSES {
            let at = Pose::from_parts(from.translation + pushed, from.rotation);
            let overlap =
                self.queries
                    .intersect_shape(at, self.shape)
                    .find_map(|(handle, collider)| {
                        let pos12 = at.inv_mul(collider.position());
                        self.queries
                            .dispatcher
                            .contact(&pos12, self.shape, collider.shape(), 0.0)
                            .inspect_err(|unsupported| {
                                tracing::warn!(
                                    ?handle,
                                    ?unsupported,
                                    "no contact query for an overlapped shape"
                                );
                            })
                            .ok()
                            .flatten()
                            .filter(|contact| contact.dist < 0.0)
                    });
            let Some(contact) = overlap else {
                break;
            };
            pushed -= (at.rotation * contact.normal1) * (SKIN - contact.dist);
        }
        pushed
    }

    /// The walkable surface the capsule centred at `centre` stands on, within [`STEP_HEIGHT`]
    /// of it, or `None` when there is none.
    ///
    /// Two casts straight down. A thin flat-based cylinder reads the face under the capsule:
    /// unlike the capsule's rounded base it reports a face normal on step edges and across
    /// seams, and its radius, tied to the walkable angle, is the perch margin. The capsule
    /// itself, cast down to its skin offset, gives the gap: how far it can drop before it
    /// touches anything. When the flat base meets a face too steep to walk on, a ray down the
    /// capsule's axis reads the surface under it instead.
    ///
    /// `rotation` turns the cast and matters only for a lying footing, which faces along the
    /// yaw; an upright hull is round about the vertical and ignores it.
    fn ground_below(&self, centre: Vec3, rotation: Quat) -> Option<Ground> {
        self.read_ground(centre, rotation).map(|read| read.ground)
    }

    /// [`Self::ground_below`] with the edge that holds the capsule up, if any.
    fn read_ground(&self, centre: Vec3, rotation: Quat) -> Option<GroundRead> {
        let footing = self.footing;
        let rotation = if footing.lying {
            rotation
        } else {
            Quat::IDENTITY
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
        let (collider, normal, face_toi, point) = if normal.y >= WALKABLE_NORMAL_Y {
            (collider, normal, face.time_of_impact, face.witness1)
        } else {
            // A steep face under the flat base does not hide the floor under the axis (Source's
            // `TryTouchGroundInQuadrants`).
            let ray = Ray::new(base_at.translation, -Vec3::Y);
            let (handle, hit) = self.queries.cast_ray_and_get_normal(
                &ray,
                footing.sweep + STEP_HEIGHT + FLAT_BASE_HALF_HEIGHT,
                true,
            )?;
            if hit.normal.y < WALKABLE_NORMAL_Y {
                return None;
            }
            (
                handle,
                hit.normal,
                hit.time_of_impact - FLAT_BASE_HALF_HEIGHT,
                ray.point_at(hit.time_of_impact),
            )
        };

        let at = Pose::from_parts(centre, rotation);
        let mut edge = None;
        let gap =
            match self
                .queries
                .cast_shape(&at, -Vec3::Y, self.shape, cast_skinned(STEP_HEIGHT))
            {
                Some((_, drop)) => {
                    // The face under the flat base lies below where the capsule's drop stops.
                    let held_above =
                        face_toi + FLAT_BASE_HALF_HEIGHT - footing.sweep - drop.time_of_impact;
                    let across = drop.normal1.with_y(0.0);
                    let reach = footing.disc + (footing.sweep + SKIN) * across.length();
                    if drop.normal1.y > HOLDING_NORMAL_Y && reach > base_radius && held_above > SKIN
                    {
                        edge = Some(Edge {
                            outward: across.normalize(),
                            clearance: (footing.sweep + SKIN) * (1.0 - across.length()) + SKIN,
                        });
                    }
                    drop.time_of_impact
                }
                // Near a mesh boundary the skinned cast can miss though the flat base hit.
                None => {
                    match self.queries.cast_shape(
                        &at,
                        -Vec3::Y,
                        self.shape,
                        cast_within(STEP_HEIGHT),
                    ) {
                        Some((_, drop)) => {
                            let cosine = drop.normal1.y.max(WALKABLE_NORMAL_Y);
                            (drop.time_of_impact - SKIN / cosine).max(0.0)
                        }
                        None => (face_toi + FLAT_BASE_HALF_HEIGHT - footing.sweep - SKIN).max(0.0),
                    }
                }
            };
        Some(GroundRead {
            ground: Ground {
                normal,
                gap,
                collider,
                point,
            },
            edge,
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
        let triangle = self
            .triangle_normal(collider, hit)
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

    /// Retry a blocked slide from on top of the obstacle (Source's `StepMove`).
    ///
    /// The reach is at least [`STEP_MIN_WIDTH`] so the capsule's rounded base ends over the
    /// step top. Returns the whole tick's translation, or `None` when that top is not walkable.
    fn step_up(&self, start: &Pose, asked: Vec3, blocked: Vec3) -> Option<Vec3> {
        let (dir, len) = step_reach((asked - blocked).with_y(0.0), STEP_MIN_WIDTH)?;

        let stopped = Pose::from_parts(start.translation + blocked, start.rotation);
        let lift = self
            .queries
            .cast_shape(&stopped, Vec3::Y, self.shape, cast_within(STEP_HEIGHT))
            .map_or(STEP_HEIGHT, |(_, hit)| hit.time_of_impact);
        if lift <= 0.0 {
            return None;
        }

        let lifted = Pose::from_parts(stopped.translation + Vec3::Y * lift, stopped.rotation);
        // The lifted capsule must clear the whole reach, so a riser taller than the lift is not
        // climbed by riding up its rounded edge.
        if self
            .queries
            .cast_shape(&lifted, dir, self.shape, cast_within(len))
            .is_some()
        {
            return None;
        }
        let across = self.shift(&lifted, dir * len);
        let over = Pose::from_parts(lifted.translation + across, lifted.rotation);
        // A cast, not a slide: sliding down onto a riser's rounded top edge slides the hull
        // back off the step. The step top must be walkable where the hull lands.
        let (handle, hit) =
            self.queries
                .cast_shape(&over, -Vec3::Y, self.shape, cast_skinned(lift + STEP_SNAP))?;
        if self.exact_normal(handle, &hit).y < WALKABLE_NORMAL_Y {
            return None;
        }
        let down = -Vec3::Y * hit.time_of_impact;

        Some(blocked + Vec3::Y * lift + across + down)
    }

    /// Collide-and-slide from `from` without recording hits.
    fn shift(&self, from: &Pose, by: Vec3) -> Vec3 {
        self.collide_and_slide(from, by, None)
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
    use crate::Hull;
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
            queries: &queries,
            shape: capsule.as_ref(),
            footing: Footing::of(HullPose::Upright(CharacterBody::default().standing.hull)),
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
            queries: &queries,
            shape: capsule.as_ref(),
            footing: Footing::of(HullPose::Upright(CharacterBody::default().standing.hull)),
            dt: 1.0 / 60.0,
        };
        slider
            .ground_below(centre, Quat::IDENTITY)
            .map(|ground| ground.normal)
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
        with_slider_ticking(meshes, 1.0 / 60.0, check)
    }

    /// Like [`with_slider_and_colliders`], for ticks of `dt` seconds.
    fn with_slider_ticking<T>(
        meshes: &[WorldMesh],
        dt: f32,
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
            queries: &queries,
            shape: capsule.as_ref(),
            footing: Footing::of(HullPose::Upright(CharacterBody::default().standing.hull)),
            dt,
        };
        check(&slider, &colliders)
    }

    /// The 1.8 m long, 0.3 m radius hull of a prone character.
    const LYING_HULL: Hull = Hull {
        shape: HullShape::Capsule,
        radius: 0.3,
        length: 1.8,
    };

    /// The ground a prone character's hull, lying along its facing `yaw`, reads with its centre
    /// at `centre`.
    fn ground_below_lying(meshes: &[WorldMesh], centre: Vec3, yaw: f32) -> Option<Ground> {
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
        let capsule = SharedShape::capsule_z(0.6, 0.3);
        let slider = Slider {
            queries: &queries,
            shape: capsule.as_ref(),
            footing: Footing::of(HullPose::Lying {
                radius: LYING_HULL.radius,
                length: LYING_HULL.length,
            }),
            dt: 1.0 / 60.0,
        };
        slider.ground_below(centre, Quat::from_rotation_y(yaw))
    }

    #[test]
    fn ground_below_a_lying_hull_reads_the_floor_at_each_hover_height() {
        for hover in [0.02_f32, 0.2] {
            let centre = Vec3::new(0.0, 0.3 + hover, 0.0);

            let found = ground_below_lying(&[ground(0.0, -10.0, 10.0)], centre, 0.0)
                .unwrap_or_else(|| panic!("hover {hover}: no ground"));

            assert!(found.normal.distance(Vec3::Y) < 1e-3, "{hover}: {found:?}");
            assert!(found.point.y.abs() < 0.02, "{hover}: {found:?}");
            // The drop stops a 0.0115 skin short of the floor.
            assert!(
                (found.gap - (hover - 0.0115)).abs() < 0.005,
                "{hover}: {found:?}"
            );
        }
    }

    #[test]
    fn ground_below_a_lying_hull_casts_it_along_the_facing() {
        // A 0.1 m kerb under the head end at z in [0.5, 1.0]: the hull lying along Z overhangs
        // it, the hull lying along X does not.
        let meshes = [ground(0.0, -10.0, 10.0), step_box_mesh(0.5, 0.5, 0.1)];
        let centre = Vec3::new(0.0, 0.3 + 0.15, 0.0);

        let along_z = ground_below_lying(&meshes, centre, 0.0).expect("ground under the centre");
        let along_x = ground_below_lying(&meshes, centre, std::f32::consts::FRAC_PI_2)
            .expect("ground under the centre");

        assert!(
            along_z.gap < 0.07,
            "the kerb stops the head end: {along_z:?}"
        );
        assert!(
            (along_x.gap - (0.15 - 0.0115)).abs() < 0.01,
            "turned along X it clears the kerb: {along_x:?}"
        );
    }

    #[test]
    fn ground_below_a_lying_hull_is_none_beyond_the_step_height() {
        let found = ground_below_lying(
            &[ground(0.0, -10.0, 10.0)],
            Vec3::new(0.0, 0.3 + 0.55, 0.0),
            0.0,
        );

        assert_eq!(found, None);
    }

    /// A character centre's height 2 cm above where a capsule rests on level ground at y = 0.
    const HOVER_Y: f32 = 1.17;

    #[test]
    fn ground_below_finds_a_ledge_under_the_flat_base_with_the_centre_over_the_void() {
        let ledge = [ground(0.0, -10.0, 1.0)];

        for past in [0.1_f32, 0.2] {
            let centre = Vec3::new(0.0, HOVER_Y, 1.0 + past);

            let (found, colliders) = with_slider_and_colliders(&ledge, |slider, colliders| {
                (
                    slider.ground_below(centre, Quat::IDENTITY),
                    colliders.to_vec(),
                )
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

                let found =
                    with_slider(&ledge, |slider| slider.ground_below(centre, Quat::IDENTITY))
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

        let found = with_slider(&[floor_facing_down], |slider| {
            slider.ground_below(centre, Quat::IDENTITY)
        })
        .expect("a floor wound the other way is still ground");

        // The cast's own normal here is tilted by about 1e-4; only the triangle's exact normal,
        // flipped upward, is vertical to this tolerance.
        assert!(found.normal.distance(Vec3::Y) < 1e-6, "{found:?}");
    }

    #[test]
    fn ground_below_finds_no_ledge_past_the_flat_base() {
        let ledge = [ground(0.0, -10.0, 1.0)];
        let centre = Vec3::new(0.0, HOVER_Y, 1.25);

        let found = with_slider(&ledge, |slider| slider.ground_below(centre, Quat::IDENTITY));

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
                (
                    slider.ground_below(centre, Quat::IDENTITY),
                    colliders.to_vec(),
                )
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
            slider.ground_below(centre, Quat::IDENTITY)
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
            (
                slider.ground_below(centre, Quat::IDENTITY),
                colliders.to_vec(),
            )
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

        let found = with_slider(&seam, |slider| slider.ground_below(centre, Quat::IDENTITY))
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

                let ground = with_slider(&[slope(degrees)], |slider| {
                    slider.ground_below(centre, Quat::IDENTITY)
                })
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
    fn an_idle_walker_on_an_edge_is_rolled_off_at_the_roll_off_speed() {
        let free = idle_slide_on_the_nosing(None);

        assert!(
            (free.translation.z - 1.0 / 60.0).abs() < 1e-3,
            "an idle walker on the edge is shifted outward by 1 m/s over one tick: {free:?}"
        );
    }

    #[test]
    fn the_roll_off_stops_at_a_wall() {
        // The capsule's front, plus its skin offset, is 1.5615; the wall is 4 mm beyond that.
        let walled = idle_slide_on_the_nosing(Some(wall_mesh(1.5655)));

        assert!(
            (0.001..0.0045).contains(&walled.translation.z),
            "the shift goes as far as the wall allows: {walled:?}"
        );
        assert!(walled.hits.is_empty(), "{walled:?}");
    }

    #[test]
    fn a_sweep_stops_a_skin_short_of_a_wall_and_ignores_one_behind_it() {
        let (toward, away) = with_slider(&[wall_mesh(2.0)], |slider| {
            let world = WorldQueries {
                queries: slider.queries,
                dt: slider.dt,
                own_pose: HullPose::Upright(CharacterBody::default().standing.hull),
                own_shape: slider.shape,
            };
            let from = Vec3::new(5.0, 1.5, 0.5);
            let toward = world.sweep(from, from + Vec3::new(0.0, 0.0, 2.0), 0.1);
            let away = world.sweep(from, from - Vec3::new(0.0, 0.0, 1.0), 0.1);
            (toward, away)
        });
        // The ball's front meets the wall after 1.4 m of the 2 m path.
        let expected = (1.4 - SKIN) / 2.0;
        assert!((toward - expected).abs() < 1e-4, "{toward} vs {expected}");
        assert_eq!(away, 1.0);
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

        assert!(own_motion_at_speed.abs() < 1e-6, "{own_motion_at_speed}");
        assert_eq!(own_motion_above_speed, 0.0);
        assert!((idle - wanted).abs() < 1e-6, "{idle}");
        assert!((half_way - wanted / 2.0).abs() < 1e-6, "{half_way}");
        assert_eq!(moving_inward, 0.0);
        assert_eq!(capped, 0.005);
    }

    /// One slide of a walker at `centre` on the nosing of [`nosing_over_a_lower_step`] asked to
    /// move `asked`, in ticks of `dt`.
    fn slide_on_the_nosing(centre: Vec3, asked: Vec3, dt: f32) -> Slide {
        with_slider_ticking(&nosing_over_a_lower_step(), dt, |slider, _| {
            slider.slide(&Pose::from_translation(centre), asked, true)
        })
    }

    /// Where the capsule centre rests with its rim a skin's width from the nosing, `past` m
    /// beyond it.
    fn rim_on_the_nosing(past: f32) -> Vec3 {
        Vec3::new(
            0.0,
            0.3 + 0.85 + (0.3115_f32 * 0.3115 - past * past).sqrt(),
            1.0 + past,
        )
    }

    #[test]
    fn the_roll_off_is_capped_at_the_clearance() {
        // 0.25 m past, the rim's contact normal leans 0.25 / 0.3115 toward the walker's side, so
        // the level move that clears the nosing is 0.3115 * (1 - 0.25 / 0.3115) + 0.0115 = 0.073.
        let rolled = slide_on_the_nosing(rim_on_the_nosing(0.25), Vec3::ZERO, 0.5);

        assert!(
            (rolled.translation.z - 0.073).abs() < 1e-3,
            "a 0.5 s tick would roll 0.5 m, but stops once clear of the nosing: {rolled:?}"
        );
    }

    #[test]
    fn a_walker_moving_off_partly_gets_only_the_rest_of_the_roll_off_speed() {
        // Starts 0.2085 m past, inside the 0.2121 m flat base, and its own 5 mm carries it
        // outside; the edge is read where it ends up, and its tick totals 1 m/s.
        let moved = slide_on_the_nosing(
            rim_on_the_nosing(0.2085),
            Vec3::new(0.0, 0.0, 0.005),
            1.0 / 60.0,
        );

        assert!((moved.translation.z - 1.0 / 60.0).abs() < 5e-4, "{moved:?}");
        assert!(!moved.grounded, "{moved:?}");
        // The shift rides down the rim: from 0.2252 m past the rim sits 0.2152 m above the
        // nosing, 0.0162 m below where the walker started, and the snap drops it 0.0157.
        assert!((moved.translation.y + 0.0157).abs() < 0.003, "{moved:?}");
        let ground = moved.ground.expect("the lower floor is under the walker");
        assert!(ground.gap < 0.004, "read where it ended: {ground:?}");
    }

    #[test]
    fn the_roll_off_starts_from_where_the_walkers_own_motion_ended() {
        // The walker's own 5 mm ends 0.2135 m past the nosing, its front plus skin at 1.525; a
        // wall 3 mm beyond leaves 3 mm to roll. Started from before its own motion, the roll would
        // have 8 mm.
        let moved = with_slider(
            &[
                nosing_over_a_lower_step()[0].clone(),
                nosing_over_a_lower_step()[1].clone(),
                wall_mesh(1.528),
            ],
            |slider| {
                slider.slide(
                    &Pose::from_translation(rim_on_the_nosing(0.2085)),
                    Vec3::new(0.0, 0.0, 0.005),
                    true,
                )
            },
        );

        assert!(
            (moved.translation.z - 0.008).abs() < 1e-3,
            "its own 5 mm, then the wall's 3 mm: {moved:?}"
        );
    }

    /// An idle capsule 0.25 m past the nosing of a `step` m high step over a floor, at centre
    /// height `y`, slid for one tick.
    fn idle_slide_past_a_low_step(step: f32, y: f32) -> Slide {
        let meshes = [ground(0.0, 1.0, 10.0), step_box_mesh(-3.0, 4.0, step)];
        with_slider(&meshes, |slider| {
            slider.slide(
                &Pose::from_translation(Vec3::new(0.0, y, 1.25)),
                Vec3::ZERO,
                true,
            )
        })
    }

    /// Level ground and a 60 degree face rising from it at z = 1.
    fn floor_with_a_steep_face() -> [WorldMesh; 2] {
        let rise = 60.0_f32.to_radians().tan();
        [
            ground(0.0, -10.0, 10.0),
            slope_mesh(1.0, 0.0, 3.0, 2.0 * rise),
        ]
    }

    /// Where a capsule centre rests on level ground at y = 0, a skin above it.
    const FLOOR_REST_Y: f32 = 0.3 + 0.85 + 0.0115;

    #[test]
    fn ground_below_at_the_foot_of_a_steep_face_reads_the_floor_under_the_axis() {
        // The flat base's first contact here is the face, 0.18 m ahead; the axis is over the floor.
        let centre = Vec3::new(0.0, FLOOR_REST_Y, 0.82);

        let found = ground_below_centre(&floor_with_a_steep_face(), centre);

        let normal = found.expect("the floor under the axis is ground");
        assert!(normal.distance(Vec3::Y) < 1e-3, "{normal:?}");
    }

    #[test]
    fn a_slide_that_lands_from_the_air_reports_the_ground() {
        let above_the_floor = Pose::from_translation(Vec3::new(0.0, 1.2, 0.0));

        let slid = with_slider(&[ground(0.0, -10.0, 10.0)], |slider| {
            slider.slide(&above_the_floor, Vec3::new(0.0, -0.1, 0.0), false)
        });

        assert!(slid.grounded, "{slid:?}");
        let ground = slid
            .ground
            .expect("a landing reads the ground it stands on");
        assert!(ground.normal.distance(Vec3::Y) < 1e-3, "{ground:?}");
    }

    #[test]
    fn a_step_up_that_lands_on_a_steep_face_is_refused() {
        // A 0.3 m riser, under the step height, topped by a 50 degree face rising away from
        // the walker: only a step-up could lift the hull, and it would land on the face.
        let rise = 50.0_f32.to_radians().tan();
        let riser_and_face = quad_mesh(
            &[
                [-10.0, 0.0, 1.0],
                [10.0, 0.0, 1.0],
                [10.0, 0.3, 1.0],
                [-10.0, 0.3, 1.0],
                [10.0, 0.3 + rise, 2.0],
                [-10.0, 0.3 + rise, 2.0],
            ],
            &[0, 2, 1, 0, 3, 2, 3, 4, 2, 3, 5, 4],
        );
        let touching_the_riser =
            Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y, 1.0 - 0.3 - 0.0115));

        let slid = with_slider(&[ground(0.0, -10.0, 10.0), riser_and_face], |slider| {
            slider.slide(&touching_the_riser, Vec3::new(0.0, 0.0, 0.1), true)
        });

        assert!(
            slid.translation.y.abs() < 0.02,
            "lifted onto the face: {slid:?}"
        );
    }

    #[test]
    fn a_walker_on_the_floor_beside_a_low_step_is_not_rolled_but_one_held_above_it_is() {
        // The rim's contact with a nosing 0.25 m to the side is outside the flat base. The walker
        // stands on the floor when the nosing is lower than 0.126 m; above that the nosing holds
        // it up, and the floor under its flat base lies more than the 0.0115 m skin below where
        // its drop stops once the nosing is 4 mm higher.
        let on_the_floor = idle_slide_past_a_low_step(0.05, 0.85 + 0.3115);
        let held_above = idle_slide_past_a_low_step(0.1298, 0.85 + 0.1298 + 0.1857);

        assert!(on_the_floor.translation.z.abs() < 1e-4, "{on_the_floor:?}");
        assert!(
            (held_above.translation.z - 1.0 / 60.0).abs() < 5e-4,
            "rolled off the nosing: {held_above:?}"
        );
    }

    const FLOOR: Vec3 = Vec3::Y;

    #[test]
    fn clip_motion_slides_along_one_plane_and_keeps_motion_leaving_it() {
        let motion = Vec3::new(1.0, 0.5, 1.0);

        let along_wall = clip_motion(motion, &[Vec3::NEG_Z]);
        let leaving_wall = clip_motion(motion, &[Vec3::Z]);
        let past_the_floor = clip_motion(motion, &[FLOOR, Vec3::NEG_Z]);

        assert!(
            along_wall.distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-6,
            "{along_wall:?}"
        );
        assert!(leaving_wall.distance(motion) < 1e-6, "{leaving_wall:?}");
        assert!(
            past_the_floor.distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-6,
            "rising off the earlier plane is kept: {past_the_floor:?}"
        );
    }

    #[test]
    fn clip_motion_follows_the_crease_of_two_planes() {
        let motion = Vec3::new(1.0, -1.0, 1.0);

        let floor_then_wall = clip_motion(motion, &[FLOOR, Vec3::NEG_Z]);
        let wall_then_floor = clip_motion(motion, &[Vec3::NEG_Z, FLOOR]);

        assert!(
            floor_then_wall.distance(Vec3::X) < 1e-6,
            "{floor_then_wall:?}"
        );
        assert!(
            wall_then_floor.distance(Vec3::X) < 1e-6,
            "{wall_then_floor:?}"
        );
    }

    #[test]
    fn clip_motion_follows_a_crease_that_is_not_perpendicular_to_the_motion() {
        let motion = Vec3::new(0.0, 1.0, 1.0);

        let clipped = clip_motion(
            motion,
            &[Vec3::new(0.6, 0.0, -0.8), Vec3::new(-0.6, 0.0, -0.8)],
        );

        assert!(clipped.distance(Vec3::Y) < 1e-5, "{clipped:?}");
    }

    #[test]
    fn clip_motion_keeps_a_third_plane_motion_that_leaves_the_others() {
        let motion = Vec3::new(1.0, 0.5, -1.0);

        let clipped = clip_motion(motion, &[FLOOR, Vec3::NEG_Z, Vec3::NEG_X]);

        assert!(
            clipped.distance(Vec3::new(0.0, 0.5, -1.0)) < 1e-5,
            "{clipped:?}"
        );
    }

    #[test]
    fn clip_motion_stops_against_a_third_plane() {
        let motion = Vec3::new(1.0, -1.0, 1.0);

        let clipped = clip_motion(motion, &[FLOOR, Vec3::NEG_Z, Vec3::NEG_X]);

        assert_eq!(clipped, Vec3::ZERO);
    }

    #[test]
    fn a_level_slide_a_skin_above_the_floor_moves_the_whole_way() {
        let resting = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y, 0.0));

        let slid = with_slider(&[ground(0.0, -10.0, 10.0)], |slider| {
            slider.slide(&resting, Vec3::new(0.0, 0.0, 0.1), true)
        });

        assert!((slid.translation.z - 0.1).abs() < 1e-4, "{slid:?}");
        assert!(slid.translation.y.abs() < 1e-3, "{slid:?}");
    }

    #[test]
    fn a_level_slide_into_a_steep_face_does_not_rise() {
        let touching_the_face = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y, 0.82));

        let slid = with_slider(&floor_with_a_steep_face(), |slider| {
            slider.slide(&touching_the_face, Vec3::new(0.0, 0.0, 0.1), false)
        });

        assert!(slid.translation.y <= 1e-3, "rode up the face: {slid:?}");
    }

    #[test]
    fn a_rising_slide_into_a_steep_face_rides_up_it() {
        let touching_the_face = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y, 0.82));

        let slid = with_slider(&floor_with_a_steep_face(), |slider| {
            slider.slide(&touching_the_face, Vec3::new(0.0, 0.05, 0.1), false)
        });

        assert!(slid.translation.y > 0.06, "did not ride up: {slid:?}");
    }

    #[test]
    fn a_hit_on_a_triangle_face_reports_its_exact_normal() {
        // The jump setup of a walker pressed against a 0.7 m riser: rising while moving forward,
        // the cast's own normal on the riser is off vertical by about 2e-4.
        let meshes = [ground(0.0, -10.0, 10.0), step_box_mesh(2.0, 6.0, 0.7)];
        let pressed = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y, 2.0 - 0.3 - 0.03));
        let asked = Vec3::new(0.0, 0.133, 4.0 / 60.0);

        let slid = with_slider(&meshes, |slider| slider.slide(&pressed, asked, true));

        let (_, normal) = slid
            .hits
            .iter()
            .find(|(_, normal)| normal.z < -0.5)
            .unwrap_or_else(|| panic!("the riser was not hit: {slid:?}"));
        assert!(
            normal.distance(Vec3::NEG_Z) < 1e-6,
            "the cast's noisy normal, not the face's: {normal:?}"
        );
    }

    /// A capsule 5 cm into the wall at z = 2, centred in the air so only the wall is touched.
    fn inside_the_wall() -> Pose {
        Pose::from_translation(Vec3::new(0.0, 1.5, 2.0 - 0.3 + 0.05))
    }

    #[test]
    fn a_hull_asked_to_stay_put_inside_a_wall_is_pushed_out_to_a_skin() {
        let slid = with_slider(&[wall_mesh(2.0)], |slider| {
            slider.slide(&inside_the_wall(), Vec3::ZERO, false)
        });

        let ends_at = inside_the_wall().translation.z + slid.translation.z;
        assert!(
            (ends_at - (2.0 - 0.3 - 0.0115)).abs() < 1e-3,
            "should end a skin from the wall: {slid:?}"
        );
    }

    #[test]
    fn a_moving_hull_is_not_depenetrated() {
        let along_the_wall = Vec3::new(0.05, 0.0, 0.0);

        let slid = with_slider(&[wall_mesh(2.0)], |slider| {
            slider.slide(&inside_the_wall(), along_the_wall, false)
        });

        assert!(slid.translation.z.abs() < 1e-3, "pushed out: {slid:?}");
        assert!((slid.translation.x - 0.05).abs() < 1e-3, "{slid:?}");
    }

    #[test]
    fn a_level_slide_ending_within_contact_is_lowered_onto_the_ground() {
        let hovering = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y + 0.03, 0.0));

        for was_grounded in [true, false] {
            let slid = with_slider(&[ground(0.0, -10.0, 10.0)], |slider| {
                slider.slide(&hovering, Vec3::new(0.0, 0.0, 0.01), was_grounded)
            });

            assert!(
                (slid.translation.y + 0.03).abs() < 1e-3,
                "was_grounded {was_grounded}: {slid:?}"
            );
            assert!(slid.grounded, "was_grounded {was_grounded}: {slid:?}");
            let ground = slid.ground.expect("ground under the lowered hull");
            assert!(ground.gap.abs() < 1e-3, "{ground:?}");
        }
    }

    #[test]
    fn a_slide_beyond_ground_contact_is_not_lowered() {
        let hovering = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y + 0.08, 0.0));

        let slid = with_slider(&[ground(0.0, -10.0, 10.0)], |slider| {
            slider.slide(&hovering, Vec3::new(0.0, 0.0, 0.01), true)
        });

        assert!(slid.translation.y.abs() < 1e-3, "lowered: {slid:?}");
        assert!(!slid.grounded, "{slid:?}");
    }

    #[test]
    fn a_rising_slide_is_not_lowered() {
        let hovering = Pose::from_translation(Vec3::new(0.0, FLOOR_REST_Y + 0.03, 0.0));

        let slid = with_slider(&[ground(0.0, -10.0, 10.0)], |slider| {
            slider.slide(&hovering, Vec3::new(0.0, 0.01, 0.01), true)
        });

        assert!((slid.translation.y - 0.01).abs() < 1e-3, "{slid:?}");
    }
}
