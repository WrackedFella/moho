//! The rapier collide-and-slide behind [`crate::movement::step`].

use crate::movement::{Slide, WALKABLE_NORMAL_Y};
use glam::Vec3;
use rapier3d::control::{
    CharacterLength, EffectiveCharacterMovement, KinematicCharacterController,
};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::{ColliderHandle, Pose, QueryPipeline, Shape};

/// How far below a grounded landing to look for the surface it landed on, in metres.
const GROUND_PROBE: f32 = 0.1;

/// Surfaces facing down at least this much (`normal.y` below its negative) are ceilings, not risers.
const CEILING_NORMAL_Y: f32 = 0.3;

/// Extra downward distance when settling onto a step top, in metres.
const STEP_SNAP: f32 = 0.05;

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

        let against_riser = hits
            .iter()
            .any(|(_, normal)| normal.y < WALKABLE_NORMAL_Y && normal.y > -CEILING_NORMAL_Y);
        if was_grounded && against_riser {
            if let Some(translation) = self.step_up(start, asked, plain.translation) {
                return Slide {
                    translation,
                    grounded: true,
                    hits,
                };
            }
        }

        let lands_on_ground = hits.iter().any(|(_, normal)| normal.y >= WALKABLE_NORMAL_Y);
        if plain.grounded && asked.y < 0.0 && !lands_on_ground {
            hits.extend(self.probe_landing(start, plain.translation));
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
    /// when that does not end grounded and further along than the plain slide's `blocked`.
    fn step_up(&self, start: &Pose, asked: Vec3, blocked: Vec3) -> Option<Vec3> {
        let autostep = self.controller.autostep?;
        let (CharacterLength::Absolute(max_height), CharacterLength::Absolute(min_width)) =
            (autostep.max_height, autostep.min_width)
        else {
            return None;
        };
        let remaining = (asked - blocked).with_y(0.0);
        if remaining.length() < STEP_MIN_PROGRESS {
            return None;
        }

        let stopped = Pose::from_parts(start.translation + blocked, start.rotation);
        let lift = self
            .queries
            .cast_shape(&stopped, Vec3::Y, self.shape, cast_within(max_height))
            .map_or(max_height, |(_, hit)| hit.time_of_impact);
        if lift <= 0.0 {
            return None;
        }

        let lifted = Pose::from_parts(stopped.translation + Vec3::Y * lift, stopped.rotation);
        let dir = remaining.normalize();
        let len = remaining.length().max(min_width);
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

        let total = blocked + Vec3::Y * lift + across.translation + down.translation;
        (total.with_y(0.0).length() > blocked.with_y(0.0).length() + STEP_MIN_PROGRESS)
            .then_some(total)
    }

    /// Collide-and-slide from `from` without recording hits.
    fn shift(&self, from: &Pose, by: Vec3) -> EffectiveCharacterMovement {
        self.controller
            .move_shape(self.dt, self.queries, self.shape, from, by, |_| {})
    }
}
