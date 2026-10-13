//! The rapier collide-and-slide behind [`crate::movement::step`].

use crate::movement::{Slide, WALKABLE_NORMAL_Y};
use glam::Vec3;
use rapier3d::control::{CharacterLength, KinematicCharacterController};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::{Pose, QueryPipeline, Shape};

/// How far below a grounded landing to look for the surface it landed on, in metres.
const GROUND_PROBE: f32 = 0.1;

/// Surfaces facing down at least this much (`normal.y` below its negative) are ceilings, not risers.
const CEILING_NORMAL_Y: f32 = 0.3;

/// Extra downward distance when settling onto a step top, in metres.
const STEP_SNAP: f32 = 0.05;

/// Least horizontal travel across a step top, in metres.
///
/// The capsule stops a radius short of the riser, so anything less leaves its rounded base
/// balanced on the step's edge, where the next slide would read it as a slope to climb.
const STEP_REACH: f32 = 0.4;

/// Horizontal motion left over, or gained, below this many metres does not count.
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

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Move the capsule from `start` by `asked` and report what it touched.
///
/// A grounded character blocked by a riser retries from on top of it, and a landing the
/// controller detected without a hit gets the surface it landed on.
pub(super) fn slide(
    controller: &KinematicCharacterController,
    queries: &QueryPipeline,
    shape: &dyn Shape,
    start: &Pose,
    asked: Vec3,
    was_grounded: bool,
    dt: f32,
) -> Slide {
    let mut hits = Vec::new();
    let plain = controller.move_shape(dt, queries, shape, start, asked, |collision| {
        hits.push((collision.handle, collision.hit.normal1));
    });

    let against_riser = hits
        .iter()
        .any(|(_, normal)| normal.y < WALKABLE_NORMAL_Y && normal.y > -CEILING_NORMAL_Y);
    if was_grounded && against_riser {
        if let Some(translation) = step_up(
            controller,
            queries,
            shape,
            start,
            asked,
            plain.translation,
            dt,
        ) {
            return Slide {
                translation,
                grounded: true,
                hits,
            };
        }
    }

    if plain.grounded && asked.y < 0.0 && hits.is_empty() {
        hits.extend(probe_landing(queries, shape, start, plain.translation));
    }
    Slide {
        translation: plain.translation,
        grounded: plain.grounded,
        hits,
    }
}

/// The surface under a character the controller grounded without reporting a hit.
///
/// The controller detects ground a few centimetres early, so a landing there would carry
/// no impact.
fn probe_landing(
    queries: &QueryPipeline,
    shape: &dyn Shape,
    start: &Pose,
    translation: Vec3,
) -> Option<(rapier3d::prelude::ColliderHandle, Vec3)> {
    let landed_at = Pose::from_parts(start.translation + translation, start.rotation);
    let (handle, hit) =
        queries.cast_shape(&landed_at, -Vec3::Y, shape, cast_within(GROUND_PROBE))?;
    Some((handle, hit.normal1))
}

/// Retry a blocked slide from on top of the obstacle (Source's `StepMove`).
///
/// The controller's own autostep fails on float noise when the capsule stops exactly its
/// skin offset from a riser, so this lifts the capsule, carries the remaining horizontal
/// motion across, and settles it down. Returns the whole tick's translation, or `None`
/// when that does not end grounded and further along than the plain slide's `blocked`.
fn step_up(
    controller: &KinematicCharacterController,
    queries: &QueryPipeline,
    shape: &dyn Shape,
    start: &Pose,
    asked: Vec3,
    blocked: Vec3,
    dt: f32,
) -> Option<Vec3> {
    let CharacterLength::Absolute(max_height) = controller.autostep?.max_height else {
        return None;
    };
    let remaining = flat(asked - blocked);
    if remaining.length() < STEP_MIN_PROGRESS {
        return None;
    }

    let stopped = Pose::from_parts(start.translation + blocked, start.rotation);
    let lift = queries
        .cast_shape(&stopped, Vec3::Y, shape, cast_within(max_height))
        .map_or(max_height, |(_, hit)| hit.time_of_impact);
    if lift <= 0.0 {
        return None;
    }

    let lifted = Pose::from_parts(stopped.translation + Vec3::Y * lift, stopped.rotation);
    let reach = remaining.normalize() * remaining.length().max(STEP_REACH);
    // Like rapier's own stair check: the lifted capsule must clear the whole reach, so a
    // riser taller than the lift is not climbed by riding up its rounded edge.
    if queries
        .cast_shape(
            &lifted,
            reach.normalize(),
            shape,
            cast_within(reach.length()),
        )
        .is_some()
    {
        return None;
    }
    let across = controller.move_shape(dt, queries, shape, &lifted, reach, |_| {});
    let over = Pose::from_parts(lifted.translation + across.translation, lifted.rotation);
    let down = controller.move_shape(
        dt,
        queries,
        shape,
        &over,
        -Vec3::Y * (lift + STEP_SNAP),
        |_| {},
    );
    if !down.grounded {
        return None;
    }

    let total = blocked + Vec3::Y * lift + across.translation + down.translation;
    (flat(total).length() > flat(blocked).length() + STEP_MIN_PROGRESS).then_some(total)
}
