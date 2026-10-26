//! Per-tick character movement rules (Quake/Source style).
//!
//! [`step`] is a pure function of the character's state, the tick's command, the
//! game's settings and one collide-and-slide query, so it can be re-run for prediction.

use crate::world::CharacterState;
use glam::Vec3;
use rapier3d::geometry::ColliderHandle;
use serde::{Deserialize, Serialize};

/// Largest pitch magnitude in radians, short of straight up or down.
pub const PITCH_LIMIT: f32 = 1.54;

/// A change to a character from outside the step, applied in queue order at the start of its
/// next step.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CharacterEffect {
    AddVelocity(Vec3),
    SetVelocity(Vec3),
    /// Set the position; zero the velocity unless `keep_velocity`; clear `grounded`.
    Teleport {
        position: Vec3,
        keep_velocity: bool,
    },
    /// Replace the look, with pitch clamped to [`PITCH_LIMIT`].
    SetLook {
        yaw: f32,
        pitch: f32,
    },
}

/// One tick's movement intent for a character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterCommand {
    /// Move axis along the facing, -1 (back) to 1 (forward); yaw 0 faces +Z. Combined with
    /// `right`, input longer than 1 is scaled down to 1.
    pub forward: f32,
    /// Move axis across the facing, -1 (left) to 1 (right).
    pub right: f32,
    /// Turn this tick, in radians; positive yaw turns from +Z toward +X.
    pub yaw_delta: f32,
    /// Look change this tick, in radians; positive looks up.
    pub pitch_delta: f32,
    /// Jump was down at some point during the tick. A grounded character jumps on a press, and
    /// on a held jump only with [`MoveSettings::autohop`].
    pub jump: bool,
    /// The stance the game wants; hold or toggle is the game's business.
    pub stance: Stance,
    /// The lean the game wants, -1 (left) to 1 (right); clamped, with infinities reaching the
    /// ends and NaN meaning 0. Which key wins is the game's call.
    pub lean: f32,
}

/// What a character's hull and eye are doing: standing, crouched or prone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

/// Which end of the hull a stance leg keeps still: the feet of a grounded character, the top
/// of an airborne one (Source's duck-jump).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    Feet,
    Head,
}

/// Game-supplied movement tuning; may change between ticks. Units: m/s, m/s², 1/s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveSettings {
    /// Ground speed at full input, m/s.
    pub max_speed: f32,
    /// Most speed ground input adds along the wish direction per second, m/s².
    pub ground_accel: f32,
    /// Ground speed lost per second as a fraction of speed, 1/s.
    pub friction: f32,
    /// Speed below which friction acts as if moving this fast, m/s, so slow characters stop.
    pub stop_speed: f32,
    /// Most speed air input adds per second, m/s².
    pub air_accel: f32,
    /// Speed along the wish direction air input can build up to, m/s.
    pub air_speed_cap: f32,
    /// Downward acceleration while airborne, m/s²; `None` reads the world's gravity.
    pub gravity: Option<f32>,
    /// Upward speed a jump sets, m/s.
    pub jump_speed: f32,
    /// Largest speed along any one axis, m/s.
    pub max_velocity: f32,
    /// Held jump jumps again on landing.
    pub autohop: bool,
    /// Ticks one stand-or-crouch leg takes; 0 changes stance within one tick.
    pub crouch_ticks: u16,
    /// Ticks one crouch-and-prone leg takes; 0 changes stance within one tick.
    pub prone_ticks: u16,
    /// Ticks a lean takes from upright to full; 0 leans fully within one tick.
    pub lean_ticks: u16,
}

impl MoveSettings {
    /// Ticks the leg between `a` and `b` takes: a prone leg or a crouch leg.
    fn leg_ticks(&self, a: Stance, b: Stance) -> u16 {
        if a == Stance::Prone || b == Stance::Prone {
            self.prone_ticks
        } else {
            self.crouch_ticks
        }
    }
}

/// The shape of a character's hull.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum HullShape {
    Capsule,
    /// A cylinder whose edges are rounded to `rounding`, with `0 < rounding <= radius`.
    RoundCylinder {
        rounding: f32,
    },
}

/// A character's collision hull, upright.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hull {
    pub shape: HullShape,
    pub radius: f32,
    /// End to end along the hull's axis, which is the height when upright.
    pub length: f32,
}

/// A hull and where the eye sits in it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stature {
    pub hull: Hull,
    /// The eye's height above the feet.
    pub eye_height: f32,
}

/// The shape data a game supplies when it adds a character.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CharacterBody {
    pub standing: Stature,
    /// The stature of a crouched character; `None` never crouches.
    pub crouched: Option<Stature>,
    /// The stature of a prone character; `None` never goes prone. Its hull is a capsule whose
    /// `length` lies along the facing, and a body that goes prone must also crouch.
    pub prone: Option<Stature>,
    /// How far the eye leans; the default is no lean.
    pub lean: Lean,
}

/// How far the eye shifts and the view rolls at a full lean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LeanReach {
    /// Sideways shift of the eye, metres.
    pub distance: f32,
    /// Roll of the view, radians.
    pub roll: f32,
}

/// A character's lean: its reach upright and prone, and the ball swept to keep the eye out of
/// walls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lean {
    /// Standing and crouched.
    pub upright: LeanReach,
    pub prone: LeanReach,
    /// Radius of the ball swept from the upright eye to the leaning one, metres.
    pub head_radius: f32,
}

impl Lean {
    /// The reach in `stance`: prone, or upright for standing and crouched.
    fn reach(&self, stance: Stance) -> LeanReach {
        if stance == Stance::Prone {
            self.prone
        } else {
            self.upright
        }
    }
}

/// A character's hull as posed: upright, or lying along the facing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HullPose {
    Upright(Hull),
    /// A capsule `length` long end to end, along the facing.
    Lying {
        radius: f32,
        length: f32,
    },
}

impl HullPose {
    /// End to end along the hull's axis.
    pub fn length(self) -> f32 {
        match self {
            Self::Upright(hull) => hull.length,
            Self::Lying { length, .. } => length,
        }
    }

    /// Whether the hull lies along the facing instead of standing.
    pub fn is_lying(self) -> bool {
        matches!(self, Self::Lying { .. })
    }

    /// How far the hull's centre rides above the feet.
    fn centre_height(self) -> f32 {
        match self {
            Self::Upright(hull) => hull.length / 2.0,
            Self::Lying { radius, .. } => radius,
        }
    }

    /// How far ahead of the centre the eye sits: the head end of a lying hull.
    fn eye_ahead(self) -> f32 {
        match self {
            Self::Upright(_) => 0.0,
            Self::Lying { radius, length } => length / 2.0 - radius,
        }
    }
}

impl CharacterBody {
    /// The stature of `stance`; a body without a prone stature crouches in it, and one without
    /// a crouched stature stands in both.
    pub(crate) fn stature(&self, stance: Stance) -> &Stature {
        match (stance, &self.prone, &self.crouched) {
            (Stance::Prone, Some(prone), _) => prone,
            (Stance::Prone | Stance::Crouch, _, Some(crouched)) => crouched,
            _ => &self.standing,
        }
    }

    /// The hull of `stance` and whether it lies along the facing.
    pub(crate) fn pose(&self, stance: Stance) -> HullPose {
        let hull = self.stature(stance).hull;
        if stance == Stance::Prone && self.prone.is_some() {
            HullPose::Lying {
                radius: hull.radius,
                length: hull.length,
            }
        } else {
            HullPose::Upright(hull)
        }
    }

    /// The stance this body can take that is nearest to `wanted`.
    fn reachable(&self, wanted: Stance) -> Stance {
        match wanted {
            Stance::Prone if self.prone.is_some() => Stance::Prone,
            Stance::Prone | Stance::Crouch if self.crouched.is_some() => Stance::Crouch,
            _ => Stance::Stand,
        }
    }
}

impl Stature {
    /// The eye's offset from the hull's centre when upright.
    pub fn eye_offset(&self) -> Vec3 {
        Vec3::Y * (self.eye_height - self.hull.length / 2.0)
    }
}

impl Default for CharacterBody {
    fn default() -> Self {
        Self {
            standing: Stature {
                hull: Hull {
                    shape: HullShape::Capsule,
                    radius: 0.3,
                    length: 2.3,
                },
                eye_height: 1.15,
            },
            crouched: None,
            prone: None,
            lean: Lean::default(),
        }
    }
}

impl CharacterBody {
    /// Panics on an impossible hull in any stature: non-positive sizes, a capsule no longer
    /// than its diameter, or `rounding` outside `0 < rounding <= radius`. A round cylinder must
    /// also be longer than twice its rounding. A prone hull must be a capsule, and a body that
    /// goes prone must also crouch, because prone legs pass through crouch.
    ///
    /// Panics rather than returning an error because a bad hull is a programmer error in
    /// the game's data.
    pub fn validate(&self) {
        validate_lean(&self.lean);
        validate_hull(&self.standing.hull);
        if let Some(crouched) = &self.crouched {
            validate_hull(&crouched.hull);
        }
        if let Some(prone) = &self.prone {
            assert!(
                self.crouched.is_some(),
                "impossible body: a prone stature needs a crouched one, as prone legs pass through crouch"
            );
            assert!(
                prone.hull.shape == HullShape::Capsule,
                "impossible hull: the prone hull must be a capsule"
            );
            validate_hull(&prone.hull);
        }
    }
}

fn validate_lean(lean: &Lean) {
    for reach in [lean.upright, lean.prone] {
        assert!(
            reach.distance.is_finite() && reach.distance >= 0.0,
            "impossible lean: distance {} must not be negative",
            reach.distance
        );
        assert!(
            reach.roll.is_finite(),
            "impossible lean: roll {} must be finite",
            reach.roll
        );
    }
    let head_radius = lean.head_radius;
    assert!(
        head_radius.is_finite() && head_radius >= 0.0,
        "impossible lean: head radius {head_radius} must not be negative"
    );
    assert!(
        head_radius > 0.0 || (lean.upright.distance == 0.0 && lean.prone.distance == 0.0),
        "impossible lean: a lean with a reach needs a positive head radius"
    );
}

fn validate_hull(hull: &Hull) {
    let Hull {
        shape,
        radius,
        length,
    } = *hull;
    assert!(
        radius.is_finite() && radius > 0.0,
        "impossible hull: radius {radius} must be positive"
    );
    assert!(
        length.is_finite() && length > 0.0,
        "impossible hull: length {length} must be positive"
    );
    let cap = match shape {
        HullShape::Capsule => radius,
        HullShape::RoundCylinder { rounding } => {
            assert!(
                rounding.is_finite() && rounding > 0.0 && rounding <= radius,
                "impossible hull: rounding {rounding} must be in (0, radius {radius}]"
            );
            rounding
        }
    };
    assert!(
        length > 2.0 * cap,
        "impossible hull: length {length} must exceed twice its end radius {cap}"
    );
}

/// The game data one [`step`] reads besides the command: the character's shape, its movement
/// settings and the gravity already resolved from them and the world's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    pub body: CharacterBody,
    pub settings: MoveSettings,
    /// Downward acceleration in m/s²; `settings.gravity` is not read by the step.
    pub gravity: f32,
}

/// One surface the character lost speed into this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
    /// The collider that was hit.
    pub collider: ColliderHandle,
    /// The surface normal, pointing toward the character.
    pub normal: Vec3,
    /// The speed lost into the surface, m/s.
    pub speed: f32,
}

/// What one collide-and-slide did (the narrow collision query the step sees).
#[derive(Clone, Debug, PartialEq)]
pub struct Slide {
    pub translation: Vec3,
    /// Walkable ground lies within contact of where the slide ends. A walker stands only when
    /// [`Self::ground`] is also `Some`.
    pub grounded: bool,
    /// The walkable surface under the capsule after the slide, within [`STEP_HEIGHT`]; `None`
    /// when there is none.
    pub ground: Option<Ground>,
    /// Each surface touched, with its normal.
    pub hits: Vec<(ColliderHandle, Vec3)>,
}

/// The walkable surface under a character's capsule.
///
/// `normal`, `collider` and `point` describe the face under the capsule's base; `gap` can stop
/// on a different surface, such as the nosing a walker rolls off before reaching the step below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// The face normal.
    pub normal: Vec3,
    /// How far the capsule can drop before it touches anything, in metres, cast straight down.
    pub gap: f32,
    /// The collider the face belongs to.
    pub collider: ColliderHandle,
    /// The contact point on the face, in world space.
    pub point: Vec3,
}

/// Tolerance, in metres, on "the slide moved less into a surface than asked".
///
/// Absorbs float noise in the slide's translation so a grazed surface is not blocking.
const BLOCKED_EPSILON: f32 = 1e-4;

/// Smallest `normal.y` of a surface a character can walk on (cos 45 degrees).
///
/// A face steeper than this is not ground: the collide-and-slide neither climbs it nor
/// grounds a character on it.
pub(crate) const WALKABLE_NORMAL_Y: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Furthest, in metres, a walker is pulled down onto ground, and the largest [`Ground::gap`]
/// at which a slide ending ungrounded still counts as standing.
///
/// A slide that rises is not lowered onto ground by the collide-and-slide, so a walker
/// carrying a ramp's upward speed over a crest is lowered by [`settle`] within this reach
/// instead of hopping.
pub(crate) const GROUND_SNAP: f32 = 0.1;

/// Tallest step, in metres, a walker climbs and the furthest a level or downhill walker is
/// lowered onto ground.
pub(crate) const STEP_HEIGHT: f32 = 0.5;

/// Vertical speed below which a character counts as resting rather than rising.
///
/// Clipping against a wall normal with a 1e-5 vertical tilt would otherwise lift a runner
/// off the ground for a tick.
const REST_SPEED: f32 = 1e-3;

/// Add up to `accel * dt` of speed along `wish`, never pushing the speed along it past `limit`.
fn accelerate(velocity: &mut Vec3, wish: Vec3, limit: f32, accel: f32, dt: f32) {
    let gap = limit - velocity.dot(wish);
    *velocity += wish * gap.min(accel * dt).max(0.0);
}

/// Friction on the horizontal speed, then acceleration along the wish direction.
fn ground_move(velocity: &mut Vec3, wish: Vec3, magnitude: f32, settings: &MoveSettings, dt: f32) {
    let speed = velocity.with_y(0.0).length();
    if speed > 0.0 {
        let drop = speed.max(settings.stop_speed) * settings.friction * dt;
        let scale = (speed - drop).max(0.0) / speed;
        velocity.x *= scale;
        velocity.z *= scale;
    }
    accelerate(
        velocity,
        wish,
        settings.max_speed * magnitude,
        settings.ground_accel,
        dt,
    );
}

/// Most distinct blocking planes [`clip`] resolves; more than this stops the character.
const MAX_CLIP_PLANES: usize = 5;

/// Normals with a dot product above this count as one plane.
const SAME_PLANE_DOT: f32 = 0.99;

/// Replace non-finite components of `velocity` with zero (logged) and clamp each component to
/// `±max`.
fn checked(velocity: Vec3, max: f32) -> Vec3 {
    if !velocity.is_finite() {
        tracing::warn!(?velocity, "non-finite velocity component reset to zero");
    }
    Vec3::select(velocity.is_finite_mask(), velocity, Vec3::ZERO)
        .clamp(Vec3::splat(-max), Vec3::splat(max))
}

/// Remove the velocity into the surfaces that blocked the slide, reporting each as an impact.
///
/// A hit blocks when the asked motion faces it and the slide moved less into it than asked,
/// or when it is the surface an airborne character just landed on. A walker (`walking`) is
/// never blocked by walkable ground it ends the slide standing on: slopes and the rounded
/// edges of steps neither stop nor clip it. Every other surface is judged on the horizontal
/// plane for a walker, as in Source's `WalkMove`, so a lift over a step never reads as
/// blocking and clipping never gives a walker vertical velocity.
///
/// The blocking planes are merged where their normals nearly coincide (the first collider is
/// kept) and the result is chosen as in Quake's `SV_FlyMove`, from the incoming velocity
/// alone so the plane order does not matter:
/// - more than [`MAX_CLIP_PLANES`] planes stop the character;
/// - a velocity clipped against one plane (overbounce 1.0) that enters no other plane is taken;
/// - otherwise two planes slide the character along their crease, and any other count stops it.
///
/// One impact is reported per plane, its speed the incoming velocity's speed into it.
fn clip(
    velocity: &mut Vec3,
    asked: Vec3,
    slid: &Slide,
    walking: bool,
    airborne_before: bool,
) -> Vec<Impact> {
    let primal = *velocity;
    let mut impacts = Vec::new();
    let mut planes = [Vec3::ZERO; MAX_CLIP_PLANES];
    let mut count = 0;
    for &(collider, surface) in &slid.hits {
        if walking && slid.grounded && surface.y >= WALKABLE_NORMAL_Y {
            continue;
        }
        let (normal, asked, moved) = if walking {
            (
                surface.with_y(0.0).normalize_or_zero(),
                asked.with_y(0.0),
                slid.translation.with_y(0.0),
            )
        } else {
            (surface, asked, slid.translation)
        };
        let lands_on = airborne_before && slid.grounded && surface.y > 0.0;
        let blocked = asked.dot(normal) < 0.0
            && (lands_on || moved.dot(normal) > asked.dot(normal) + BLOCKED_EPSILON);
        let into = primal.dot(normal);
        if !blocked || into >= 0.0 {
            continue;
        }
        if planes[..count]
            .iter()
            .any(|plane| plane.dot(normal) > SAME_PLANE_DOT)
        {
            continue;
        }
        if count == MAX_CLIP_PLANES {
            *velocity = Vec3::ZERO;
            return impacts;
        }
        planes[count] = normal;
        count += 1;
        impacts.push(Impact {
            collider,
            normal: surface,
            speed: -into,
        });
    }

    if count > 0 {
        *velocity = slide_along(primal, &planes[..count]);
    }
    impacts
}

/// The velocity `primal` becomes against `planes`: see [`clip`].
fn slide_along(primal: Vec3, planes: &[Vec3]) -> Vec3 {
    for (i, &normal) in planes.iter().enumerate() {
        let clipped = primal.reject_from_normalized(normal);
        let enters_another = planes
            .iter()
            .enumerate()
            .any(|(j, plane)| j != i && clipped.dot(*plane) < 0.0);
        if !enters_another {
            return clipped;
        }
    }
    match planes {
        [a, b] => {
            let crease = a.cross(*b).normalize_or_zero();
            primal.project_onto_normalized(crease)
        }
        _ => Vec3::ZERO,
    }
}

/// `look` (yaw, pitch) turned by `delta` with pitch clamped at [`PITCH_LIMIT`]; the one
/// place the turn is computed, so a drawn view and a stepped state agree bit for bit.
pub(crate) fn turned_look(look: (f32, f32), delta: (f32, f32)) -> (f32, f32) {
    (
        look.0 + delta.0,
        (look.1 + delta.1).clamp(-PITCH_LIMIT, PITCH_LIMIT),
    )
}

fn apply(state: &mut CharacterState, effect: CharacterEffect) {
    match effect {
        CharacterEffect::AddVelocity(velocity) => state.velocity += velocity,
        CharacterEffect::SetVelocity(velocity) => state.velocity = velocity,
        CharacterEffect::Teleport {
            position,
            keep_velocity,
        } => {
            state.position = position;
            if !keep_velocity {
                state.velocity = Vec3::ZERO;
            }
            state.grounded = false;
        }
        CharacterEffect::SetLook { yaw, pitch } => {
            state.yaw = yaw;
            state.pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
    }
}

/// The world as one character's step sees it.
pub trait CharacterQueries {
    /// Collide-and-slide `pose`, centred at `from.position`, by `asked`.
    fn slide(&self, from: &CharacterState, pose: HullPose, asked: Vec3) -> Slide;

    /// Whether `pose`, centred at `centre` and facing `yaw`, overlaps nothing solid.
    fn fits(&self, pose: HullPose, centre: Vec3, yaw: f32) -> bool;

    /// The free fraction (0 to 1) of the straight path from `from` to `to` for a ball of
    /// `radius`, less a skin, ignoring the character's own hull and sensors.
    fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> f32;
}

/// Start, reverse or advance the stance leg toward `wanted`.
///
/// Each leg moves one step along stand, crouch, prone, so a wanted stance two steps away takes
/// the next leg toward it once the one running has settled. A leg starts, or a running one
/// reverses, only where the target hull fits at the anchored centre. A fresh leg anchors by
/// `grounded`: a grounded character keeps the bottom of its hull, an airborne one its top. A
/// reversal keeps the running leg's anchor, so the eye stays continuous. A leg into or out of
/// prone starts or reverses only on the ground, and keeps the feet; once running it runs on in
/// the air. A refused leg leaves the running one untouched, so it retries every tick. A body
/// with no crouched stature always stands. `progress` then advances, clamped at the leg's
/// duration; a leg that reaches it is settled, and `from` becomes `stance`.
fn change_stance(
    state: &mut CharacterState,
    wanted: Stance,
    body: &CharacterBody,
    settings: &MoveSettings,
    queries: &impl CharacterQueries,
) {
    let wanted = body.reachable(wanted);
    if wanted != state.stance {
        let target = if state.stance == Stance::Crouch {
            wanted
        } else {
            Stance::Crouch
        };
        let running = settings.leg_ticks(state.from, state.stance);
        // `progress >= running` also covers a leg time shortened since the leg began.
        let settled = state.from == state.stance || state.progress >= running;
        let prone_leg = target == Stance::Prone || state.stance == Stance::Prone;
        if (settled || target == state.from) && (state.grounded || !prone_leg) {
            let duration = settings.leg_ticks(state.stance, target);
            let anchor = if !settled {
                state.anchor
            } else if state.grounded {
                Anchor::Feet
            } else {
                Anchor::Head
            };
            let shift = body.pose(target).centre_height() - body.pose(state.stance).centre_height();
            let anchored = match anchor {
                Anchor::Feet => shift,
                Anchor::Head => -shift,
            };
            let centre = state.position + Vec3::Y * anchored;
            if queries.fits(body.pose(target), centre, state.yaw) {
                state.progress = if settled {
                    0
                } else {
                    duration - state.progress
                };
                state.from = state.stance;
                state.anchor = anchor;
                state.stance = target;
                state.position = centre;
            }
        }
    }
    let duration = settings.leg_ticks(state.from, state.stance);
    state.progress = state.progress.saturating_add(1).min(duration);
    if state.progress >= duration {
        state.from = state.stance;
    }
}

/// Turn a lying hull's yaw from `old` toward `wanted` as far as it fits at `centre`.
///
/// Four bisections between the two, keeping the furthest yaw that fits, so at most five `fits`
/// probes and the same answer every time.
fn clamp_lying_turn(
    pose: HullPose,
    centre: Vec3,
    old: f32,
    wanted: f32,
    queries: &impl CharacterQueries,
) -> f32 {
    if queries.fits(pose, centre, wanted) {
        return wanted;
    }
    let (mut fits, mut blocked) = (old, wanted);
    for _ in 0..4 {
        let middle = (fits + blocked) / 2.0;
        if queries.fits(pose, centre, middle) {
            fits = middle;
        } else {
            blocked = middle;
        }
    }
    fits
}

/// How far along the stance leg the character is, 0 to 1; a leg of no ticks is done.
fn leg_done(state: &CharacterState, settings: &MoveSettings) -> f32 {
    let duration = settings.leg_ticks(state.from, state.stance);
    if duration == 0 {
        1.0
    } else {
        f32::from(state.progress) / f32::from(duration)
    }
}

/// Move `state.lean` toward the wanted lean by one tick's share of the full lean.
fn advance_lean(state: &mut CharacterState, wanted: f32, settings: &MoveSettings) {
    let wanted = if wanted.is_nan() { 0.0 } else { wanted }.clamp(-1.0, 1.0);
    if settings.lean_ticks == 0 {
        state.lean = wanted;
        return;
    }
    let step = 1.0 / f32::from(settings.lean_ticks);
    state.lean += (wanted - state.lean).clamp(-step, step);
}

/// Shift the eye sideways by the lean, stopped short of any wall, and roll the view with it.
///
/// A zero lean or reach leaves the eye unshifted and the roll at 0, with no sweep.
fn lean_eye(
    state: &mut CharacterState,
    body: &CharacterBody,
    settings: &MoveSettings,
    queries: &impl CharacterQueries,
) {
    let (from, to) = (body.lean.reach(state.from), body.lean.reach(state.stance));
    let done = leg_done(state, settings);
    let distance = from.distance + (to.distance - from.distance) * done;
    let roll = from.roll + (to.roll - from.roll) * done;

    let (sin, cos) = state.yaw.sin_cos();
    let offset = Vec3::new(-cos, 0.0, sin) * (state.lean * distance);
    if offset == Vec3::ZERO {
        state.roll = 0.0;
        return;
    }
    let upright = state.position + state.eye;
    let free = queries
        .sweep(upright, upright + offset, body.lean.head_radius)
        .clamp(0.0, 1.0);
    state.eye += offset * free;
    state.roll = roll * state.lean * free;
}

/// The eye's offset from the centre of the hull in use, part-way through the stance leg.
///
/// The eye moves in the frame of the leg's anchor, so the anchored end of the hull and the
/// eye stay together: its height above the feet for a feet-anchored leg, its depth below the
/// top of the hull for a head-anchored one. A prone eye also slides toward the head end of the
/// lying hull, along the facing.
fn eye_offset(state: &CharacterState, body: &CharacterBody, settings: &MoveSettings) -> Vec3 {
    let done = leg_done(state, settings);
    let from = body.stature(state.from);
    let to = body.stature(state.stance);
    let y = match state.anchor {
        Anchor::Feet => {
            from.eye_height + (to.eye_height - from.eye_height) * done
                - body.pose(state.stance).centre_height()
        }
        Anchor::Head => {
            let depth = |stature: &Stature| stature.hull.length - stature.eye_height;
            to.hull.length / 2.0 - (depth(from) + (depth(to) - depth(from)) * done)
        }
    };
    let (ahead_from, ahead_to) = (
        body.pose(state.from).eye_ahead(),
        body.pose(state.stance).eye_ahead(),
    );
    let ahead = ahead_from + (ahead_to - ahead_from) * done;
    let (sin, cos) = state.yaw.sin_cos();
    Vec3::new(sin * ahead, y, cos * ahead)
}

/// Advance `state` by one tick of `dt` seconds and report the impacts.
///
/// `queries.slide` receives the displacement the tick asks for and reports what the world
/// allowed, along with the state it starts from, which a teleport may have moved. The queued
/// `effects` apply first, in order, so a replay of commands and effects reproduces the run.
/// The look turns next, so a turn and a move in one command face the new way.
///
/// The order within a tick:
/// 0. the queued effects apply in order; a teleport clears `grounded`, so that tick runs
///    airborne and settle finds the ground; the velocity is then checked as in 4b, so friction
///    never meets an infinite speed;
/// 1. the look turns; a lying hull turns only as far as it fits (see `clamp_lying_turn`);
/// 2. the stance leg starts, reverses or advances (see `change_stance`), so the slide of this
///    tick sees the new hull;
///    2b. a jump adds `jump_speed` to the vertical velocity: a grounded character jumps when
///    jump is down and was up last tick, or held with `autohop`, never while prone
///    or rising out of prone;
/// 3. half a tick of gravity (`v.y -= g·dt/2`), unless walking;
/// 4. friction and acceleration (walking) or air acceleration;
///    4b. the velocity is checked: a non-finite component becomes 0 and each component is
///    clamped to `±max_velocity`, so the slide runs at most at that speed;
/// 5. `slide` receives `asked = v·dt`;
/// 6. the velocity is clipped against what the slide hit (see `clip`);
/// 7. the other half of gravity, unless walking;
/// 8. settle: a walker that ends the tick grounded moves along its ground, keeping its
///    horizontal velocity; a landing zeroes the vertical velocity;
/// 9. the velocity is checked again, so stored state is bounded, `jump_held` records the
///    command's `jump`, whether or not the character jumped, and the eye follows the stance leg.
///
/// The two halves around the move give an airborne tick the exact displacement under constant
/// gravity at any tick rate. A landing's impact speed is therefore the mid-tick downward speed.
///
/// `gravity` is the downward acceleration in m/s², already resolved by the caller from
/// [`MoveSettings::gravity`] and the world's; `settings.gravity` is not read here.
pub fn step(
    state: &mut CharacterState,
    command: &CharacterCommand,
    effects: &[CharacterEffect],
    tuning: &Tuning,
    dt: f32,
    queries: &impl CharacterQueries,
) -> Vec<Impact> {
    let Tuning {
        body,
        ref settings,
        gravity,
    } = *tuning;
    for effect in effects {
        apply(state, *effect);
    }
    state.velocity = checked(state.velocity, settings.max_velocity);

    let old_yaw = state.yaw;
    (state.yaw, state.pitch) = turned_look(
        (state.yaw, state.pitch),
        (command.yaw_delta, command.pitch_delta),
    );
    let pose = body.pose(state.stance);
    if pose.is_lying() && state.yaw != old_yaw {
        state.yaw = clamp_lying_turn(pose, state.position, old_yaw, state.yaw, queries);
    }

    change_stance(state, command.stance, &body, settings, queries);

    let (sin, cos) = state.yaw.sin_cos();
    let wish_vector =
        Vec3::new(sin, 0.0, cos) * command.forward + Vec3::new(-cos, 0.0, sin) * command.right;
    let wish = wish_vector.normalize_or_zero();
    let magnitude = wish_vector.length().min(1.0);

    let prone_or_leaving_prone = state.stance == Stance::Prone || state.from == Stance::Prone;
    let jumped = state.grounded
        && command.jump
        && !prone_or_leaving_prone
        && (!state.jump_held || settings.autohop);
    if jumped {
        state.velocity.y += settings.jump_speed;
    }

    let airborne_before = !state.grounded;
    // A walker may climb a slope, so its upward speed only counts as leaving the ground once
    // it exceeds what its horizontal speed explains.
    let walking = state.grounded
        && !jumped
        && state.velocity.y <= state.velocity.with_y(0.0).length() + REST_SPEED;
    let half_gravity = gravity * dt / 2.0;
    if walking {
        ground_move(&mut state.velocity, wish, magnitude, settings, dt);
    } else {
        state.velocity.y -= half_gravity;
        accelerate(
            &mut state.velocity,
            wish,
            settings.air_speed_cap * magnitude,
            settings.air_accel,
            dt,
        );
    }

    state.velocity = checked(state.velocity, settings.max_velocity);

    let asked = state.velocity * dt;
    let slid = queries.slide(state, body.pose(state.stance), asked);
    state.position += slid.translation;

    let impacts = clip(&mut state.velocity, asked, &slid, walking, airborne_before);
    if !walking {
        state.velocity.y -= half_gravity;
    }

    settle(state, &slid, walking, asked, dt);
    state.velocity = checked(state.velocity, settings.max_velocity);
    state.jump_held = command.jump;
    state.eye = eye_offset(state, &body, settings);
    advance_lean(state, command.lean, settings);
    lean_eye(state, &body, settings, queries);
    impacts
}

/// Vertical speed that keeps `velocity` tangent to a surface of `normal`, its horizontal part
/// unchanged.
fn along_ground(velocity: Vec3, normal: Vec3) -> f32 {
    -(normal.x * velocity.x + normal.z * velocity.z) / normal.y
}

/// Settle `state` onto the ground after the slide and clip.
///
/// A walker hanging above walkable ground is lowered by [`Ground::gap`] when that is within
/// reach, as in Source's `StayOnGround`: [`STEP_HEIGHT`] for a level or downward `asked`
/// motion, [`GROUND_SNAP`] for a rising one, so a ramp's crest still launches. Motion counts
/// as rising only above [`REST_SPEED`] over `dt`, so the float noise of a ground normal on
/// level ground does not shorten the reach. A walker that ends grounded keeps its horizontal
/// velocity and follows its ground; any other character ends grounded only if not rising, and
/// a landing zeroes its vertical speed.
fn settle(state: &mut CharacterState, slid: &Slide, walking: bool, asked: Vec3, dt: f32) {
    let reach = if asked.y <= REST_SPEED * dt {
        STEP_HEIGHT
    } else {
        GROUND_SNAP
    };
    let hanging = slid
        .ground
        .filter(|ground| walking && !slid.grounded && ground.gap <= reach);
    if let Some(ground) = hanging {
        state.position.y -= ground.gap.max(0.0);
    }
    let standing = slid.grounded && slid.ground.is_some();
    state.grounded = (standing || hanging.is_some()) && (walking || state.velocity.y <= REST_SPEED);
    if !state.grounded {
        return;
    }
    if !walking {
        state.velocity.y = 0.0;
    } else if let Some(ground) = slid.ground {
        state.velocity.y = along_ground(state.velocity, ground.normal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::test_support::test_settings;
    use proptest::prelude::*;

    const DT: f32 = 1.0 / 60.0;

    fn clear_sweep(_: Vec3, _: Vec3, _: f32) -> f32 {
        1.0
    }

    /// [`CharacterQueries`] over closures: `slide` runs once, `fits` is open space and `sweep`
    /// finds nothing unless told.
    struct Fake<S, F = fn(HullPose, Vec3, f32) -> bool> {
        slide: std::cell::RefCell<Option<S>>,
        fits: F,
        sweep: Box<dyn Fn(Vec3, Vec3, f32) -> f32>,
    }

    impl<S: FnOnce(&CharacterState, HullPose, Vec3) -> Slide> Fake<S> {
        fn new(slide: S) -> Self {
            Self {
                slide: std::cell::RefCell::new(Some(slide)),
                fits: |_, _, _| true,
                sweep: Box::new(clear_sweep),
            }
        }
    }

    impl<S, F> Fake<S, F> {
        /// The same fake with `fits` deciding whether a posed hull fits.
        fn fitting<G: Fn(HullPose, Vec3, f32) -> bool>(self, fits: G) -> Fake<S, G> {
            Fake {
                slide: self.slide,
                fits,
                sweep: self.sweep,
            }
        }

        fn sweeping(self, sweep: impl Fn(Vec3, Vec3, f32) -> f32 + 'static) -> Self {
            Self {
                sweep: Box::new(sweep),
                ..self
            }
        }
    }

    impl<S, F> CharacterQueries for Fake<S, F>
    where
        S: FnOnce(&CharacterState, HullPose, Vec3) -> Slide,
        F: Fn(HullPose, Vec3, f32) -> bool,
    {
        fn slide(&self, from: &CharacterState, pose: HullPose, asked: Vec3) -> Slide {
            let slide = self
                .slide
                .borrow_mut()
                .take()
                .expect("a step slides at most once");
            slide(from, pose, asked)
        }

        fn fits(&self, pose: HullPose, centre: Vec3, yaw: f32) -> bool {
            (self.fits)(pose, centre, yaw)
        }

        fn sweep(&self, from: Vec3, to: Vec3, radius: f32) -> f32 {
            (self.sweep)(from, to, radius)
        }
    }

    /// The default body under [`test_settings`] and its gravity.
    fn test_tuning() -> Tuning {
        let settings = test_settings();
        Tuning {
            body: CharacterBody::default(),
            gravity: settings.gravity.expect("test settings set gravity"),
            settings,
        }
    }

    fn collider() -> ColliderHandle {
        ColliderHandle::invalid()
    }

    /// [`super::step`] with no queued effects and a slide that only sees the asked motion.
    fn step_without_effects(
        state: &mut CharacterState,
        command: &CharacterCommand,
        settings: &MoveSettings,
        dt: f32,
        slide: impl FnOnce(Vec3) -> Slide,
    ) -> Vec<Impact> {
        let tuning = Tuning {
            settings: *settings,
            gravity: settings.gravity.expect("test settings set gravity"),
            ..test_tuning()
        };
        super::step(
            state,
            command,
            &[],
            &tuning,
            dt,
            &Fake::new(|_, _, asked| slide(asked)),
        )
    }

    fn state_at(position: Vec3, velocity: Vec3, grounded: bool) -> CharacterState {
        CharacterState {
            position,
            velocity,
            yaw: 0.0,
            pitch: 0.0,
            grounded,
            jump_held: false,
            eye: Vec3::ZERO,
            stance: Stance::Stand,
            from: Stance::Stand,
            progress: 0,
            anchor: Anchor::Feet,
            lean: 0.0,
            roll: 0.0,
        }
    }

    /// A walkable face of `normal` that the capsule hangs `gap` above.
    fn face(normal: Vec3, gap: f32) -> Ground {
        Ground {
            normal,
            gap,
            collider: collider(),
            point: Vec3::ZERO,
        }
    }

    fn standing() -> CharacterState {
        state_at(Vec3::ZERO, Vec3::ZERO, true)
    }

    fn forward() -> CharacterCommand {
        CharacterCommand {
            forward: 1.0,
            ..CharacterCommand::default()
        }
    }

    fn horizontal(v: Vec3) -> Vec3 {
        Vec3::new(v.x, 0.0, v.z)
    }

    /// Open air: the character goes exactly where it asked, touching nothing.
    fn open_air(asked: Vec3) -> Slide {
        Slide {
            translation: asked,
            grounded: false,
            ground: None,
            hits: Vec::new(),
        }
    }

    /// A flat floor at y = 0 under a character whose position is `y`.
    fn flat_floor(y: f32) -> impl FnOnce(Vec3) -> Slide {
        move |asked| {
            let landed = y + asked.y <= 0.0;
            let translation = Vec3::new(asked.x, if landed { -y } else { asked.y }, asked.z);
            let hits = if landed && asked.y < 0.0 {
                vec![(collider(), Vec3::Y)]
            } else {
                Vec::new()
            };
            Slide {
                translation,
                grounded: landed,
                ground: landed.then_some(face(Vec3::Y, 0.0)),
                hits,
            }
        }
    }

    fn walk(
        state: &mut CharacterState,
        command: &CharacterCommand,
        settings: &MoveSettings,
    ) -> Vec<Impact> {
        step_without_effects(state, command, settings, DT, flat_floor(state.position.y))
    }

    mod ground {
        use super::*;

        #[test]
        fn forward_at_speed_covers_max_speed_per_second_along_facing() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0), true);

            for _ in 0..60 {
                walk(&mut state, &forward(), &test_settings());
            }

            assert!(
                (state.position - Vec3::new(0.0, 0.0, 4.0)).length() < 1e-3,
                "{:?}",
                state.position
            );
        }

        #[test]
        fn forward_follows_the_facing_for_a_turned_character() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), true);
            state.yaw = std::f32::consts::FRAC_PI_2;

            for _ in 0..60 {
                walk(&mut state, &forward(), &test_settings());
            }

            assert!(
                (state.position - Vec3::new(4.0, 0.0, 0.0)).length() < 1e-3,
                "yaw pi/2 faces +X, {:?}",
                state.position
            );
        }

        #[test]
        fn right_input_moves_along_the_strategy_right_vector() {
            let cases = [
                (0.0, Vec3::new(-4.0, 0.0, 0.0)),
                (std::f32::consts::FRAC_PI_2, Vec3::new(0.0, 0.0, 4.0)),
            ];
            let command = CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            };

            for (yaw, right) in cases {
                let mut state = state_at(Vec3::ZERO, right, true);
                state.yaw = yaw;

                for _ in 0..60 {
                    walk(&mut state, &command, &test_settings());
                }

                assert!(
                    (state.position - right).length() < 1e-3,
                    "yaw {yaw} has right = {right:?}, got {:?}",
                    state.position
                );
            }
        }

        #[test]
        fn diagonal_at_speed_covers_max_speed_per_second() {
            let wish = Vec3::new(-1.0, 0.0, 1.0).normalize();
            let mut state = state_at(Vec3::ZERO, wish * 4.0, true);
            let command = CharacterCommand {
                forward: 1.0,
                right: 1.0,
                ..CharacterCommand::default()
            };

            for _ in 0..60 {
                walk(&mut state, &command, &test_settings());
            }

            assert!(
                (state.position.length() - 4.0).abs() < 1e-3,
                "{:?}",
                state.position
            );
            assert!(
                (state.position - wish * 4.0).length() < 1e-3,
                "{:?}",
                state.position
            );
        }

        #[test]
        fn half_analog_input_walks_at_half_max_speed() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), true);
            let command = CharacterCommand {
                forward: 0.5,
                ..CharacterCommand::default()
            };

            for _ in 0..60 {
                walk(&mut state, &command, &test_settings());
            }

            assert!(
                (state.position.z - 2.0).abs() < 1e-3,
                "{:?}",
                state.position
            );
        }

        #[test]
        fn pitch_does_not_change_ground_speed_or_height() {
            let mut state = state_at(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 4.0), true);
            state.pitch = 60.0_f32.to_radians();

            for _ in 0..60 {
                walk(&mut state, &forward(), &test_settings());
            }

            assert!(
                (horizontal(state.position) - Vec3::new(0.0, 0.0, 4.0)).length() < 1e-3,
                "{:?}",
                state.position
            );
            assert_eq!(state.position.y, 0.0);
        }

        #[test]
        fn raised_max_speed_applies_on_the_next_tick() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0), true);
            let mut settings = MoveSettings {
                ground_accel: 1000.0,
                ..test_settings()
            };
            walk(&mut state, &forward(), &settings);
            settings.max_speed = 6.0;
            let before = state.position;

            walk(&mut state, &forward(), &settings);

            assert!(
                ((state.position - before).z - 0.1).abs() < 1e-4,
                "{:?}",
                state.position - before
            );
        }

        /// Counter-Strike values, which Source's rules were tuned for.
        fn source_like() -> MoveSettings {
            MoveSettings {
                max_speed: 6.35,
                ground_accel: 63.5,
                friction: 4.0,
                stop_speed: 2.54,
                ..test_settings()
            }
        }

        /// Per-tick horizontal friction factor; 1 at rest.
        fn friction_scale(speed: f32, settings: &MoveSettings) -> f32 {
            if speed <= 0.0 {
                return 1.0;
            }
            let drop = speed.max(settings.stop_speed) * settings.friction * DT;
            (speed - drop).max(0.0) / speed
        }

        fn assert_close(actual: f32, expected: f32, tick: usize, what: &str) {
            assert!(
                (actual - expected).abs() <= 1e-5 * expected.abs() + 1e-6,
                "tick {tick}: {what} {actual}, expected {expected}"
            );
        }

        #[test]
        fn sideways_speed_decays_by_friction_only_while_forward_is_held() {
            let settings = source_like();
            let mut state = state_at(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), true);

            for tick in 0..60 {
                let before = state.velocity;
                let scale = friction_scale(horizontal(before).length(), &settings);

                walk(&mut state, &forward(), &settings);

                assert_close(state.velocity.x, before.x * scale, tick, "sideways speed");
            }
        }

        #[test]
        fn turning_keeps_the_old_directions_speed_to_friction() {
            let settings = source_like();
            let turn_right = CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            };
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, settings.max_speed), true);

            for tick in 0..30 {
                let before = state.velocity;
                let scale = friction_scale(horizontal(before).length(), &settings);
                let new_axis = -before.x * scale;
                let gain = (settings.max_speed - new_axis)
                    .min(settings.ground_accel * DT)
                    .max(0.0);

                walk(&mut state, &turn_right, &settings);

                assert_close(state.velocity.z, before.z * scale, tick, "old direction");
                assert_close(-state.velocity.x, new_axis + gain, tick, "new direction");
                assert!(
                    -state.velocity.x <= settings.max_speed + 1e-4,
                    "tick {tick}: new direction {} passed max speed",
                    -state.velocity.x
                );
            }
        }

        #[test]
        fn strategy_settings_reach_max_speed_in_one_tick_and_stop_in_one_tick() {
            for max_speed in [4.0, 6.0] {
                let settings = MoveSettings {
                    max_speed,
                    ground_accel: 360.0,
                    friction: 60.0,
                    stop_speed: 1.0,
                    air_accel: 360.0,
                    air_speed_cap: max_speed,
                    gravity: Some(18.0),
                    jump_speed: 8.0,
                    max_velocity: 90.0,
                    autohop: false,
                    crouch_ticks: 0,
                    prone_ticks: 0,
                    lean_ticks: 0,
                };
                let mut state = standing();

                walk(&mut state, &forward(), &settings);
                let pressed = horizontal(state.velocity).length();
                walk(&mut state, &CharacterCommand::default(), &settings);
                let released = horizontal(state.velocity).length();

                assert!(
                    (pressed - max_speed).abs() < 1e-4,
                    "max {max_speed}: after one tick of forward {pressed}"
                );
                assert!(
                    released < 1e-4,
                    "max {max_speed}: after one tick released {released}"
                );
            }
        }

        /// Source prestrafe, kept on purpose: input adds along the wish without removing old
        /// speed, so a turn at max speed overshoots (peak about 6.34 m/s after three ticks)
        /// and settles back over roughly 20 ticks.
        #[test]
        fn a_45_degree_turn_at_max_speed_overshoots_it_then_settles() {
            let settings = MoveSettings {
                max_speed: 6.0,
                ground_accel: 80.0,
                friction: 10.0,
                stop_speed: 1.0,
                air_accel: 20.0,
                air_speed_cap: 1.0,
                gravity: Some(18.0),
                jump_speed: 6.5,
                max_velocity: 90.0,
                autohop: false,
                crouch_ticks: 0,
                prone_ticks: 0,
                lean_ticks: 0,
            };
            let diagonal = CharacterCommand {
                forward: 1.0,
                right: 1.0,
                ..CharacterCommand::default()
            };
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 6.0), true);

            let mut speeds = Vec::new();
            for _ in 0..120 {
                walk(&mut state, &diagonal, &settings);
                speeds.push(horizontal(state.velocity).length());
            }

            assert!(
                (speeds[0] - 6.0171).abs() < 1e-3,
                "first tick speed {}",
                speeds[0]
            );
            assert!(
                (speeds[3] - 6.3393).abs() < 1e-3,
                "peak speed at tick 3 was {}",
                speeds[3]
            );
            for (tick, speed) in speeds.iter().enumerate().take(12) {
                assert!(
                    *speed > 6.0,
                    "tick {tick}: speed {speed} did not exceed max 6"
                );
            }
            let last = speeds[119];
            assert!(
                (last - 6.0).abs() < 1e-3,
                "after 120 ticks speed {last} did not settle at 6"
            );
        }

        #[test]
        fn ground_accel_reaches_max_speed_over_ticks_and_stops_there() {
            let settings = MoveSettings {
                ground_accel: 8.0,
                friction: 0.0,
                ..test_settings()
            };
            let mut state = standing();

            walk(&mut state, &forward(), &settings);
            let after_one = state.velocity.z;
            let mut peak = after_one;
            for _ in 1..29 {
                walk(&mut state, &forward(), &settings);
                peak = peak.max(state.velocity.z);
            }
            let almost = state.velocity.z;
            for _ in 29..120 {
                walk(&mut state, &forward(), &settings);
                peak = peak.max(state.velocity.z);
            }

            assert!((after_one - 8.0 / 60.0).abs() < 1e-4, "{after_one}");
            assert!(almost < 4.0 - 1e-3, "{almost}");
            assert!(
                (state.velocity.z - 4.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(peak <= 4.0 + 1e-4, "peak {peak}");
        }

        #[test]
        fn no_input_on_ground_slows_each_tick_to_rest() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0), true);
            let mut speeds = vec![state.velocity.z];

            for _ in 0..120 {
                walk(&mut state, &CharacterCommand::default(), &test_settings());
                speeds.push(state.velocity.z);
            }

            assert!(
                (speeds[1] - (4.0 - 4.0 * 4.0 * DT)).abs() < 1e-4,
                "first tick drops speed * friction * dt, got {}",
                speeds[1]
            );
            let slowing = speeds
                .windows(2)
                .take_while(|w| w[0] > 0.0)
                .all(|w| w[1] < w[0]);
            assert!(slowing, "{speeds:?}");
            assert_eq!(*speeds.last().unwrap(), 0.0);
            assert!(speeds.iter().all(|s| *s >= 0.0), "friction never reverses");
        }

        #[test]
        fn overspeed_along_the_wish_decays_by_friction_only_down_to_max_speed() {
            let settings = source_like();
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 12.0), true);
            let mut settled = false;

            for tick in 0..120 {
                let before = state.velocity.z;
                let scale = friction_scale(before, &settings);

                walk(&mut state, &forward(), &settings);

                if before * scale > settings.max_speed {
                    assert_close(state.velocity.z, before * scale, tick, "overspeed");
                } else {
                    settled = true;
                }
                if settled {
                    assert!(
                        state.velocity.z >= settings.max_speed - 1e-4,
                        "tick {tick}: speed {} dropped below max",
                        state.velocity.z
                    );
                }
            }

            assert!(settled, "never came down to max speed");
            assert!(
                (state.velocity.z - settings.max_speed).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn reversing_input_brakes_then_builds_speed_the_other_way_up_to_max() {
            let settings = source_like();
            let backward = CharacterCommand {
                forward: -1.0,
                ..CharacterCommand::default()
            };
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, settings.max_speed), true);
            let mut crossed_zero = false;

            for tick in 0..120 {
                let before = state.velocity.z;
                let scale = friction_scale(before.abs(), &settings);
                let slowed = before * scale;
                let expected =
                    slowed - (settings.ground_accel * DT).min(settings.max_speed + slowed);

                walk(&mut state, &backward, &settings);

                assert_close(state.velocity.z, expected, tick, "reversing speed");
                assert!(
                    state.velocity.z >= -settings.max_speed - 1e-4,
                    "tick {tick}: speed {} passed max backwards",
                    state.velocity.z
                );
                crossed_zero |= state.velocity.z < 0.0;
            }

            assert!(crossed_zero, "never reversed direction");
            assert!(
                (state.velocity.z + settings.max_speed).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn lowering_analog_input_at_speed_decays_by_friction_to_the_smaller_limit() {
            let settings = test_settings();
            let half = CharacterCommand {
                forward: 0.5,
                ..CharacterCommand::default()
            };
            let limit = 0.5 * settings.max_speed;
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, settings.max_speed), true);

            for tick in 0..120 {
                let before = state.velocity.z;
                let scale = friction_scale(before, &settings);

                walk(&mut state, &half, &settings);

                if before * scale > limit {
                    assert_close(state.velocity.z, before * scale, tick, "speed above limit");
                }
                assert!(
                    state.velocity.z >= limit - 1e-4 || before * scale > limit,
                    "tick {tick}: speed {} fell below the limit",
                    state.velocity.z
                );
            }

            assert!(
                (state.velocity.z - limit).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        fn slope_normal(degrees: f32) -> Vec3 {
            let angle = degrees.to_radians();
            Vec3::new(0.0, angle.cos(), -angle.sin())
        }

        fn on_ground(normal: Option<Vec3>) -> impl FnOnce(Vec3) -> Slide {
            move |asked| Slide {
                translation: asked,
                grounded: true,
                ground: normal.map(|normal| face(normal, 0.0)),
                hits: Vec::new(),
            }
        }

        fn frictionless() -> MoveSettings {
            MoveSettings {
                friction: 0.0,
                ..test_settings()
            }
        }

        #[test]
        fn a_grounded_slide_without_walkable_ground_leaves_a_walker_airborne() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -1.87, 4.0), true);

            step_without_effects(&mut state, &forward(), &frictionless(), DT, on_ground(None));

            assert!(
                (state.velocity.y + 1.87).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(
                (state.velocity.z - 4.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(!state.grounded, "{state:?}");
        }

        #[test]
        fn walker_whose_slide_ends_airborne_is_not_flattened_onto_the_ground() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -1.0, 4.0), true);

            step_without_effects(&mut state, &forward(), &frictionless(), DT, |asked| Slide {
                translation: asked,
                grounded: false,
                ground: Some(face(slope_normal(25.0), 1.0)),
                hits: Vec::new(),
            });

            assert!(!state.grounded, "{state:?}");
            assert!(
                (state.velocity.y + 1.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        /// A slide that moves as asked and ends over `ground` with the controller's `grounded`.
        fn over(grounded: bool, normal: Vec3, gap: f32) -> impl FnOnce(Vec3) -> Slide {
            move |asked| Slide {
                translation: asked,
                grounded,
                ground: Some(face(normal, gap)),
                hits: Vec::new(),
            }
        }

        fn run_over(
            mut state: CharacterState,
            command: &CharacterCommand,
            slide: impl FnOnce(Vec3) -> Slide,
        ) -> CharacterState {
            step_without_effects(&mut state, command, &frictionless(), DT, slide);
            state
        }

        const START_Y: f32 = 3.0;

        fn walker_at_speed(vy: f32) -> CharacterState {
            state_at(Vec3::new(0.0, START_Y, 0.0), Vec3::new(0.0, vy, 4.0), true)
        }

        #[test]
        fn walker_hanging_over_flat_ground_is_lowered_onto_it_and_stays_grounded() {
            let after = run_over(walker_at_speed(0.0), &forward(), over(false, Vec3::Y, 0.05));

            assert!(after.grounded, "{after:?}");
            assert!(
                (after.position.y - (START_Y - 0.05)).abs() < 1e-5,
                "{after:?}"
            );
            assert_eq!(after.velocity.y, 0.0);
        }

        #[test]
        fn walker_hanging_over_a_slope_is_lowered_and_follows_it() {
            let climb = 4.0 * 25.0_f32.to_radians().tan();
            let after = run_over(
                walker_at_speed(climb),
                &forward(),
                over(false, slope_normal(25.0), 0.05),
            );

            assert!(after.grounded, "{after:?}");
            let expected_y = START_Y + climb * DT - 0.05;
            assert!((after.position.y - expected_y).abs() < 1e-4, "{after:?}");
            assert!((after.velocity.y - climb).abs() < 1e-3, "{after:?}");
        }

        #[test]
        fn walker_hanging_within_the_step_height_on_a_level_move_is_lowered() {
            for gap in [GROUND_SNAP + 0.01, 0.3, STEP_HEIGHT] {
                let after = run_over(walker_at_speed(0.0), &forward(), over(false, Vec3::Y, gap));

                assert!(after.grounded, "gap {gap}: {after:?}");
                assert!(
                    (after.position.y - (START_Y - gap)).abs() < 1e-5,
                    "gap {gap}: {after:?}"
                );
                assert_eq!(after.velocity.y, 0.0, "gap {gap}");
            }
        }

        #[test]
        fn walker_with_a_tiny_rising_ask_still_gets_the_step_height_reach() {
            // Float noise in a level ground normal asks a few millionths of a metre upward,
            // under REST_SPEED * DT, which must not shorten the reach to the snap distance.
            let noise = 3e-4;
            let asked_y = noise * DT;
            assert!(asked_y > 0.0 && asked_y < REST_SPEED * DT);

            let after = run_over(
                walker_at_speed(noise),
                &forward(),
                over(false, Vec3::Y, 0.3),
            );

            assert!(after.grounded, "{after:?}");
            assert!(
                (after.position.y - (START_Y + asked_y - 0.3)).abs() < 1e-5,
                "{after:?}"
            );
        }

        #[test]
        fn walker_on_a_downhill_move_is_lowered_within_the_step_height() {
            let descent = -1.0;

            let after = run_over(
                walker_at_speed(descent),
                &forward(),
                over(false, Vec3::Y, 0.3),
            );

            assert!(after.grounded, "{after:?}");
            let expected_y = START_Y + descent * DT - 0.3;
            assert!((after.position.y - expected_y).abs() < 1e-5, "{after:?}");
        }

        #[test]
        fn walker_rising_is_lowered_only_within_the_snap_distance() {
            let climb = 1.0;

            let near = run_over(
                walker_at_speed(climb),
                &forward(),
                over(false, Vec3::Y, GROUND_SNAP),
            );
            let far = run_over(
                walker_at_speed(climb),
                &forward(),
                over(false, Vec3::Y, GROUND_SNAP + 0.01),
            );

            assert!(near.grounded, "{near:?}");
            assert!(
                (near.position.y - (START_Y + climb * DT - GROUND_SNAP)).abs() < 1e-5,
                "{near:?}"
            );
            assert!(!far.grounded, "a rising walker keeps its launch: {far:?}");
            assert!(
                (far.position.y - (START_Y + climb * DT)).abs() < 1e-5,
                "{far:?}"
            );
        }

        #[test]
        fn walker_hanging_beyond_the_step_height_on_a_level_move_is_left_airborne_where_it_is() {
            let after = run_over(
                walker_at_speed(0.0),
                &forward(),
                over(false, Vec3::Y, STEP_HEIGHT + 0.01),
            );

            assert!(!after.grounded, "{after:?}");
            assert!((after.position.y - START_Y).abs() < 1e-5, "{after:?}");
        }

        #[test]
        fn walker_sunk_below_the_plane_stays_grounded_without_being_raised() {
            let after = run_over(
                walker_at_speed(0.0),
                &forward(),
                over(false, Vec3::Y, -0.02),
            );

            assert!(after.grounded, "{after:?}");
            assert!((after.position.y - START_Y).abs() < 1e-5, "{after:?}");
        }

        #[test]
        fn walker_the_slide_already_grounded_is_not_lowered() {
            let after = run_over(walker_at_speed(0.0), &forward(), over(true, Vec3::Y, 0.05));

            assert!(after.grounded, "{after:?}");
            assert!((after.position.y - START_Y).abs() < 1e-5, "{after:?}");
        }

        #[test]
        fn character_airborne_before_the_tick_is_not_snapped_onto_ground() {
            let falling = state_at(
                Vec3::new(0.0, START_Y, 0.0),
                Vec3::new(0.0, 0.0, 4.0),
                false,
            );

            let after = run_over(falling, &forward(), over(false, Vec3::Y, 0.05));

            assert!(!after.grounded, "{after:?}");
            assert!(
                (after.position.y - (START_Y - 18.0 * DT * DT / 2.0)).abs() < 1e-5,
                "only gravity moved it: {after:?}"
            );
        }

        #[test]
        fn jumping_character_is_not_snapped_back_onto_ground() {
            let jump = CharacterCommand {
                jump: true,
                ..forward()
            };

            let after = run_over(walker_at_speed(0.0), &jump, over(false, Vec3::Y, 0.05));

            assert!(!after.grounded, "{after:?}");
            assert!(after.position.y > START_Y, "{after:?}");
        }

        #[test]
        fn upslope_walker_stays_grounded() {
            let normal = slope_normal(25.0);
            let climb = 4.0 * 25.0_f32.to_radians().tan();
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, climb, 4.0), true);

            step_without_effects(
                &mut state,
                &forward(),
                &frictionless(),
                DT,
                on_ground(Some(normal)),
            );

            assert!(state.grounded, "{state:?}");
            assert!(
                (state.velocity.y - climb).abs() < 1e-3,
                "keeps climbing along the slope: {:?}",
                state.velocity
            );
            assert!(
                state.velocity.dot(normal).abs() < 1e-3,
                "{:?}",
                state.velocity
            );
        }

        proptest! {
            #[test]
            fn walker_velocity_follows_the_ground_keeping_horizontal_speed(
                tilt in 0.1f32..0.75,
                heading in -3.1f32..3.1,
                speed in 1.0f32..4.0,
                travel in -3.1f32..3.1,
            ) {
                let settings = test_settings();
                let normal = Vec3::new(
                    tilt.sin() * heading.cos(),
                    tilt.cos(),
                    tilt.sin() * heading.sin(),
                );
                let start = Vec3::new(travel.sin(), 0.0, travel.cos()) * speed;
                let mut expected = start;
                ground_move(&mut expected, Vec3::Z, 1.0, &settings, DT);
                let mut state = state_at(Vec3::ZERO, start, true);

                step_without_effects(&mut state, &forward(), &settings, DT, on_ground(Some(normal)));

                prop_assert!(state.grounded);
                prop_assert!(
                    state.velocity.dot(normal).abs() < 1e-3,
                    "{:?} is not along {normal:?}", state.velocity
                );
                prop_assert!(
                    (horizontal(state.velocity) - horizontal(expected)).length() < 1e-4,
                    "{:?} vs {expected:?}", state.velocity
                );
            }
        }

        #[test]
        fn slow_character_stops_at_the_stop_speed_rate() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.5), true);

            walk(&mut state, &CharacterCommand::default(), &test_settings());

            assert!(
                (state.velocity.z - (0.5 - 1.0 * 4.0 * DT)).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }
    }

    mod air {
        use super::*;

        fn drift(state: &mut CharacterState, command: &CharacterCommand) {
            step_without_effects(state, command, &test_settings(), DT, open_air_slide);
        }

        fn open_air_slide(asked: Vec3) -> Slide {
            open_air(asked)
        }

        #[test]
        fn airborne_without_input_keeps_horizontal_speed() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 6.0), true);
            let jump = CharacterCommand {
                jump: true,
                ..CharacterCommand::default()
            };
            let mut airborne_ticks = 0;

            step_without_effects(&mut state, &jump, &test_settings(), DT, flat_floor(0.0));
            for _ in 0..200 {
                if state.grounded {
                    break;
                }
                assert!(
                    (horizontal(state.velocity).length() - 6.0).abs() < 1e-4,
                    "{:?}",
                    state.velocity
                );
                airborne_ticks += 1;
                walk(&mut state, &CharacterCommand::default(), &test_settings());
            }

            assert!(airborne_ticks > 30, "the jump stays up for a while");
            assert!(state.grounded, "and comes back down");
        }

        #[test]
        fn air_gain_along_wish_never_exceeds_cap() {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::ZERO, false);
            let command = CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            };
            let wish = Vec3::new(-1.0, 0.0, 0.0);
            drift(&mut state, &command);
            let first_gain = state.velocity.dot(wish);
            let mut previous = first_gain;
            let mut largest_gain = first_gain;
            for _ in 1..120 {
                drift(&mut state, &command);
                let along = state.velocity.dot(wish);
                largest_gain = largest_gain.max(along - previous);
                previous = along;
                assert!(along <= 0.8 + 1e-4, "along-wish speed {along}");
            }

            assert!(
                (first_gain - 10.0 / 60.0).abs() < 1e-4,
                "air accel adds air_accel * dt from rest: {first_gain}"
            );
            assert!(largest_gain > 0.0, "air control does something");
            assert!(largest_gain <= 0.8 + 1e-4, "gain {largest_gain}");
            assert!((previous - 0.8).abs() < 1e-3, "reaches the cap: {previous}");
        }

        #[test]
        fn air_cap_limits_speed_along_the_wish_not_total_speed() {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::new(0.0, 0.0, 6.0), false);
            let command = CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            };

            for _ in 0..120 {
                drift(&mut state, &command);
            }

            assert!(
                (state.velocity.x + 0.8).abs() < 1e-3,
                "{:?}",
                state.velocity
            );
            assert!(
                (state.velocity.z - 6.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn air_wish_does_not_slow_a_character_already_faster_than_the_cap() {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::new(-3.0, 0.0, 0.0), false);
            let command = CharacterCommand {
                right: 1.0,
                ..CharacterCommand::default()
            };

            drift(&mut state, &command);

            assert!(
                (state.velocity.x + 3.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn jump_tick_asks_the_average_of_impulse_and_end_velocity() {
            let settings = test_settings();
            let mut state = standing();
            let jump = CharacterCommand {
                jump: true,
                ..CharacterCommand::default()
            };
            let mut asked = Vec3::ZERO;

            step_without_effects(&mut state, &jump, &settings, DT, |wanted| {
                asked = wanted;
                open_air(wanted)
            });

            assert!(
                (asked.y
                    - (settings.jump_speed
                        - settings.gravity.expect("test settings set gravity") * DT / 2.0)
                        * DT)
                    .abs()
                    < 1e-6,
                "the jump tick asks (jump - g*dt/2)*dt, asked y = {}",
                asked.y
            );
            assert!(
                (state.velocity.y
                    - (settings.jump_speed
                        - settings.gravity.expect("test settings set gravity") * DT))
                    .abs()
                    < 1e-5,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn airborne_tick_asks_the_average_of_start_and_end_velocity() {
            let start_vy = 3.0;
            let mut state = state_at(
                Vec3::new(0.0, 50.0, 0.0),
                Vec3::new(0.0, start_vy, 0.0),
                false,
            );
            let mut asked = Vec3::ZERO;

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |wanted| {
                    asked = wanted;
                    open_air(wanted)
                },
            );

            assert!(
                (asked.y - (start_vy - 18.0 * DT / 2.0) * DT).abs() < 1e-6,
                "the slide asks (v - g*dt/2)*dt, asked y = {}",
                asked.y
            );
            assert!(
                (state.velocity.y - (start_vy - 18.0 * DT)).abs() < 1e-5,
                "full gravity by the end of the tick, {:?}",
                state.velocity
            );
            assert!(
                (state.position.y - (50.0 + asked.y)).abs() < 1e-6,
                "y = {}",
                state.position.y
            );
        }
    }

    mod jump {
        use super::*;

        #[test]
        fn jump_from_ground_rises_and_air_jump_does_nothing() {
            let mut state = standing();
            let jump = CharacterCommand {
                jump: true,
                ..CharacterCommand::default()
            };

            walk(&mut state, &jump, &test_settings());
            let first_height = state.position.y;
            let first_velocity = state.velocity.y;
            let first_grounded = state.grounded;
            walk(&mut state, &jump, &test_settings());

            assert!(first_height > 0.0, "rises: {first_height}");
            assert!(
                (first_velocity - (8.0 - 18.0 * DT)).abs() < 1e-3,
                "{first_velocity}"
            );
            assert!(!first_grounded, "a rising character is airborne");
            assert!(
                (state.velocity.y - (8.0 - 2.0 * 18.0 * DT)).abs() < 1e-3,
                "the second jump must not relaunch, v.y = {}",
                state.velocity.y
            );
        }

        #[test]
        fn jump_adds_the_impulse_to_the_ground_velocity() {
            let slope = 25.0_f32.to_radians();
            let climb = 4.0 * slope.tan();
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, climb, 4.0), true);
            let jump = CharacterCommand {
                jump: true,
                ..forward()
            };

            step_without_effects(&mut state, &jump, &test_settings(), DT, |asked| Slide {
                translation: asked,
                grounded: false,
                ground: None,
                hits: Vec::new(),
            });

            assert!(
                (state.velocity.y - (climb + 8.0 - 18.0 * DT)).abs() < 1e-3,
                "{:?}",
                state.velocity
            );
            assert!(
                (state.velocity.z - 4.0).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(!state.grounded);
        }

        fn grounded_slide(ground: Option<Vec3>) -> impl FnOnce(Vec3) -> Slide {
            move |asked| Slide {
                translation: asked,
                grounded: true,
                ground: ground.map(|normal| face(normal, 0.0)),
                hits: Vec::new(),
            }
        }

        #[test]
        fn jump_smaller_than_the_horizontal_speed_still_leaves_the_ground() {
            let settings = MoveSettings {
                jump_speed: 2.0,
                max_velocity: 90.0,
                ..test_settings()
            };
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 4.0), true);
            let jump = CharacterCommand {
                jump: true,
                ..forward()
            };

            step_without_effects(
                &mut state,
                &jump,
                &settings,
                DT,
                grounded_slide(Some(Vec3::Y)),
            );

            assert!(!state.grounded, "{state:?}");
            assert!(
                (state.velocity.y - (2.0 - 18.0 * DT)).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn upward_speed_above_the_horizontal_speed_is_airborne_even_on_flat_ground() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 2.0, 1.0), true);

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                grounded_slide(Some(Vec3::Y)),
            );

            assert!(!state.grounded, "{state:?}");
            assert!(
                (state.velocity.y - (2.0 - 18.0 * DT)).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn grounded_without_jump_command_stays_on_the_ground() {
            let mut state = standing();

            let impacts = walk(&mut state, &CharacterCommand::default(), &test_settings());

            assert!(impacts.is_empty(), "{impacts:?}");
            assert!(state.grounded);
            assert_eq!(state.velocity.y, 0.0);
            assert_eq!(state.position.y, 0.0);
        }

        #[test]
        fn upward_velocity_is_airborne_even_when_slide_says_grounded() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0), true);

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
                    ground: None,
                    hits: Vec::new(),
                },
            );

            assert!(!state.grounded);
            assert!(
                (state.velocity.y - (5.0 - 18.0 * DT)).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn grounded_slide_zeroes_downward_speed() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -3.0, 0.0), false);

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
                    ground: Some(face(Vec3::Y, 0.0)),
                    hits: Vec::new(),
                },
            );

            assert!(state.grounded);
            assert_eq!(state.velocity.y, 0.0);
        }

        #[test]
        fn a_landing_without_walkable_ground_keeps_its_clipped_downhill_speed() {
            let steep = 60.0_f32.to_radians();
            let normal = Vec3::new(0.0, steep.cos(), steep.sin());
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -3.0, 0.0), false);

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |asked| Slide {
                    translation: asked - normal * asked.dot(normal),
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), normal)],
                },
            );

            assert!(!state.grounded, "{state:?}");
            let half_gravity = 18.0 * DT / 2.0;
            assert!(
                (state.velocity.dot(normal) + half_gravity * normal.y).abs() < 1e-3,
                "clipped along the face, then half a tick of gravity: {:?}",
                state.velocity
            );
            assert!(state.velocity.y < -1.0, "{:?}", state.velocity);
            assert!(state.velocity.z > 1.0, "{:?}", state.velocity);
        }

        fn jump_command(jump: bool) -> CharacterCommand {
            CharacterCommand {
                jump,
                ..CharacterCommand::default()
            }
        }

        fn took_off(state: &CharacterState) -> bool {
            let expected = test_settings().jump_speed - 18.0 * DT;
            !state.grounded && (state.velocity.y - expected).abs() < 1e-3
        }

        #[test]
        fn a_press_made_in_the_air_and_held_does_not_jump_on_landing() {
            let mut state = state_at(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -3.0, 0.0), false);
            let held = jump_command(true);
            walk(&mut state, &held, &test_settings());
            assert!(!state.grounded, "still falling after the first tick");
            walk(&mut state, &held, &test_settings());
            assert!(state.grounded, "landed on the second tick: {state:?}");

            walk(&mut state, &held, &test_settings());

            assert!(state.grounded, "stayed on the ground: {state:?}");
            assert!(
                state.velocity.y.abs() < 1e-4,
                "no jump from a press made before landing: {:?}",
                state.velocity
            );
            walk(&mut state, &jump_command(false), &test_settings());
            walk(&mut state, &held, &test_settings());
            assert!(took_off(&state), "release then press jumps: {state:?}");
        }

        proptest! {
            #[test]
            fn jump_held_follows_the_command_every_tick(
                started_held in any::<bool>(),
                autohop in any::<bool>(),
                presses in prop::collection::vec(any::<bool>(), 1..40),
            ) {
                let settings = MoveSettings { autohop, ..test_settings() };
                let mut state = standing();
                state.jump_held = started_held;

                for (tick, jump) in presses.into_iter().enumerate() {
                    walk(&mut state, &jump_command(jump), &settings);

                    prop_assert_eq!(state.jump_held, jump, "tick {}", tick);
                }
            }
        }

        #[test]
        fn an_effect_does_not_release_jump() {
            let effects = [
                CharacterEffect::SetLook {
                    yaw: 0.0,
                    pitch: 0.0,
                },
                CharacterEffect::AddVelocity(Vec3::ZERO),
                CharacterEffect::SetVelocity(Vec3::ZERO),
            ];
            for effect in effects {
                let mut state = standing();
                state.jump_held = true;

                super::step(
                    &mut state,
                    &jump_command(true),
                    &[effect],
                    &test_tuning(),
                    DT,
                    &Fake::new(|from, _, asked| flat_floor(from.position.y)(asked)),
                );

                assert!(state.grounded, "{effect:?}: {state:?}");
                assert!(
                    state.velocity.y.abs() < 1e-4,
                    "{effect:?} released the held jump: {:?}",
                    state.velocity
                );
                assert!(state.jump_held, "{effect:?}");
            }
        }
    }

    mod look {
        use super::*;

        #[test]
        fn look_deltas_turn_yaw_and_pitch() {
            let mut state = standing();
            let command = CharacterCommand {
                yaw_delta: 0.25,
                pitch_delta: -0.5,
                ..CharacterCommand::default()
            };

            walk(&mut state, &command, &test_settings());
            walk(&mut state, &command, &test_settings());

            assert!((state.yaw - 0.5).abs() < 1e-6, "{}", state.yaw);
            assert!((state.pitch + 1.0).abs() < 1e-6, "{}", state.pitch);
        }

        #[test]
        fn pitch_stops_short_of_vertical() {
            let mut up = standing();
            let mut down = standing();
            let look_up = CharacterCommand {
                pitch_delta: 3.0,
                ..CharacterCommand::default()
            };
            let look_down = CharacterCommand {
                pitch_delta: -3.0,
                ..CharacterCommand::default()
            };

            walk(&mut up, &look_up, &test_settings());
            walk(&mut down, &look_down, &test_settings());

            assert_eq!(PITCH_LIMIT, 1.54);
            assert_eq!(up.pitch, PITCH_LIMIT);
            assert_eq!(down.pitch, -PITCH_LIMIT);
            assert!(up.pitch < std::f32::consts::FRAC_PI_2);
        }

        #[test]
        fn yaw_turn_applies_before_the_wish_direction_is_taken() {
            let mut state = state_at(Vec3::ZERO, Vec3::ZERO, true);
            let command = CharacterCommand {
                forward: 1.0,
                yaw_delta: std::f32::consts::FRAC_PI_2,
                ..CharacterCommand::default()
            };

            walk(&mut state, &command, &test_settings());

            assert!(state.velocity.x > 0.1, "{:?}", state.velocity);
            assert!(state.velocity.z.abs() < 1e-4, "{:?}", state.velocity);
        }
    }

    mod impacts {
        use super::*;

        fn wall_normal() -> Vec3 {
            Vec3::new(0.0, 0.0, -1.0)
        }

        fn frictionless() -> MoveSettings {
            MoveSettings {
                friction: 0.0,
                ..test_settings()
            }
        }

        #[test]
        fn blocked_hit_clips_velocity_and_reports_the_speed_into_it() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 10.0), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked * 0.25,
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert_eq!(impacts.len(), 1);
            assert!((impacts[0].speed - 10.0).abs() < 1e-4, "{:?}", impacts[0]);
            assert_eq!(impacts[0].normal, wall_normal());
            assert_eq!(impacts[0].collider, collider());
            assert!(state.velocity.z.abs() < 1e-4, "{:?}", state.velocity);
        }

        #[test]
        fn glancing_hit_keeps_the_speed_along_the_surface() {
            let along = 4.0 * std::f32::consts::FRAC_1_SQRT_2;
            let mut state = state_at(Vec3::ZERO, Vec3::new(along, 0.0, along), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: Vec3::new(asked.x, 0.0, 0.0),
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert_eq!(impacts.len(), 1);
            assert!((impacts[0].speed - along).abs() < 1e-4, "{:?}", impacts[0]);
            assert!(
                (state.velocity.x - along).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(state.velocity.z.abs() < 1e-4, "{:?}", state.velocity);
        }

        #[test]
        fn hit_that_did_not_block_motion_reports_nothing_and_keeps_velocity() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 3.0), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert!(impacts.is_empty(), "{impacts:?}");
            assert!(
                (state.velocity.z - 3.0).abs() < 1e-6,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn step_up_lift_with_a_riser_hit_reports_nothing_and_keeps_velocity() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 3.0), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked + Vec3::new(0.0, 0.45, 0.0),
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert!(impacts.is_empty(), "{impacts:?}");
            assert!(
                (state.velocity.z - 3.0).abs() < 1e-6,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn corner_with_two_blocking_normals_on_one_collider_reports_both() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(3.0, 0.0, 3.0), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |_| Slide {
                    translation: Vec3::ZERO,
                    grounded: true,
                    ground: None,
                    hits: vec![
                        (collider(), Vec3::new(0.0, 0.0, -1.0)),
                        (collider(), Vec3::new(-1.0, 0.0, 0.0)),
                    ],
                },
            );

            assert_eq!(impacts.len(), 2, "{impacts:?}");
            assert!(
                impacts.iter().all(|i| (i.speed - 3.0).abs() < 1e-4),
                "{impacts:?}"
            );
            assert!(
                horizontal(state.velocity).length() < 1e-4,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn same_wall_hit_twice_in_one_slide_reports_once() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 10.0), true);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked * 0.25,
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal()), (collider(), wall_normal())],
                },
            );

            assert_eq!(impacts.len(), 1, "{impacts:?}");
        }

        #[test]
        fn landing_beside_a_wall_the_slide_was_not_stopped_by_reports_no_wall_impact() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -2.0, 3.0), false);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert!(impacts.is_empty(), "{impacts:?}");
            assert_eq!(state.velocity.z, 3.0);
        }

        #[test]
        fn moving_short_by_exactly_the_tolerance_is_not_blocked() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 30.0), false);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked.with_z(-(asked.dot(wall_normal()) + BLOCKED_EPSILON)),
                    grounded: false,
                    ground: None,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert!(impacts.is_empty(), "{impacts:?}");
            assert_eq!(state.velocity.z, 30.0);
        }

        #[test]
        fn surface_the_asked_motion_runs_along_does_not_block_velocity_turned_into_it() {
            let mut velocity = Vec3::new(1.0, 0.0, -1.0);
            let slid = Slide {
                translation: Vec3::new(0.1, 0.0, 0.01),
                grounded: true,
                ground: None,
                hits: vec![(collider(), Vec3::Z)],
            };

            let impacts = clip(&mut velocity, Vec3::new(0.1, 0.0, 0.0), &slid, false, false);

            assert!(impacts.is_empty(), "{impacts:?}");
            assert_eq!(velocity, Vec3::new(1.0, 0.0, -1.0));
        }

        #[test]
        fn landing_impact_reports_the_mid_tick_fall_speed() {
            let settings = test_settings();
            let mut state = state_at(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -12.0, 0.0), false);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &settings,
                DT,
                flat_floor(0.1),
            );

            assert_eq!(impacts.len(), 1, "{impacts:?}");
            assert!(
                (impacts[0].speed
                    - (12.0 + settings.gravity.expect("test settings set gravity") * DT / 2.0))
                    .abs()
                    < 1e-4,
                "half gravity before the slide, so the impact is 12.15: {:?}",
                impacts[0]
            );
            assert_eq!(state.velocity.y, 0.0);
            assert!(state.grounded);
        }

        #[test]
        fn landing_hit_reports_fall_speed_and_zeroes_vertical_velocity() {
            let settings = MoveSettings {
                gravity: Some(0.0),
                ..test_settings()
            };
            let mut state = state_at(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -12.0, 0.0), false);

            let impacts = step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &settings,
                DT,
                flat_floor(0.1),
            );

            assert_eq!(impacts.len(), 1);
            assert!((impacts[0].speed - 12.0).abs() < 1e-4, "{:?}", impacts[0]);
            assert_eq!(impacts[0].normal, Vec3::Y);
            assert_eq!(state.velocity.y, 0.0);
            assert!(state.grounded);
        }
    }

    #[test]
    fn zero_dt_changes_nothing_but_look_and_the_jump_launch() {
        let mut state = state_at(Vec3::new(1.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 1.0), true);
        let command = CharacterCommand {
            forward: 1.0,
            yaw_delta: 0.3,
            jump: true,
            ..CharacterCommand::default()
        };

        let impacts =
            step_without_effects(&mut state, &command, &test_settings(), 0.0, flat_floor(0.0));

        assert!(impacts.is_empty(), "{impacts:?}");
        assert_eq!(state.position, Vec3::new(1.0, 0.0, 2.0));
        assert_eq!(state.velocity, Vec3::new(2.0, 8.0, 1.0));
        assert!((state.yaw - 0.3).abs() < 1e-6, "{}", state.yaw);
        assert!(!state.grounded, "a launched character is airborne");
    }

    #[test]
    fn step_writes_only_the_given_state() {
        let start = state_at(Vec3::new(1.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 3.0), true);
        let command = CharacterCommand {
            forward: 1.0,
            right: 0.5,
            yaw_delta: 0.1,
            pitch_delta: 0.2,
            jump: true,
            stance: Stance::Stand,
            lean: 0.0,
        };
        let mut first = start;
        let mut second = start;

        let first_impacts =
            step_without_effects(&mut first, &command, &test_settings(), DT, flat_floor(0.0));
        let second_impacts =
            step_without_effects(&mut second, &command, &test_settings(), DT, flat_floor(0.0));

        assert_eq!(first, second);
        assert_eq!(first_impacts, second_impacts);
        assert_ne!(first, start, "the step did change the state it was given");
    }

    #[test]
    fn step_writes_the_eye_from_the_tuned_body() {
        let mut state = standing();
        let tuning = Tuning {
            body: CharacterBody {
                standing: Stature {
                    hull: Hull {
                        shape: HullShape::Capsule,
                        radius: 0.405,
                        length: 1.83,
                    },
                    eye_height: 1.63,
                },
                crouched: None,
                prone: None,
                lean: Lean::default(),
            },
            ..test_tuning()
        };

        super::step(
            &mut state,
            &CharacterCommand::default(),
            &[],
            &tuning,
            DT,
            &Fake::new(|_, _, asked| flat_floor(0.0)(asked)),
        );

        assert!(
            (state.eye - Vec3::Y * (1.63 - 0.915)).length() < 1e-6,
            "{}",
            state.eye
        );
    }

    proptest! {
        #[test]
        fn ground_speed_from_rest_never_exceeds_max_speed_times_input(
            forward in -2.0f32..2.0,
            right in -2.0f32..2.0,
            yaw in -7.0f32..7.0,
        ) {
            let settings = test_settings();
            let command = CharacterCommand { forward, right, ..CharacterCommand::default() };
            let limit = settings.max_speed * forward.hypot(right).min(1.0);
            let mut state = standing();
            state.yaw = yaw;

            for _ in 0..120 {
                walk(&mut state, &command, &settings);
                prop_assert!(
                    horizontal(state.velocity).length() <= limit + 1e-3,
                    "{:?} over limit {limit}", state.velocity
                );
            }

            prop_assert!(
                (horizontal(state.velocity).length() - limit).abs() <= 1e-3,
                "{:?} did not settle at {limit}", state.velocity
            );
        }

        #[test]
        fn air_gain_along_wish_never_exceeds_cap_for_any_input(
            forward in -1.0f32..1.0,
            right in -1.0f32..1.0,
            yaw in -7.0f32..7.0,
            vx in -10.0f32..10.0,
            vz in -10.0f32..10.0,
        ) {
            let settings = test_settings();
            let command = CharacterCommand { forward, right, ..CharacterCommand::default() };
            let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            let right_dir = Vec3::new(-yaw.cos(), 0.0, yaw.sin());
            let wish = (fwd * forward + right_dir * right).normalize_or_zero();
            prop_assume!(wish != Vec3::ZERO);
            let mut state = state_at(Vec3::new(0.0, 100.0, 0.0), Vec3::new(vx, 0.0, vz), false);
            state.yaw = yaw;
            let before = state.velocity.dot(wish);

            step_without_effects(&mut state, &command, &settings, DT, open_air);

            let after = state.velocity.dot(wish);
            prop_assert!(
                after <= before.max(settings.air_speed_cap) + 1e-3,
                "before {before}, after {after}"
            );
            prop_assert!(after - before <= settings.air_speed_cap + 1e-3);
        }

        #[test]
        fn pitch_stays_within_limit(
            deltas in proptest::collection::vec(-5.0f32..5.0, 1..40),
            start in -1.54f32..1.54,
        ) {
            let mut state = standing();
            state.pitch = start;

            for delta in deltas {
                let command = CharacterCommand { pitch_delta: delta, ..CharacterCommand::default() };
                walk(&mut state, &command, &test_settings());
                prop_assert!(state.pitch.abs() <= PITCH_LIMIT, "{}", state.pitch);
            }
        }
    }

    #[test]
    fn commands_and_effects_round_trip_through_postcard() {
        let commands = [
            CharacterCommand::default(),
            CharacterCommand {
                forward: 1.0,
                right: -0.5,
                yaw_delta: 0.25,
                pitch_delta: -0.125,
                jump: true,
                stance: Stance::Crouch,
                lean: -0.75,
            },
        ];
        let effects = [
            CharacterEffect::AddVelocity(Vec3::new(1.0, -2.0, 3.5)),
            CharacterEffect::SetVelocity(Vec3::new(0.0, 8.0, -1.0)),
            CharacterEffect::Teleport {
                position: Vec3::new(3.0, 20.0, -4.0),
                keep_velocity: true,
            },
            CharacterEffect::Teleport {
                position: Vec3::ZERO,
                keep_velocity: false,
            },
            CharacterEffect::SetLook {
                yaw: 0.7,
                pitch: -0.3,
            },
        ];

        let command_bytes = postcard::to_allocvec(&commands).expect("commands serialise");
        let effect_bytes = postcard::to_allocvec(&effects).expect("effects serialise");

        let back_commands: [CharacterCommand; 2] =
            postcard::from_bytes(&command_bytes).expect("commands deserialise");
        let back_effects: [CharacterEffect; 5] =
            postcard::from_bytes(&effect_bytes).expect("effects deserialise");
        assert_eq!(back_commands, commands);
        assert_eq!(back_effects, effects);
    }

    mod clip {
        use super::*;

        fn handle(index: u32) -> ColliderHandle {
            ColliderHandle::from_raw_parts(index, 0)
        }

        /// Normals of an inside corner opening toward -Z, `half_angle` radians either side of
        /// that axis, facing into the opening.
        fn wedge(half_angle: f32) -> [Vec3; 2] {
            let (sin, cos) = half_angle.sin_cos();
            [Vec3::new(-cos, 0.0, -sin), Vec3::new(cos, 0.0, -sin)]
        }

        /// Clips an airborne character's `velocity` against `normals`, all of which block it:
        /// the slide moved nowhere and the asked motion faces every one.
        fn clipped(velocity: Vec3, normals: &[Vec3]) -> (Vec3, Vec<Impact>) {
            let slid = Slide {
                translation: Vec3::ZERO,
                grounded: false,
                ground: None,
                hits: (0u32..)
                    .zip(normals)
                    .map(|(index, &normal)| (handle(index), normal))
                    .collect(),
            };
            let mut after = velocity;

            let impacts = super::super::clip(&mut after, velocity * DT, &slid, false, false);

            (after, impacts)
        }

        /// The impact reported for the plane that `clipped` numbered `index`.
        fn impact_on(impacts: &[Impact], index: u32) -> &Impact {
            let found = impacts
                .iter()
                .filter(|impact| impact.collider == handle(index))
                .collect::<Vec<_>>();
            assert_eq!(
                found.len(),
                1,
                "one impact on collider {index}: {impacts:?}"
            );
            found[0]
        }

        #[test]
        fn velocity_entering_two_planes_lies_along_their_cross_product() {
            let [a, b] = wedge(std::f32::consts::FRAC_PI_6);
            let velocity = Vec3::new(1.0, -3.0, 5.0);

            let (after, impacts) = clipped(velocity, &[a, b]);

            let crease = a.cross(b).normalize();
            assert!(
                after.cross(crease).length() < 1e-5,
                "{after:?} is not along {crease:?}"
            );
            assert!(
                (after.dot(crease) - velocity.dot(crease)).abs() < 1e-5,
                "the speed along the crease is kept: {after:?}"
            );
            assert_eq!(impacts.len(), 2, "{impacts:?}");
            for (index, normal) in [a, b].into_iter().enumerate() {
                let impact = impact_on(&impacts, index as u32);
                assert_eq!(impact.normal, normal);
                assert!(
                    (impact.speed + velocity.dot(normal)).abs() < 1e-5,
                    "measured from the incoming velocity: {impact:?}"
                );
            }

            let slot = wedge(5.0f32.to_radians());
            let into_the_slot = Vec3::new(0.0, 0.0, 10.0);

            let (after, impacts) = clipped(into_the_slot, &slot);

            assert!(
                after.length() < 1e-5,
                "the crease of a narrow slot is perpendicular to the motion: {after:?}"
            );
            assert_eq!(impacts.len(), 2, "{impacts:?}");
            for (index, normal) in slot.into_iter().enumerate() {
                let impact = impact_on(&impacts, index as u32);
                assert!(
                    (impact.speed + into_the_slot.dot(normal)).abs() < 1e-5,
                    "{impact:?}"
                );
            }
        }

        #[test]
        fn landing_into_a_floor_and_wall_seam_keeps_the_speed_along_the_wall() {
            let velocity = Vec3::new(3.0, -5.0, 10.0);

            let (after, impacts) = clipped(velocity, &[Vec3::Y, Vec3::NEG_Z]);

            assert!(
                (after - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-5,
                "{after:?}"
            );
            assert!((impact_on(&impacts, 0).speed - 5.0).abs() < 1e-5);
            assert!((impact_on(&impacts, 1).speed - 10.0).abs() < 1e-5);
        }

        #[test]
        fn planes_exactly_at_the_merge_threshold_stay_separate() {
            let tilted = Vec3::new((1.0f32 - 0.99 * 0.99).sqrt(), 0.0, -0.99);
            assert_eq!(Vec3::NEG_Z.dot(tilted), SAME_PLANE_DOT);

            let (_, impacts) = clipped(Vec3::new(0.0, 0.0, 10.0), &[Vec3::NEG_Z, tilted]);

            assert_eq!(impacts.len(), 2, "{impacts:?}");
        }

        #[test]
        fn a_clip_that_slides_exactly_along_another_plane_does_not_enter_it() {
            let velocity = Vec3::new(0.0, -3.0, 10.0);
            let normals = [
                Vec3::NEG_Z,
                Vec3::new(-0.6, 0.0, -0.8),
                Vec3::new(0.6, 0.0, -0.8),
            ];

            let (after, _) = clipped(velocity, &normals);

            assert_eq!(after, Vec3::new(0.0, -3.0, 0.0));
        }

        #[test]
        fn three_blocking_planes_stop_the_character() {
            let [a, b] = wedge(std::f32::consts::FRAC_PI_6);
            let ceiling = Vec3::new(0.0, -1.0, 0.0);
            let velocity = Vec3::new(0.5, 3.0, 5.0);

            let (after, impacts) = clipped(velocity, &[a, b, ceiling]);

            assert!(after.length() < 1e-5, "{after:?}");
            assert_eq!(impacts.len(), 3, "{impacts:?}");
            for (index, normal) in [a, b, ceiling].into_iter().enumerate() {
                let impact = impact_on(&impacts, index as u32);
                assert!(
                    (impact.speed + velocity.dot(normal)).abs() < 1e-5,
                    "{impact:?}"
                );
            }
        }

        #[test]
        fn repeated_hits_on_one_plane_report_one_impact() {
            let wall = Vec3::new(0.0, 0.0, -1.0);

            let (after, impacts) = clipped(Vec3::new(3.0, 0.0, 10.0), &[wall; 20]);

            assert_eq!(impacts.len(), 1, "{impacts:?}");
            assert!((impacts[0].speed - 10.0).abs() < 1e-5, "{impacts:?}");
            assert!(
                (after - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-5,
                "merged before the plane cap, so the wall clips instead of stopping: {after:?}"
            );
        }

        /// Three planes facing `Vec3::new(3.0, 0.0, 10.0)`, clipping against the first of which
        /// leaves a velocity that enters neither of the others.
        fn clearing_planes() -> [Vec3; 3] {
            [
                Vec3::new(0.0, 0.0, -1.0),
                Vec3::new(0.6, 0.0, -0.8),
                Vec3::new(0.28, -0.6, -0.75).normalize(),
            ]
        }

        #[test]
        fn a_single_clip_that_clears_the_other_planes_wins_over_stopping() {
            let velocity = Vec3::new(3.0, 0.0, 10.0);
            let planes = clearing_planes();

            let (after, impacts) = clipped(velocity, &planes);

            assert!(
                (after - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-5,
                "{after:?}"
            );
            assert_eq!(impacts.len(), 3, "{impacts:?}");
            for (index, normal) in planes.into_iter().enumerate() {
                let impact = impact_on(&impacts, index as u32);
                assert_eq!(impact.normal, normal);
                assert!(
                    (impact.speed + velocity.dot(normal)).abs() < 1e-5,
                    "{impact:?}"
                );
            }
        }

        #[test]
        fn more_than_five_planes_stop_the_character() {
            let velocity = Vec3::new(3.0, 0.0, 10.0);
            let planes = [
                clearing_planes().as_slice(),
                &[
                    Vec3::new(0.3, 0.5, -0.8).normalize(),
                    Vec3::new(0.1, -0.3, -0.9).normalize(),
                    Vec3::new(0.5, 0.4, -0.7).normalize(),
                ],
            ]
            .concat();

            let (after, impacts) = clipped(velocity, &planes);

            assert!(after.length() < 1e-5, "{after:?}");
            assert_eq!(impacts.len(), MAX_CLIP_PLANES, "{impacts:?}");
            for impact in &impacts {
                let index = impact.collider.into_raw_parts().0 as usize;
                assert!(
                    (impact.speed + velocity.dot(planes[index])).abs() < 1e-5,
                    "any reported impact is measured from the incoming velocity: {impact:?}"
                );
            }
        }

        #[test]
        fn three_planes_without_a_single_clip_stop_in_any_order() {
            let velocity = Vec3::new(-3.405, 5.769, -3.936);
            let planes = [
                Vec3::new(-0.122, -0.959, -0.254).normalize(),
                Vec3::new(-0.652, -0.522, 0.55).normalize(),
                Vec3::new(-0.415, -0.028, 0.909).normalize(),
            ];
            let orders = [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ];

            for order in orders {
                let ordered = order.map(|index| planes[index]);

                let (after, _) = clipped(velocity, &ordered);

                assert!(after.length() < 1e-5, "order {order:?}: {after:?}");
            }
        }

        #[test]
        fn near_parallel_planes_merge_into_the_first_one() {
            let tilted = [0.0f32, 0.035, -0.035, 0.017, -0.017]
                .map(|angle| Vec3::new(angle.sin(), 0.0, -angle.cos()));

            let (after, impacts) = clipped(Vec3::new(3.0, 0.0, 10.0), &tilted);

            assert_eq!(impacts.len(), 1, "{impacts:?}");
            assert_eq!(impacts[0].collider, handle(0));
            assert_eq!(impacts[0].normal, tilted[0]);
            assert!((impacts[0].speed - 10.0).abs() < 1e-5, "{impacts:?}");
            assert!(
                (after - Vec3::new(3.0, 0.0, 0.0)).length() < 1e-5,
                "{after:?}"
            );
        }

        fn sorted(mut impacts: Vec<Impact>) -> Vec<Impact> {
            impacts.sort_by_key(|impact| impact.collider.into_raw_parts());
            impacts
        }

        proptest! {
            #[test]
            fn plane_order_does_not_change_the_result(
                velocity in (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0),
                planes in prop::collection::vec(
                    ((-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0), any::<u32>()),
                    2..=4,
                ),
            ) {
                let velocity = Vec3::new(velocity.0, velocity.1, velocity.2);
                prop_assume!(velocity.length() > 1.0);
                let mut keyed = Vec::new();
                for (index, ((x, y, z), key)) in planes.into_iter().enumerate() {
                    let raw = Vec3::new(x, y, z);
                    prop_assume!(raw.length() > 0.3);
                    let facing = if raw.dot(velocity) > 0.0 { -raw } else { raw }.normalize();
                    prop_assume!(facing.dot(velocity) < -0.1 * velocity.length());
                    keyed.push((handle(index as u32), facing, key));
                }
                for (i, first) in keyed.iter().enumerate() {
                    for second in &keyed[i + 1..] {
                        prop_assume!(first.1.dot(second.1).abs() < 0.98);
                    }
                }
                let mut shuffled = keyed.clone();
                shuffled.sort_by_key(|&(_, _, key)| key);
                let run = |planes: &[(ColliderHandle, Vec3, u32)]| {
                    let slid = Slide {
                        translation: Vec3::ZERO,
                        grounded: false,
                        ground: None,
                        hits: planes.iter().map(|&(c, n, _)| (c, n)).collect(),
                    };
                    let mut after = velocity;
                    let impacts = super::super::clip(&mut after, velocity * DT, &slid, false, false);
                    (after, sorted(impacts))
                };

                let (in_order, in_order_impacts) = run(&keyed);
                let (permuted, permuted_impacts) = run(&shuffled);

                prop_assert!(
                    (in_order - permuted).length() < 1e-5,
                    "{in_order:?} vs {permuted:?}"
                );
                prop_assert_eq!(in_order_impacts.len(), permuted_impacts.len());
                for (x, y) in in_order_impacts.iter().zip(&permuted_impacts) {
                    prop_assert_eq!(x.collider, y.collider);
                    prop_assert_eq!(x.normal, y.normal);
                    prop_assert!((x.speed - y.speed).abs() < 1e-5, "{x:?} vs {y:?}");
                }
            }
        }
    }

    mod clamp {
        use super::*;

        const MAX: f32 = 90.0;

        /// One airborne step in open air under `effects`: the motion the slide was asked for
        /// and the state afterwards.
        fn open_air_step(effects: &[CharacterEffect]) -> (Vec3, CharacterState) {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::ZERO, false);
            let mut asked_of = None;

            step(
                &mut state,
                &CharacterCommand::default(),
                effects,
                &test_tuning(),
                DT,
                &Fake::new(|_, _, asked| {
                    asked_of = Some(asked);
                    open_air(asked)
                }),
            );

            (asked_of.expect("the step slid"), state)
        }

        #[test]
        fn a_1000_m_s_set_velocity_slides_at_max_velocity() {
            let set = CharacterEffect::SetVelocity(Vec3::new(1000.0, -1000.0, 60.0));

            let (asked, _) = open_air_step(&[set]);

            assert_eq!(asked.x, MAX * DT, "clamped to the maximum: {asked:?}");
            assert_eq!(
                asked.y,
                -MAX * DT,
                "clamped after the half tick of gravity, not before: {asked:?}"
            );
            assert_eq!(
                asked.z,
                60.0 * DT,
                "within the maximum, unchanged: {asked:?}"
            );
        }

        #[test]
        fn a_non_finite_velocity_component_resets_to_zero() {
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let set = CharacterEffect::SetVelocity(Vec3::new(bad, 0.0, 3.0));

                let (asked, after) = open_air_step(&[set]);

                assert_eq!(asked.x, 0.0, "{bad}: {asked:?}");
                assert_eq!(after.velocity.x, 0.0, "{bad}: {:?}", after.velocity);
                assert!(
                    (after.velocity.z - 3.0).abs() < 1e-6,
                    "{bad}: {:?}",
                    after.velocity
                );
                assert!(after.position.is_finite(), "{bad}: {:?}", after.position);
            }
        }

        #[test]
        fn a_non_finite_velocity_on_the_ground_keeps_its_finite_components() {
            let mut state = standing();
            state.velocity = Vec3::new(f32::INFINITY, 0.0, 3.0);

            step_without_effects(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                flat_floor(0.0),
            );

            assert_eq!(state.velocity.x, 0.0, "{:?}", state.velocity);
            assert!(state.velocity.z > 0.0, "{:?}", state.velocity);
        }

        proptest! {
            #[test]
            fn velocity_after_the_step_is_within_max_velocity(
                x in -1.0e6f32..1.0e6,
                y in -1.0e6f32..1.0e6,
                z in -1.0e6f32..1.0e6,
            ) {
                let set = CharacterEffect::SetVelocity(Vec3::new(x, y, z));

                let (asked, after) = open_air_step(&[set]);

                let v = after.velocity;
                prop_assert!(
                    v.abs().max_element() <= MAX + 1e-4,
                    "stored {v:?} from {x}, {y}, {z}"
                );
                prop_assert!(
                    (asked / DT).abs().max_element() <= MAX + 1e-3,
                    "asked {asked:?}"
                );
            }
        }
    }

    mod effects {
        use super::*;

        /// One step on a flat floor at y = 0 that slides from wherever the step starts it.
        fn on_floor(
            state: &mut CharacterState,
            command: &CharacterCommand,
            effects: &[CharacterEffect],
        ) -> Vec<Impact> {
            step(
                state,
                command,
                effects,
                &test_tuning(),
                DT,
                &Fake::new(|from, _, asked| flat_floor(from.position.y)(asked)),
            )
        }

        /// The motion the step asked for and the state the slide started from.
        fn ask_in_open_air(
            state: &mut CharacterState,
            effects: &[CharacterEffect],
        ) -> (Vec3, CharacterState) {
            ask_in_open_air_under(state, &CharacterCommand::default(), effects)
        }

        fn ask_in_open_air_under(
            state: &mut CharacterState,
            command: &CharacterCommand,
            effects: &[CharacterEffect],
        ) -> (Vec3, CharacterState) {
            let mut seen = None;
            step(
                state,
                command,
                effects,
                &test_tuning(),
                DT,
                &Fake::new(|from, _, asked| {
                    seen = Some((asked, *from));
                    open_air(asked)
                }),
            );
            seen.expect("the step slid")
        }

        #[test]
        fn add_velocity_applies_before_gravity_and_the_move() {
            let settings = test_settings();
            let mut walker = standing();
            let mut lifted = standing();

            on_floor(
                &mut walker,
                &CharacterCommand::default(),
                &[CharacterEffect::AddVelocity(Vec3::new(3.0, 0.0, 0.0))],
            );
            let (asked, from) = ask_in_open_air(
                &mut lifted,
                &[CharacterEffect::AddVelocity(Vec3::new(0.0, 4.0, 0.0))],
            );

            let after_friction = 3.0 * (1.0 - settings.friction * DT);
            assert!(
                (walker.velocity.x - after_friction).abs() < 1e-4,
                "friction acts on the added speed: {:?}",
                walker.velocity
            );
            assert!(
                (walker.position.x - after_friction * DT).abs() < 1e-5,
                "{:?}",
                walker.position
            );
            assert!(
                (asked.y - (4.0 - settings.gravity.expect("test settings set gravity") * DT / 2.0) * DT).abs() < 1e-6,
                "a grounded character given upward speed runs airborne, so half a tick of gravity acts: asked y = {}",
                asked.y
            );
            assert!(!lifted.grounded, "{lifted:?}");
            assert!(from.velocity.y > 0.0, "{from:?}");
        }

        #[test]
        fn add_velocity_then_teleport_ends_at_the_target_at_rest() {
            let target = Vec3::new(10.0, 0.0, -3.0);
            let mut state = standing();
            let mut started_from = None;
            let effects = [
                CharacterEffect::AddVelocity(Vec3::new(5.0, 0.0, 0.0)),
                CharacterEffect::Teleport {
                    position: target,
                    keep_velocity: false,
                },
            ];

            step(
                &mut state,
                &CharacterCommand::default(),
                &effects,
                &test_tuning(),
                DT,
                &Fake::new(|from, _, asked| {
                    started_from = Some(*from);
                    flat_floor(from.position.y)(asked)
                }),
            );

            let from = started_from.expect("the step slid");
            assert_eq!(from.position, target, "the slide starts at the teleport");
            assert_eq!(state.position, target);
            assert_eq!(state.velocity, Vec3::ZERO);
            assert!(state.grounded, "{state:?}");
        }

        #[test]
        fn set_velocity_replaces_it() {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::new(1.0, 2.0, 3.0), false);

            let (asked, _) = ask_in_open_air(
                &mut state,
                &[CharacterEffect::SetVelocity(Vec3::new(0.0, 0.0, 6.0))],
            );

            assert!(
                (asked - Vec3::new(0.0, -18.0 * DT / 2.0 * DT, 6.0 * DT)).length() < 1e-6,
                "{asked:?}"
            );
            assert!(
                (state.velocity - Vec3::new(0.0, -18.0 * DT, 6.0)).length() < 1e-5,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn teleport_keeping_velocity_keeps_it() {
            let target = Vec3::new(7.0, 60.0, 8.0);
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::new(2.0, 0.0, 3.0), false);

            let (asked, from) = ask_in_open_air(
                &mut state,
                &[CharacterEffect::Teleport {
                    position: target,
                    keep_velocity: true,
                }],
            );

            assert_eq!(from.position, target);
            assert!(
                (asked - Vec3::new(2.0 * DT, -18.0 * DT / 2.0 * DT, 3.0 * DT)).length() < 1e-6,
                "{asked:?}"
            );
            assert!(
                (state.position - (target + asked)).length() < 1e-5,
                "{:?}",
                state.position
            );
            assert!(
                (state.velocity - Vec3::new(2.0, -18.0 * DT, 3.0)).length() < 1e-5,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn teleport_clears_grounded() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), true);
            let jump = CharacterCommand {
                jump: true,
                ..CharacterCommand::default()
            };

            let (asked, from) = ask_in_open_air_under(
                &mut state,
                &jump,
                &[CharacterEffect::Teleport {
                    position: Vec3::new(5.0, 40.0, 5.0),
                    keep_velocity: true,
                }],
            );

            assert!(
                !from.grounded,
                "the old ground says nothing about the new place"
            );
            assert!(
                (asked.y + 18.0 * DT / 2.0 * DT).abs() < 1e-6,
                "the tick runs airborne: gravity acts and the jump does not launch: asked y = {}",
                asked.y
            );
            assert!(!state.grounded, "{state:?}");
        }

        #[test]
        fn set_look_replaces_the_look_before_the_commands_turn() {
            let mut state = standing();
            state.yaw = 1.0;
            state.pitch = 0.5;
            let command = CharacterCommand {
                forward: 1.0,
                yaw_delta: 0.5,
                pitch_delta: 0.25,
                ..CharacterCommand::default()
            };

            on_floor(
                &mut state,
                &command,
                &[CharacterEffect::SetLook {
                    yaw: 0.25,
                    pitch: -0.5,
                }],
            );

            assert!((state.yaw - 0.75).abs() < 1e-6, "{}", state.yaw);
            assert!((state.pitch + 0.25).abs() < 1e-6, "{}", state.pitch);
            assert!(
                (state.velocity.x.atan2(state.velocity.z) - 0.75).abs() < 1e-4,
                "the move faces the turned look: {:?}",
                state.velocity
            );
        }

        #[test]
        fn two_teleports_in_one_tick_end_at_the_second() {
            let first = Vec3::new(1.0, 0.0, 1.0);
            let second = Vec3::new(7.0, 0.0, -2.0);
            let mut state = state_at(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0), true);
            let mut started_from = None;
            let effects = [
                CharacterEffect::Teleport {
                    position: first,
                    keep_velocity: false,
                },
                CharacterEffect::Teleport {
                    position: second,
                    keep_velocity: false,
                },
            ];

            step(
                &mut state,
                &CharacterCommand::default(),
                &effects,
                &test_tuning(),
                DT,
                &Fake::new(|from, _, asked| {
                    started_from = Some(*from);
                    flat_floor(from.position.y)(asked)
                }),
            );

            let from = started_from.expect("the step slid");
            assert_eq!(from.position, second);
            assert_eq!(from.velocity.x, 0.0);
            assert_eq!(from.velocity.z, 0.0);
            assert_eq!(state.position, second);
            assert_eq!(state.velocity, Vec3::ZERO);
        }

        #[test]
        fn teleport_keeping_velocity_on_a_grounded_walker_runs_that_tick_airborne() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(5.0, 0.0, 0.0), true);

            let (asked, _) = ask_in_open_air(
                &mut state,
                &[CharacterEffect::Teleport {
                    position: Vec3::new(0.0, 40.0, 0.0),
                    keep_velocity: true,
                }],
            );

            assert!(
                (asked.x - 5.0 * DT).abs() < 1e-6,
                "no ground friction: asked x = {}",
                asked.x
            );
            assert!(
                (asked.y + 18.0 * DT / 2.0 * DT).abs() < 1e-6,
                "half a tick of gravity: asked y = {}",
                asked.y
            );
            assert!(
                (state.velocity.x - 5.0).abs() < 1e-5,
                "{:?}",
                state.velocity
            );
        }

        #[test]
        fn set_look_clamps_pitch() {
            let mut up = standing();
            let mut down = standing();
            let back_down = CharacterCommand {
                pitch_delta: -0.5,
                ..CharacterCommand::default()
            };
            let back_up = CharacterCommand {
                pitch_delta: 0.5,
                ..CharacterCommand::default()
            };

            on_floor(
                &mut up,
                &back_down,
                &[CharacterEffect::SetLook {
                    yaw: 0.0,
                    pitch: 3.0,
                }],
            );
            on_floor(
                &mut down,
                &back_up,
                &[CharacterEffect::SetLook {
                    yaw: 0.0,
                    pitch: -3.0,
                }],
            );

            assert!(
                (up.pitch - (PITCH_LIMIT - 0.5)).abs() < 1e-6,
                "{}",
                up.pitch
            );
            assert!(
                (down.pitch - (0.5 - PITCH_LIMIT)).abs() < 1e-6,
                "{}",
                down.pitch
            );
        }
    }

    mod stance {
        use super::*;
        use crate::world::test_support::{crouching_body, prone_body};
        use std::cell::{Cell, RefCell};

        /// Half the standing hull (1.83 m) and half the crouched one (1.1 m).
        const STAND_HALF: f32 = 0.915;
        const CROUCH_HALF: f32 = 0.55;
        const STAND_EYE: f32 = 1.63;
        const CROUCH_EYE: f32 = 0.95;
        const EPS: f32 = 1e-4;

        fn tuning(body: CharacterBody, crouch_ticks: u16, gravity: f32) -> Tuning {
            Tuning {
                body,
                settings: MoveSettings {
                    crouch_ticks,
                    ..test_settings()
                },
                gravity,
            }
        }

        fn on_the_floor() -> CharacterState {
            state_at(Vec3::new(0.0, STAND_HALF, 0.0), Vec3::ZERO, true)
        }

        fn in_the_air() -> CharacterState {
            state_at(Vec3::new(0.0, 10.0, 0.0), Vec3::ZERO, false)
        }

        fn grounded_slide(asked: Vec3) -> Slide {
            Slide {
                translation: asked.with_y(0.0),
                grounded: true,
                ground: Some(face(Vec3::Y, 0.0)),
                hits: Vec::new(),
            }
        }

        /// One step wanting `wanted` over open space (floor under a grounded character).
        /// Returns the hull the slide was given.
        fn tick_fitting(
            state: &mut CharacterState,
            wanted: Stance,
            tuning: &Tuning,
            fits: impl Fn(&Hull, Vec3, f32) -> bool,
        ) -> Hull {
            let hull_of = |pose: HullPose| match pose {
                HullPose::Upright(hull) => hull,
                HullPose::Lying { radius, length } => Hull {
                    shape: HullShape::Capsule,
                    radius,
                    length,
                },
            };
            let pose = tick_posed(state, wanted, tuning, move |pose, centre, yaw| {
                fits(&hull_of(pose), centre, yaw)
            });
            hull_of(pose)
        }

        /// [`tick_fitting`] with `fits` seeing the pose; returns the pose the slide was given.
        fn tick_posed(
            state: &mut CharacterState,
            wanted: Stance,
            tuning: &Tuning,
            fits: impl Fn(HullPose, Vec3, f32) -> bool,
        ) -> HullPose {
            let seen = Cell::new(None);
            let queries = Fake::new(|from: &CharacterState, pose: HullPose, asked| {
                seen.set(Some(pose));
                if from.grounded {
                    grounded_slide(asked)
                } else {
                    open_air(asked)
                }
            })
            .fitting(fits);
            let command = CharacterCommand {
                stance: wanted,
                ..CharacterCommand::default()
            };

            super::super::step(state, &command, &[], tuning, DT, &queries);

            seen.get().expect("the step slid")
        }

        fn tick(state: &mut CharacterState, wanted: Stance, tuning: &Tuning) -> Hull {
            tick_fitting(state, wanted, tuning, |_, _, _| true)
        }

        /// The eye's height above the feet of the hull in use.
        fn eye_above_feet(state: &CharacterState, body: &CharacterBody) -> f32 {
            let hull = match state.stance {
                Stance::Stand => body.standing.hull,
                Stance::Crouch => body.crouched.expect("the body crouches").hull,
                Stance::Prone => body.prone.expect("the body goes prone").hull,
            };
            state.eye.y + hull.length / 2.0
        }

        /// The eye height `down` of the way from standing to crouched.
        fn eye_down(down: f32) -> f32 {
            STAND_EYE + (CROUCH_EYE - STAND_EYE) * down
        }

        #[test]
        fn the_eye_lowers_each_tick_over_the_crouch_time() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();

            let mut eyes = Vec::new();
            for _ in 0..8 {
                tick(&mut state, Stance::Crouch, &tuning);
                eyes.push(eye_above_feet(&state, &body));
            }
            for _ in 0..4 {
                tick(&mut state, Stance::Crouch, &tuning);
                eyes.push(eye_above_feet(&state, &body));
            }

            for (i, eye) in eyes.iter().take(8).enumerate() {
                let expected = eye_down((i + 1) as f32 / 8.0);
                assert!(
                    (eye - expected).abs() < EPS,
                    "tick {}: eye {eye}, expected {expected}; all {eyes:?}",
                    i + 1
                );
            }
            for eye in &eyes[8..] {
                assert!((eye - CROUCH_EYE).abs() < EPS, "it stays: {eyes:?}");
            }
            assert_eq!(state.stance, Stance::Crouch);
        }

        #[test]
        fn reversing_mid_leg_continues_from_the_current_eye() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();
            for _ in 0..3 {
                tick(&mut state, Stance::Crouch, &tuning);
            }
            let midway = eye_above_feet(&state, &body);

            let mut eyes = Vec::new();
            for _ in 0..4 {
                tick(&mut state, Stance::Stand, &tuning);
                eyes.push(eye_above_feet(&state, &body));
            }

            assert!((midway - eye_down(3.0 / 8.0)).abs() < EPS, "{midway}");
            for (i, eye) in eyes.iter().take(3).enumerate() {
                let expected = eye_down((2 - i) as f32 / 8.0);
                assert!(
                    (eye - expected).abs() < EPS,
                    "tick {}: eye {eye}, expected {expected} rising from {midway}; all {eyes:?}",
                    i + 1
                );
            }
            assert!((eyes[3] - STAND_EYE).abs() < EPS, "it stays: {eyes:?}");
            assert_eq!(state.stance, Stance::Stand);
        }

        /// Asks for `wanted` in the air from a settled `from`, returning the state before, the
        /// state after, the hull the slide saw and the centre `fits` was asked about.
        fn change_in_the_air(
            from: Stance,
            wanted: Stance,
        ) -> (CharacterState, CharacterState, Hull, Vec3) {
            let tuning = tuning(crouching_body(), 8, 0.0);
            let mut state = in_the_air();
            for _ in 0..10 {
                tick(&mut state, from, &tuning);
            }
            let before = state;
            let asked = Cell::new(Vec3::NAN);
            let fits = |_: &Hull, centre: Vec3, _: f32| {
                asked.set(centre);
                true
            };

            let seen = tick_fitting(&mut state, wanted, &tuning, fits);

            (before, state, seen, asked.get())
        }

        #[test]
        fn crouching_in_the_air_keeps_the_top_of_the_hull() {
            let (before, after, seen, asked) = change_in_the_air(Stance::Stand, Stance::Crouch);

            let top = before.position.y + STAND_HALF;
            assert_eq!(seen.length, 1.1, "the slide sees the crouched hull");
            assert_eq!(after.stance, Stance::Crouch);
            assert!(
                (after.position.y + CROUCH_HALF - top).abs() < EPS,
                "{after:?}"
            );
            assert!(
                after.position.y - CROUCH_HALF > before.position.y - STAND_HALF + 0.7,
                "the feet did not tuck up: {after:?}"
            );
            assert!(
                (asked.y + CROUCH_HALF - top).abs() < EPS,
                "fits was asked about {asked:?}, not the top-anchored centre"
            );
            assert!(!after.grounded);
        }

        #[test]
        fn standing_in_the_air_keeps_the_top_of_the_hull() {
            let (before, after, seen, asked) = change_in_the_air(Stance::Crouch, Stance::Stand);

            let top = before.position.y + CROUCH_HALF;
            assert_eq!(seen.length, 1.83, "the slide sees the standing hull");
            assert_eq!(after.stance, Stance::Stand);
            assert!(
                (after.position.y + STAND_HALF - top).abs() < EPS,
                "{after:?}"
            );
            assert!(
                after.position.y - STAND_HALF < before.position.y - CROUCH_HALF - 0.7,
                "the feet did not drop: {after:?}"
            );
            assert!(
                (asked.y + STAND_HALF - top).abs() < EPS,
                "fits was asked about {asked:?}, not the top-anchored centre"
            );
        }

        /// The world eye of a character in the air under no gravity, one tick at a time, from
        /// the settled `from` stance through a leg toward `wanted`, plus the tick before it.
        fn world_eyes_through_an_airborne_leg(from: Stance, wanted: Stance) -> Vec<f32> {
            let tuning = tuning(crouching_body(), 8, 0.0);
            let mut state = in_the_air();
            for _ in 0..10 {
                tick(&mut state, from, &tuning);
            }
            let mut eyes = vec![state.eye_y_in_the_world()];
            for _ in 0..10 {
                tick(&mut state, wanted, &tuning);
                eyes.push(state.eye_y_in_the_world());
            }
            eyes
        }

        trait WorldEye {
            fn eye_y_in_the_world(&self) -> f32;
        }

        impl WorldEye for CharacterState {
            fn eye_y_in_the_world(&self) -> f32 {
                self.position.y + self.eye.y
            }
        }

        /// The eye's depth below the top of the hull: 0.2 m standing, 0.15 m crouched.
        const STAND_DEPTH: f32 = 0.2;
        const CROUCH_DEPTH: f32 = 0.15;

        #[test]
        fn crouching_in_the_air_keeps_the_eye_where_it_was() {
            let eyes = world_eyes_through_an_airborne_leg(Stance::Stand, Stance::Crouch);

            let step = (STAND_DEPTH - CROUCH_DEPTH) / 8.0;
            assert!(
                (eyes[1] - eyes[0]).abs() <= step + 1e-5,
                "the eye popped on the start tick: {eyes:?}"
            );
            for pair in eyes.windows(2) {
                assert!(pair[1] >= pair[0] - 1e-5, "monotonic: {eyes:?}");
            }
            let settled = eyes[0] - (CROUCH_DEPTH - STAND_DEPTH);
            assert!((eyes[10] - settled).abs() < EPS, "{eyes:?}");
        }

        #[test]
        fn standing_in_the_air_keeps_the_eye_where_it_was() {
            let eyes = world_eyes_through_an_airborne_leg(Stance::Crouch, Stance::Stand);

            let step = (STAND_DEPTH - CROUCH_DEPTH) / 8.0;
            assert!(
                (eyes[1] - eyes[0]).abs() <= step + 1e-5,
                "the eye popped on the start tick: {eyes:?}"
            );
            for pair in eyes.windows(2) {
                assert!(pair[1] <= pair[0] + 1e-5, "monotonic: {eyes:?}");
            }
            let settled = eyes[0] - (STAND_DEPTH - CROUCH_DEPTH);
            assert!((eyes[10] - settled).abs() < EPS, "{eyes:?}");
        }

        #[test]
        fn a_settled_crouch_stays_settled_when_the_leg_gets_longer() {
            let body = crouching_body();
            let mut state = on_the_floor();
            let eight = tuning(body, 8, 18.0);
            for _ in 0..10 {
                tick(&mut state, Stance::Crouch, &eight);
            }
            let settled_eye = state.eye;
            let twelve = tuning(body, 12, 18.0);

            tick(&mut state, Stance::Crouch, &twelve);
            let after_the_change = state.eye;
            let mut eyes = Vec::new();
            for _ in 0..3 {
                tick(&mut state, Stance::Stand, &twelve);
                eyes.push(eye_above_feet(&state, &body));
            }

            assert_eq!(after_the_change, settled_eye);
            for (i, eye) in eyes.iter().enumerate() {
                let expected = CROUCH_EYE + (STAND_EYE - CROUCH_EYE) * (i + 1) as f32 / 12.0;
                assert!(
                    (eye - expected).abs() < EPS,
                    "a fresh 12 tick leg, tick {}: {eye}, expected {expected}",
                    i + 1
                );
            }
        }

        #[test]
        fn a_leg_cut_short_by_a_shorter_crouch_time_reverses_without_underflow() {
            let body = crouching_body();
            let mut state = on_the_floor();
            for _ in 0..9 {
                tick(&mut state, Stance::Crouch, &tuning(body, 12, 18.0));
            }

            tick(&mut state, Stance::Stand, &tuning(body, 4, 18.0));

            let first_quarter = CROUCH_EYE + (STAND_EYE - CROUCH_EYE) / 4.0;
            assert_eq!(state.stance, Stance::Stand);
            assert!((eye_above_feet(&state, &body) - first_quarter).abs() < EPS);
        }

        #[test]
        fn reversing_a_feet_anchored_leg_in_the_air_keeps_the_eye_continuous() {
            let tuning = tuning(crouching_body(), 8, 0.0);
            let mut state = on_the_floor();
            for _ in 0..3 {
                tick(&mut state, Stance::Crouch, &tuning);
            }
            let queries = Fake::new(|_: &CharacterState, _: HullPose, asked| grounded_slide(asked));
            let jump = CharacterCommand {
                jump: true,
                stance: Stance::Crouch,
                ..CharacterCommand::default()
            };
            super::super::step(&mut state, &jump, &[], &tuning, DT, &queries);
            state.velocity = Vec3::ZERO;
            assert!(!state.grounded, "the jump left the floor");

            let step = (STAND_EYE - CROUCH_EYE) / 8.0;
            let mut previous = state.eye_y_in_the_world();
            for _ in 0..8 {
                tick(&mut state, Stance::Stand, &tuning);
                let eye = state.eye_y_in_the_world();
                assert!(
                    (eye - previous).abs() <= step + 1e-4,
                    "world eye jumped {previous} -> {eye}"
                );
                previous = eye;
            }
            assert_eq!(state.stance, Stance::Stand);
        }

        #[test]
        fn standing_on_the_ground_keeps_the_feet() {
            let tuning = tuning(crouching_body(), 8, 18.0);
            let mut state = on_the_floor();
            for _ in 0..10 {
                tick(&mut state, Stance::Crouch, &tuning);
            }

            tick(&mut state, Stance::Stand, &tuning);

            assert_eq!(state.stance, Stance::Stand);
            assert!(
                (state.position.y - STAND_HALF).abs() < EPS,
                "the feet left the floor: {state:?}"
            );
            assert!(state.grounded);
        }

        #[test]
        fn crouching_on_the_ground_keeps_the_feet() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();

            tick(&mut state, Stance::Crouch, &tuning);

            assert_eq!(state.stance, Stance::Crouch);
            assert!(
                (state.position.y - CROUCH_HALF).abs() < EPS,
                "the feet left the floor: {state:?}"
            );
            assert!(state.grounded);
        }

        #[test]
        fn a_body_without_a_crouch_never_crouches() {
            let body = CharacterBody {
                crouched: None,
                ..crouching_body()
            };
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();
            let start = state;

            let mut seen = Vec::new();
            for _ in 0..12 {
                seen.push(tick(&mut state, Stance::Crouch, &tuning));
            }

            assert!(seen.iter().all(|hull| hull.length == 1.83), "{seen:?}");
            assert_eq!(state.stance, Stance::Stand);
            assert_eq!(state.from, Stance::Stand);
            assert_eq!(state.position, start.position);
            assert!((state.eye.y + STAND_HALF - STAND_EYE).abs() < EPS);
        }

        #[test]
        fn zero_crouch_ticks_changes_in_one_tick() {
            let body = crouching_body();
            let tuning = tuning(body, 0, 18.0);
            let mut state = on_the_floor();

            tick(&mut state, Stance::Crouch, &tuning);
            let crouched = eye_above_feet(&state, &body);
            let crouched_stance = state.stance;
            tick(&mut state, Stance::Stand, &tuning);
            let stood = eye_above_feet(&state, &body);

            assert_eq!(crouched_stance, Stance::Crouch);
            assert!((crouched - CROUCH_EYE).abs() < EPS, "{crouched}");
            assert_eq!(state.stance, Stance::Stand);
            assert!((stood - STAND_EYE).abs() < EPS, "{stood}");
        }

        #[test]
        fn the_slide_sees_the_crouched_hull_on_the_tick_the_crouch_starts() {
            let tuning = tuning(crouching_body(), 8, 18.0);
            let mut state = on_the_floor();

            let first = tick(&mut state, Stance::Crouch, &tuning);

            assert_eq!(first.length, 1.1);
            assert_eq!(first.radius, 0.38);
        }

        #[test]
        fn a_stand_that_does_not_fit_keeps_the_crouch_and_retries_every_tick() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();
            for _ in 0..10 {
                tick(&mut state, Stance::Crouch, &tuning);
            }
            let crouched = state;
            let asked = RefCell::new(Vec::new());
            let fits = |hull: &Hull, centre: Vec3, yaw: f32| {
                asked.borrow_mut().push((*hull, centre, yaw));
                hull.length < 1.5
            };

            let mut seen = Vec::new();
            for _ in 0..3 {
                seen.push(tick_fitting(&mut state, Stance::Stand, &tuning, fits));
            }

            assert!(seen.iter().all(|hull| hull.length == 1.1), "{seen:?}");
            assert_eq!(state.stance, Stance::Crouch);
            assert_eq!(state.position, crouched.position);
            assert!((eye_above_feet(&state, &body) - CROUCH_EYE).abs() < EPS);
            let asked = asked.into_inner();
            assert!(!asked.is_empty(), "it never asked");
            for (hull, centre, _) in &asked {
                assert_eq!(hull.length, 1.83, "it asks about the standing hull");
                assert!(
                    (centre.y - STAND_HALF).abs() < EPS,
                    "it asks at the feet-anchored centre: {centre:?}"
                );
            }
        }

        #[test]
        fn a_refused_reversal_leaves_the_crouch_leg_running() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();
            for _ in 0..3 {
                tick(&mut state, Stance::Crouch, &tuning);
            }
            let standing_hull_is_refused = |hull: &Hull, _: Vec3, _: f32| hull.length < 1.5;

            let mut eyes = Vec::new();
            let mut seen = Vec::new();
            for _ in 0..3 {
                seen.push(tick_fitting(
                    &mut state,
                    Stance::Stand,
                    &tuning,
                    standing_hull_is_refused,
                ));
                eyes.push(eye_above_feet(&state, &body));
            }

            assert_eq!(state.stance, Stance::Crouch);
            assert!(seen.iter().all(|hull| hull.length == 1.1), "{seen:?}");
            for (i, eye) in eyes.iter().enumerate() {
                let expected = eye_down((i + 4) as f32 / 8.0);
                assert!(
                    (eye - expected).abs() < EPS,
                    "tick {}: eye {eye}, expected {expected}; all {eyes:?}",
                    i + 4
                );
            }
        }

        #[test]
        fn a_refused_stand_stands_on_the_first_tick_it_fits() {
            let body = crouching_body();
            let tuning = tuning(body, 8, 18.0);
            let mut state = on_the_floor();
            for _ in 0..10 {
                tick(&mut state, Stance::Crouch, &tuning);
            }
            let room = Cell::new(false);
            let fits = |_: &Hull, _: Vec3, _: f32| room.get();

            tick_fitting(&mut state, Stance::Stand, &tuning, fits);
            let while_blocked = state.stance;
            room.set(true);
            let seen = tick_fitting(&mut state, Stance::Stand, &tuning, fits);

            assert_eq!(while_blocked, Stance::Crouch);
            assert_eq!(state.stance, Stance::Stand);
            assert_eq!(seen.length, 1.83, "the slide sees the standing hull");
            assert!(
                (state.position.y - STAND_HALF).abs() < EPS,
                "the feet stayed on the floor: {state:?}"
            );
        }

        proptest! {
            #[test]
            fn the_world_eye_never_jumps_more_than_one_lerp_step_in_the_air(
                wants in prop::collection::vec(any::<bool>(), 0..80),
                crouch_ticks in 1u16..=30,
            ) {
                let tuning = tuning(crouching_body(), crouch_ticks, 0.0);
                let mut state = in_the_air();
                let step = (STAND_DEPTH - CROUCH_DEPTH) / f32::from(crouch_ticks);

                let mut previous = state.position.y + STAND_HALF - STAND_DEPTH;
                for crouch in wants {
                    let wanted = if crouch { Stance::Crouch } else { Stance::Stand };
                    tick(&mut state, wanted, &tuning);
                    let eye = state.eye_y_in_the_world();
                    prop_assert!(
                        (eye - previous).abs() <= step + 1e-5,
                        "world eye jumped {previous} -> {eye}, at most {step} a tick"
                    );
                    previous = eye;
                }
            }
        }

        proptest! {
            #[test]
            fn eye_stays_between_the_two_statures(
                wants in prop::collection::vec(any::<bool>(), 0..80),
                crouch_ticks in 1u16..=30,
            ) {
                let body = crouching_body();
                let tuning = tuning(body, crouch_ticks, 18.0);
                let mut state = on_the_floor();
                let per_tick = (STAND_EYE - CROUCH_EYE) / f32::from(crouch_ticks);

                let mut previous = STAND_EYE;
                for crouch in wants {
                    let wanted = if crouch { Stance::Crouch } else { Stance::Stand };
                    tick(&mut state, wanted, &tuning);
                    let eye = eye_above_feet(&state, &body);
                    prop_assert!(
                        (CROUCH_EYE - EPS..=STAND_EYE + EPS).contains(&eye),
                        "eye {eye} outside the statures"
                    );
                    prop_assert!(
                        (eye - previous).abs() <= per_tick + EPS,
                        "eye jumped {previous} -> {eye}, at most {per_tick} a tick"
                    );
                    previous = eye;
                }
                for _ in 0..crouch_ticks {
                    tick(&mut state, Stance::Crouch, &tuning);
                }

                prop_assert_eq!(state.stance, Stance::Crouch);
                prop_assert!((eye_above_feet(&state, &body) - CROUCH_EYE).abs() < EPS);
            }
        }

        /// The prone statures of [`prone_body`]: a 0.3 m radius capsule lying 1.8 m long, so its
        /// centre rides 0.3 m above the floor and the eye sits 0.6 m ahead of it.
        const PRONE_CENTRE: f32 = 0.3;
        const PRONE_EYE: f32 = 0.35;
        const PRONE_FORWARD: f32 = 0.6;

        fn prone_tuning(crouch_ticks: u16, prone_ticks: u16, gravity: f32) -> Tuning {
            Tuning {
                body: prone_body(),
                settings: MoveSettings {
                    crouch_ticks,
                    prone_ticks,
                    ..test_settings()
                },
                gravity,
            }
        }

        /// A grounded character at rest in `stance` on a floor at y = 0, its leg long settled.
        fn settled_on_the_floor(stance: Stance) -> CharacterState {
            let centre = match stance {
                Stance::Stand => STAND_HALF,
                Stance::Crouch => CROUCH_HALF,
                Stance::Prone => PRONE_CENTRE,
            };
            CharacterState {
                stance,
                from: stance,
                progress: u16::MAX,
                ..state_at(Vec3::new(0.0, centre, 0.0), Vec3::ZERO, true)
            }
        }

        /// A character in the air in `stance`, its leg long settled.
        fn settled_in_the_air(stance: Stance) -> CharacterState {
            CharacterState {
                grounded: false,
                ..settled_on_the_floor(stance)
            }
        }

        /// One step of `command`, returning the pose the slide was given.
        fn step_command(
            state: &mut CharacterState,
            command: &CharacterCommand,
            tuning: &Tuning,
        ) -> HullPose {
            step_effects(state, command, &[], tuning)
        }

        /// [`step_command`] with queued `effects`.
        fn step_effects(
            state: &mut CharacterState,
            command: &CharacterCommand,
            effects: &[CharacterEffect],
            tuning: &Tuning,
        ) -> HullPose {
            let seen = Cell::new(None);
            let queries = Fake::new(|from: &CharacterState, pose: HullPose, asked| {
                seen.set(Some(pose));
                if from.grounded {
                    grounded_slide(asked)
                } else {
                    open_air(asked)
                }
            });

            super::super::step(state, command, effects, tuning, DT, &queries);

            seen.get().expect("the step slid")
        }

        fn wanting(stance: Stance) -> CharacterCommand {
            CharacterCommand {
                stance,
                ..CharacterCommand::default()
            }
        }

        /// The eye's height above the floor at y = 0.
        fn eye_above_the_floor(state: &CharacterState) -> f32 {
            state.position.y + state.eye.y
        }

        #[test]
        fn standing_to_prone_passes_through_crouch() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = on_the_floor();

            let mut legs = Vec::new();
            let mut poses = Vec::new();
            for _ in 0..32 {
                poses.push(step_command(&mut state, &wanting(Stance::Prone), &tuning));
                legs.push((state.stance, state.from, state.progress));
            }

            for (i, &(stance, from, progress)) in legs.iter().enumerate() {
                let tick = i + 1;
                let expected = match tick {
                    1..=7 => (Stance::Crouch, Stance::Stand, tick),
                    8 => (Stance::Crouch, Stance::Crouch, 8),
                    9..=27 => (Stance::Prone, Stance::Crouch, tick - 8),
                    28 => (Stance::Prone, Stance::Prone, 20),
                    _ => (Stance::Prone, Stance::Prone, 20),
                };
                assert_eq!(
                    (stance, from, usize::from(progress)),
                    expected,
                    "tick {tick}; all {legs:?}"
                );
            }
            for (i, pose) in poses.iter().enumerate() {
                let (length, lying) = if i < 8 { (1.1, false) } else { (1.8, true) };
                assert_eq!(
                    (pose.length(), pose.is_lying()),
                    (length, lying),
                    "tick {}: the slide sees the hull of the stance in use",
                    i + 1
                );
            }
        }

        #[test]
        fn the_eye_lowers_and_slides_forward_across_the_prone_leg() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Crouch);

            let mut eyes = Vec::new();
            for _ in 0..24 {
                step_command(&mut state, &wanting(Stance::Prone), &tuning);
                eyes.push((eye_above_the_floor(&state), state.eye.z, state.eye.x));
            }

            for (i, &(height, ahead, sideways)) in eyes.iter().enumerate() {
                let done = ((i + 1) as f32 / 20.0).min(1.0);
                let expected_height = CROUCH_EYE + (PRONE_EYE - CROUCH_EYE) * done;
                assert!(
                    (height - expected_height).abs() < EPS,
                    "tick {}: eye {height}, expected {expected_height}; all {eyes:?}",
                    i + 1
                );
                assert!(
                    (ahead - PRONE_FORWARD * done).abs() < EPS,
                    "tick {}: eye {ahead} m ahead, expected {}",
                    i + 1,
                    PRONE_FORWARD * done
                );
                assert!(sideways.abs() < EPS, "facing +Z: {eyes:?}");
            }
        }

        #[test]
        fn reversing_a_prone_leg_continues_from_the_current_eye() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Crouch);
            for _ in 0..5 {
                step_command(&mut state, &wanting(Stance::Prone), &tuning);
            }
            let midway = eye_above_the_floor(&state);

            let mut eyes = Vec::new();
            for _ in 0..6 {
                step_command(&mut state, &wanting(Stance::Crouch), &tuning);
                eyes.push(eye_above_the_floor(&state));
            }

            assert!(
                (midway - (CROUCH_EYE - 0.6 * 5.0 / 20.0)).abs() < EPS,
                "{midway}"
            );
            for (i, eye) in eyes.iter().take(5).enumerate() {
                let expected = PRONE_EYE + (CROUCH_EYE - PRONE_EYE) * (16 + i) as f32 / 20.0;
                assert!(
                    (eye - expected).abs() < EPS,
                    "tick {}: eye {eye}, expected {expected} rising from {midway}; all {eyes:?}",
                    i + 1
                );
            }
            assert!((eyes[5] - CROUCH_EYE).abs() < EPS, "it stays: {eyes:?}");
            assert_eq!(state.stance, Stance::Crouch);
            assert_eq!(state.from, Stance::Crouch);
        }

        #[test]
        fn the_prone_eye_offset_turns_with_the_yaw() {
            let tuning = prone_tuning(8, 20, 18.0);

            for yaw in [
                0.0_f32,
                0.5,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
            ] {
                let mut state = CharacterState {
                    yaw,
                    ..settled_on_the_floor(Stance::Prone)
                };

                step_command(&mut state, &wanting(Stance::Prone), &tuning);

                let ahead = Vec3::new(yaw.sin(), 0.0, yaw.cos()) * PRONE_FORWARD;
                let horizontal = Vec3::new(state.eye.x, 0.0, state.eye.z);
                assert!(
                    horizontal.distance(ahead) < EPS,
                    "yaw {yaw}: eye {:?}, expected {ahead:?} ahead of the centre",
                    state.eye
                );
                assert!(
                    (eye_above_the_floor(&state) - PRONE_EYE).abs() < EPS,
                    "yaw {yaw}: {state:?}"
                );
            }
        }

        #[test]
        fn a_prone_character_cannot_jump() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);
            let jump = CharacterCommand {
                jump: true,
                ..wanting(Stance::Prone)
            };

            let mut vertical = Vec::new();
            for _ in 0..3 {
                step_command(&mut state, &jump, &tuning);
                vertical.push(state.velocity.y);
            }

            assert_eq!(vertical, vec![0.0; 3]);
            assert!(state.grounded);
        }

        #[test]
        fn a_leg_toward_prone_cannot_jump_either() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = CharacterState {
                stance: Stance::Prone,
                from: Stance::Crouch,
                progress: 5,
                ..settled_on_the_floor(Stance::Prone)
            };
            let jump = CharacterCommand {
                jump: true,
                ..wanting(Stance::Prone)
            };

            step_command(&mut state, &jump, &tuning);

            assert_eq!(state.velocity.y, 0.0);
            assert!(state.grounded);
        }

        #[test]
        fn a_crouched_character_still_jumps() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Crouch);
            let jump = CharacterCommand {
                jump: true,
                ..wanting(Stance::Crouch)
            };

            step_command(&mut state, &jump, &tuning);

            assert!(state.velocity.y > 1.0, "{state:?}");
        }

        #[test]
        fn prone_is_refused_in_the_air() {
            let tuning = prone_tuning(8, 20, 0.0);
            let mut state = settled_in_the_air(Stance::Crouch);
            let before = state;

            let mut poses = Vec::new();
            for _ in 0..30 {
                poses.push(step_command(&mut state, &wanting(Stance::Prone), &tuning));
            }

            assert_eq!(state.stance, Stance::Crouch);
            assert_eq!(state.from, Stance::Crouch);
            assert!(
                poses.iter().all(|p| p.length() == 1.1 && !p.is_lying()),
                "the slide kept the crouched hull: {poses:?}"
            );
            assert_eq!(state.position.y, before.position.y);
        }

        #[test]
        fn leaving_prone_is_refused_in_the_air() {
            let tuning = prone_tuning(8, 20, 0.0);
            let mut state = settled_in_the_air(Stance::Prone);

            let mut poses = Vec::new();
            for _ in 0..30 {
                poses.push(step_command(&mut state, &wanting(Stance::Stand), &tuning));
            }

            assert_eq!(state.stance, Stance::Prone);
            assert!(
                poses.iter().all(|p| p.length() == 1.8 && p.is_lying()),
                "the slide kept the lying hull: {poses:?}"
            );
        }

        #[test]
        fn a_standing_character_in_the_air_asking_for_prone_only_crouches() {
            let tuning = prone_tuning(8, 20, 0.0);
            let mut state = settled_in_the_air(Stance::Stand);

            for _ in 0..40 {
                step_command(&mut state, &wanting(Stance::Prone), &tuning);
            }

            assert_eq!(state.stance, Stance::Crouch);
            assert_eq!(state.from, Stance::Crouch);
        }

        #[test]
        fn the_lying_centre_rides_the_radius_above_the_feet_going_prone_on_the_ground() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Crouch);

            step_command(&mut state, &wanting(Stance::Prone), &tuning);

            assert_eq!(state.stance, Stance::Prone);
            assert!((state.position.y - PRONE_CENTRE).abs() < EPS, "{state:?}");
            assert!(state.grounded);
        }

        #[test]
        fn a_leg_out_of_prone_takes_prone_ticks_and_the_leg_to_stand_takes_crouch_ticks() {
            let tuning = prone_tuning(5, 12, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);

            let mut legs = Vec::new();
            for _ in 0..20 {
                step_command(&mut state, &wanting(Stance::Stand), &tuning);
                legs.push((state.stance, state.from));
            }

            let settled_at = |stance| {
                legs.iter()
                    .position(|&(s, from)| s == stance && from == stance)
                    .map(|i| i + 1)
            };
            assert_eq!(
                settled_at(Stance::Crouch),
                Some(12),
                "prone_ticks: {legs:?}"
            );
            assert_eq!(
                settled_at(Stance::Stand),
                Some(17),
                "then crouch_ticks: {legs:?}"
            );
        }

        #[test]
        fn a_refused_prone_leg_starts_on_the_first_tick_it_fits() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Crouch);
            let crouched = state;
            let tick = Cell::new(1);
            let target_probes = RefCell::new(Vec::new());

            let mut seen = Vec::new();
            for n in 1..=3 {
                tick.set(n);
                let fits = |pose: HullPose, centre: Vec3, _: f32| {
                    if pose.is_lying() && pose.length() == 1.8 {
                        target_probes.borrow_mut().push(centre);
                        return tick.get() >= 3;
                    }
                    true
                };
                tick_posed(&mut state, Stance::Prone, &tuning, fits);
                seen.push((state.stance, state.progress));
            }

            assert_eq!(seen[0].0, Stance::Crouch, "{seen:?}");
            assert_eq!(seen[1].0, Stance::Crouch, "{seen:?}");
            assert_eq!(seen[2], (Stance::Prone, 1), "{seen:?}");
            assert!(crouched.position.y > 0.0);
            for centre in target_probes.into_inner() {
                assert!(
                    (centre.y - PRONE_CENTRE).abs() < EPS,
                    "the lying probe is at the feet-anchored centre: {centre:?}"
                );
            }
        }

        #[test]
        fn a_refused_crouch_out_of_prone_starts_on_the_first_tick_it_fits() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);
            let tick = Cell::new(1);
            let target_probes = RefCell::new(Vec::new());

            let mut seen = Vec::new();
            for n in 1..=3 {
                tick.set(n);
                let fits = |pose: HullPose, centre: Vec3, _: f32| {
                    if !pose.is_lying() && pose.length() == 1.1 {
                        target_probes.borrow_mut().push(centre);
                        return tick.get() >= 3;
                    }
                    true
                };
                tick_posed(&mut state, Stance::Stand, &tuning, fits);
                seen.push((state.stance, state.progress));
            }

            assert_eq!(seen[0].0, Stance::Prone, "{seen:?}");
            assert_eq!(seen[1].0, Stance::Prone, "{seen:?}");
            assert_eq!(seen[2], (Stance::Crouch, 1), "{seen:?}");
            for centre in target_probes.into_inner() {
                assert!(
                    (centre.y - CROUCH_HALF).abs() < EPS,
                    "the crouch probe is at the feet-anchored centre: {centre:?}"
                );
            }
        }

        #[test]
        fn a_jump_on_a_leg_out_of_prone_is_refused_until_the_leg_settles() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);
            let press = |jump| CharacterCommand {
                jump,
                ..wanting(Stance::Crouch)
            };

            let mut during = Vec::new();
            for n in 1..=19 {
                step_command(&mut state, &press(n % 2 == 1), &tuning);
                during.push(state.velocity.y);
            }
            step_command(&mut state, &press(false), &tuning);
            let settled = (state.stance, state.from);
            step_command(&mut state, &press(true), &tuning);

            assert_eq!(during, vec![0.0; 19]);
            assert_eq!(settled, (Stance::Crouch, Stance::Crouch));
            assert!(state.velocity.y > 1.0, "it jumps once crouched: {state:?}");
        }

        #[test]
        fn the_eye_slides_back_to_the_centre_across_a_leg_out_of_prone() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);

            let mut ahead = Vec::new();
            for _ in 0..22 {
                step_command(&mut state, &wanting(Stance::Crouch), &tuning);
                ahead.push(state.eye.z);
            }

            for (i, z) in ahead.iter().enumerate() {
                let expected = PRONE_FORWARD * (1.0 - ((i + 1) as f32 / 20.0).min(1.0));
                assert!(
                    (z - expected).abs() < EPS,
                    "tick {}: {z} m ahead, expected {expected}; all {ahead:?}",
                    i + 1
                );
            }
        }

        #[test]
        fn a_body_without_a_prone_goes_no_further_than_crouch() {
            let tuning = Tuning {
                body: crouching_body(),
                ..prone_tuning(8, 20, 18.0)
            };
            let mut state = on_the_floor();

            let mut poses = Vec::new();
            for _ in 0..30 {
                poses.push(step_command(&mut state, &wanting(Stance::Prone), &tuning));
            }

            assert_eq!((state.stance, state.from), (Stance::Crouch, Stance::Crouch));
            assert!(poses.iter().all(|p| !p.is_lying()), "{poses:?}");
        }

        #[test]
        fn a_body_with_neither_crouch_nor_prone_stays_standing() {
            let body = CharacterBody {
                crouched: None,
                prone: None,
                lean: Lean::default(),
                ..crouching_body()
            };
            let tuning = Tuning {
                body,
                ..prone_tuning(8, 20, 18.0)
            };
            let mut state = on_the_floor();

            for _ in 0..30 {
                step_command(&mut state, &wanting(Stance::Prone), &tuning);
            }

            assert_eq!((state.stance, state.from), (Stance::Stand, Stance::Stand));
        }

        #[test]
        fn an_upright_character_turns_whether_or_not_the_turn_fits() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Stand);
            let command = CharacterCommand {
                yaw_delta: 0.25,
                ..CharacterCommand::default()
            };
            let queries = Fake::new(|_: &CharacterState, _: HullPose, asked| grounded_slide(asked))
                .fitting(|_, _, _| false);

            super::super::step(&mut state, &command, &[], &tuning, DT, &queries);

            assert_eq!(state.yaw, 0.25);
        }

        #[test]
        fn a_lying_character_that_does_not_turn_probes_nothing() {
            let tuning = prone_tuning(8, 20, 18.0);
            let mut state = settled_on_the_floor(Stance::Prone);
            let probes = Cell::new(0);
            let queries = Fake::new(|_: &CharacterState, _: HullPose, asked| grounded_slide(asked))
                .fitting(|_, _, _| {
                    probes.set(probes.get() + 1);
                    true
                });

            super::super::step(
                &mut state,
                &wanting(Stance::Prone),
                &[],
                &tuning,
                DT,
                &queries,
            );

            assert_eq!(probes.get(), 0, "a stance that holds still asks nothing");
        }

        #[test]
        fn a_prone_leg_running_when_the_character_leaves_the_ground_runs_on() {
            let tuning = prone_tuning(8, 20, 0.0);
            let mut state = settled_on_the_floor(Stance::Crouch);
            for _ in 0..3 {
                step_command(&mut state, &wanting(Stance::Prone), &tuning);
            }
            let knocked = CharacterEffect::Teleport {
                position: state.position,
                keep_velocity: false,
            };

            let first = step_effects(&mut state, &wanting(Stance::Prone), &[knocked], &tuning);
            assert!(!state.grounded, "setup: it is in the air");
            let mut poses = vec![first];
            for _ in 0..25 {
                poses.push(step_command(&mut state, &wanting(Stance::Prone), &tuning));
            }

            assert_eq!((state.stance, state.from), (Stance::Prone, Stance::Prone));
            assert!(
                poses.iter().all(|p| p.is_lying() && p.length() == 1.8),
                "the slide kept the lying hull through the airborne leg: {poses:?}"
            );
        }
    }

    mod lean {
        use super::*;
        use crate::world::test_support::prone_body;
        use std::cell::RefCell;
        use std::rc::Rc;

        const UPRIGHT_DISTANCE: f32 = 0.35;
        const PRONE_DISTANCE: f32 = 0.15;
        const HEAD_RADIUS: f32 = 0.1;
        const LEAN_TICKS: u16 = 6;
        const EPS: f32 = 1e-4;
        /// The standing eye's offset from the hull centre: 1.63 m up less half of 1.83 m.
        const STAND_EYE_Y: f32 = 0.715;
        const STAND_HALF: f32 = 0.915;
        const CROUCH_HALF: f32 = 0.55;

        type Sweep = Rc<dyn Fn(Vec3, Vec3, f32) -> f32>;

        fn upright_roll() -> f32 {
            12.0_f32.to_radians()
        }

        fn prone_roll() -> f32 {
            20.0_f32.to_radians()
        }

        fn leaning_body() -> CharacterBody {
            CharacterBody {
                lean: Lean {
                    upright: LeanReach {
                        distance: UPRIGHT_DISTANCE,
                        roll: upright_roll(),
                    },
                    prone: LeanReach {
                        distance: PRONE_DISTANCE,
                        roll: prone_roll(),
                    },
                    head_radius: HEAD_RADIUS,
                },
                ..prone_body()
            }
        }

        fn tuning() -> Tuning {
            Tuning {
                body: leaning_body(),
                settings: MoveSettings {
                    lean_ticks: LEAN_TICKS,
                    prone_ticks: 10,
                    ..test_settings()
                },
                gravity: 18.0,
            }
        }

        fn on_the_floor() -> CharacterState {
            state_at(Vec3::new(0.0, STAND_HALF, 0.0), Vec3::ZERO, true)
        }

        fn leaning(lean: f32) -> CharacterCommand {
            CharacterCommand {
                lean,
                ..CharacterCommand::default()
            }
        }

        fn clear() -> Sweep {
            Rc::new(clear_sweep)
        }

        /// A wall plane at world `x`; a ball of `radius` is stopped when its surface touches it,
        /// and a ball moving away from it is never stopped.
        fn wall_at(x: f32) -> Sweep {
            Rc::new(move |from: Vec3, to: Vec3, radius: f32| {
                let dx = to.x - from.x;
                let toward = x > from.x;
                if dx == 0.0 || (dx > 0.0) != toward {
                    return 1.0;
                }
                let gap = if toward {
                    x - radius - from.x
                } else {
                    from.x - (x + radius)
                };
                (gap / dx.abs()).clamp(0.0, 1.0)
            })
        }

        fn grounded_slide(translation: Vec3) -> Slide {
            Slide {
                translation,
                grounded: true,
                ground: Some(face(Vec3::Y, 0.0)),
                hits: Vec::new(),
            }
        }

        fn tick(
            state: &mut CharacterState,
            tuning: &Tuning,
            command: &CharacterCommand,
            sweep: &Sweep,
        ) {
            let sweep = Rc::clone(sweep);
            let queries = Fake::new(|_, _, _| grounded_slide(Vec3::ZERO))
                .sweeping(move |from, to, radius| sweep(from, to, radius));

            super::super::step(state, command, &[], tuning, DT, &queries);
        }

        fn hold(
            state: &mut CharacterState,
            tuning: &Tuning,
            lean: f32,
            ticks: usize,
            sweep: &Sweep,
        ) {
            for _ in 0..ticks {
                tick(state, tuning, &leaning(lean), sweep);
            }
        }

        #[test]
        fn one_tick_leans_a_sixth_of_the_way() {
            for (wanted, side) in [(-1.0, 1.0), (1.0, -1.0)] {
                let mut state = on_the_floor();

                tick(&mut state, &tuning(), &leaning(wanted), &clear());

                let sixth = 1.0 / 6.0;
                assert!(
                    (state.lean - wanted * sixth).abs() < EPS,
                    "lean {} for {wanted}",
                    state.lean
                );
                assert!(
                    (state.eye.x - side * UPRIGHT_DISTANCE * sixth).abs() < EPS,
                    "the eye moved {} for {wanted}, expected {}",
                    state.eye.x,
                    side * UPRIGHT_DISTANCE * sixth
                );
                assert!((state.eye.y - STAND_EYE_Y).abs() < EPS, "{:?}", state.eye);
                assert!(state.eye.z.abs() < EPS, "{:?}", state.eye);
                assert!(
                    (state.roll - wanted * upright_roll() * sixth).abs() < EPS,
                    "roll {} for {wanted}",
                    state.roll
                );
            }
        }

        #[test]
        fn leaning_right_moves_the_eye_along_the_move_right_vector_of_a_turned_facing() {
            let mut state = on_the_floor();
            state.yaw = std::f32::consts::FRAC_PI_2;
            let before = Vec3::new(0.0, STAND_EYE_Y, 0.0);

            hold(&mut state, &tuning(), 1.0, 6, &clear());

            let right = Vec3::new(0.0, 0.0, 1.0);
            let expected = before + right * UPRIGHT_DISTANCE;
            assert!(
                state.eye.distance(expected) < EPS,
                "eye {:?}, expected {expected:?}",
                state.eye
            );
            assert!((state.roll - upright_roll()).abs() < EPS, "{}", state.roll);
        }

        #[test]
        fn releasing_returns_upright_over_the_lean_time() {
            let mut state = on_the_floor();
            hold(&mut state, &tuning(), 1.0, 6, &clear());
            assert!((state.lean - 1.0).abs() < EPS, "setup: {}", state.lean);

            let mut leans = Vec::new();
            let mut eyes = Vec::new();
            let mut rolls = Vec::new();
            for _ in 0..8 {
                tick(&mut state, &tuning(), &leaning(0.0), &clear());
                leans.push(state.lean);
                eyes.push(state.eye.x);
                rolls.push(state.roll);
            }

            for k in 1..=6 {
                let left = 1.0 - k as f32 / 6.0;
                assert!((leans[k - 1] - left).abs() < EPS, "tick {k}: {leans:?}");
                assert!(
                    (eyes[k - 1] + UPRIGHT_DISTANCE * left).abs() < EPS,
                    "tick {k}: {eyes:?}"
                );
                assert!(
                    (rolls[k - 1] - upright_roll() * left).abs() < EPS,
                    "tick {k}: {rolls:?}"
                );
            }
            assert!(
                leans[6..].iter().all(|l| l.abs() < EPS) && eyes[6..].iter().all(|x| x.abs() < EPS),
                "it stays upright: {leans:?} {eyes:?}"
            );
        }

        #[test]
        fn a_lean_command_beyond_full_is_clamped() {
            for (wanted, side) in [(5.0, -1.0), (-5.0, 1.0)] {
                let mut state = on_the_floor();

                hold(&mut state, &tuning(), wanted, 12, &clear());

                assert!((state.lean - wanted.signum()).abs() < EPS, "{}", state.lean);
                assert!(
                    (state.eye.x - side * UPRIGHT_DISTANCE).abs() < EPS,
                    "{:?}",
                    state.eye
                );
            }
        }

        #[test]
        fn a_non_finite_lean_command_leaves_the_lean_finite() {
            let mut state = on_the_floor();
            hold(&mut state, &tuning(), 0.5, 3, &clear());
            let partial = state.lean;

            for (wanted, settles) in [
                (f32::NAN, 0.0),
                (f32::INFINITY, 1.0),
                (f32::NEG_INFINITY, -1.0),
            ] {
                let before = state.lean;
                tick(&mut state, &tuning(), &leaning(wanted), &clear());
                let finite =
                    state.lean.is_finite() && state.eye.is_finite() && state.roll.is_finite();
                assert!(finite, "{wanted}: {state:?}");
                assert!(
                    (state.lean - before).abs() <= 1.0 / f32::from(LEAN_TICKS) + EPS,
                    "{wanted}: one tick's share"
                );
                hold(&mut state, &tuning(), wanted, 12, &clear());
                assert!(
                    (state.lean - settles).abs() < EPS,
                    "{wanted} settles at {settles}: {}",
                    state.lean
                );
                assert!(state.eye.is_finite() && state.roll.is_finite(), "{state:?}");
            }
            assert!(partial.abs() > EPS, "setup: part-way {partial}");
        }

        #[test]
        fn zero_lean_ticks_leans_fully_within_one_tick() {
            let tuning = Tuning {
                settings: MoveSettings {
                    lean_ticks: 0,
                    ..tuning().settings
                },
                ..tuning()
            };
            let mut state = on_the_floor();

            tick(&mut state, &tuning, &leaning(1.0), &clear());

            assert!((state.lean - 1.0).abs() < EPS, "lean {}", state.lean);
            assert!(
                (state.eye.x + UPRIGHT_DISTANCE).abs() < EPS,
                "eye {:?}",
                state.eye
            );
            assert!((state.roll - upright_roll()).abs() < EPS, "{}", state.roll);
        }

        #[test]
        fn a_partial_lean_command_settles_at_that_lean_without_overshoot() {
            let mut state = on_the_floor();

            let mut leans = Vec::new();
            for _ in 0..10 {
                tick(&mut state, &tuning(), &leaning(0.4), &clear());
                leans.push(state.lean);
            }

            for (i, lean) in leans.iter().enumerate() {
                let expected = ((i + 1) as f32 / 6.0).min(0.4);
                assert!((lean - expected).abs() < EPS, "tick {}: {leans:?}", i + 1);
            }
            assert!(
                (state.eye.x + 0.4 * UPRIGHT_DISTANCE).abs() < EPS,
                "the eye settles at 0.4 of the reach: {:?}",
                state.eye
            );
        }

        #[test]
        fn the_reach_blends_from_upright_to_prone_over_the_prone_leg() {
            let tuning = tuning();
            let mut state = state_at(Vec3::new(0.0, CROUCH_HALF, 0.0), Vec3::ZERO, true);
            state.stance = Stance::Crouch;
            state.from = Stance::Crouch;
            let crouched_leaning = CharacterCommand {
                stance: Stance::Crouch,
                ..leaning(1.0)
            };
            for _ in 0..6 {
                tick(&mut state, &tuning, &crouched_leaning, &clear());
            }
            assert!(
                (state.eye.x + UPRIGHT_DISTANCE).abs() < EPS,
                "setup: {:?}",
                state.eye
            );

            let mut halfway = state;
            for _ in 0..5 {
                tick(&mut halfway, &tuning, &prone_leaning(1.0), &clear());
            }
            let mut lying = halfway;
            for _ in 0..5 {
                tick(&mut lying, &tuning, &prone_leaning(1.0), &clear());
            }

            assert_eq!(halfway.stance, Stance::Prone);
            assert_eq!(halfway.progress, 5, "setup: halfway through the leg");
            let mid_distance = (UPRIGHT_DISTANCE + PRONE_DISTANCE) / 2.0;
            assert!(
                (halfway.eye.x + mid_distance).abs() < EPS,
                "halfway {:?}, expected x {}",
                halfway.eye,
                -mid_distance
            );
            let mid_roll = (upright_roll() + prone_roll()) / 2.0;
            assert!((halfway.roll - mid_roll).abs() < EPS, "{}", halfway.roll);
            assert!(
                (lying.eye.x + PRONE_DISTANCE).abs() < EPS,
                "{:?}",
                lying.eye
            );
            assert!((lying.roll - prone_roll()).abs() < EPS, "{}", lying.roll);
        }

        fn prone_leaning(lean: f32) -> CharacterCommand {
            CharacterCommand {
                lean,
                stance: Stance::Prone,
                ..CharacterCommand::default()
            }
        }

        #[test]
        fn the_sweep_runs_from_the_upright_eye_after_the_slide_to_the_leaning_one() {
            let calls: Rc<RefCell<Vec<(Vec3, Vec3, f32)>>> = Rc::default();
            let mut state = state_at(Vec3::new(1.0, STAND_HALF, 2.0), Vec3::ZERO, true);
            state.yaw = std::f32::consts::FRAC_PI_2;
            let recorded = Rc::clone(&calls);
            for _ in 0..6 {
                let recorded = Rc::clone(&recorded);
                let queries = Fake::new(|_, _, _| grounded_slide(Vec3::new(0.3, 0.0, 0.0)))
                    .sweeping(move |from, to, radius| {
                        recorded.borrow_mut().push((from, to, radius));
                        1.0
                    });

                super::super::step(&mut state, &leaning(1.0), &[], &tuning(), DT, &queries);
            }

            let calls = calls.borrow();
            let &(from, to, radius) = calls.last().expect("the step swept");
            let upright_eye = state.position + Vec3::new(0.0, STAND_EYE_Y, 0.0);
            assert!(
                from.distance(upright_eye) < EPS,
                "from {from:?}, upright eye {upright_eye:?}"
            );
            assert!(
                (to - from).distance(Vec3::new(0.0, 0.0, UPRIGHT_DISTANCE)) < EPS,
                "swept {from:?} to {to:?}"
            );
            assert!((radius - HEAD_RADIUS).abs() < EPS, "{radius}");
            assert!(
                (state.position.x - 2.8).abs() < EPS,
                "setup: the slide moved the character: {:?}",
                state.position
            );
        }

        #[test]
        fn a_wall_stops_the_eye_a_head_radius_short_and_scales_the_roll() {
            let mut state = on_the_floor();
            let wall = wall_at(0.25);

            hold(&mut state, &tuning(), -1.0, 10, &wall);

            let free = (0.25 - HEAD_RADIUS) / UPRIGHT_DISTANCE;
            assert!((state.eye.x - 0.15).abs() < EPS, "{:?}", state.eye);
            assert!(
                (state.roll + upright_roll() * free).abs() < EPS,
                "roll {}, expected {}",
                state.roll,
                -upright_roll() * free
            );
        }

        #[test]
        fn a_lean_held_against_a_wall_extends_when_it_clears() {
            let mut state = on_the_floor();
            hold(&mut state, &tuning(), -1.0, 10, &wall_at(0.25));
            assert!(
                (state.lean + 1.0).abs() < EPS,
                "the lean is held at full against the wall: {}",
                state.lean
            );
            assert!(state.eye.x < 0.2, "setup: clamped {:?}", state.eye);

            tick(&mut state, &tuning(), &leaning(-1.0), &clear());

            assert!(
                (state.eye.x - UPRIGHT_DISTANCE).abs() < EPS,
                "the eye is out at once: {:?}",
                state.eye
            );
            assert!((state.roll + upright_roll()).abs() < EPS, "{}", state.roll);
        }

        proptest! {
            #[test]
            fn the_eye_never_ends_inside_a_wall(
                gap in 0.02f32..1.0,
                wanted in 0.2f32..=1.0,
                left in any::<bool>(),
            ) {
                let side = if left { 1.0 } else { -1.0 };
                let wall_x = side * (HEAD_RADIUS + gap);
                let mut state = on_the_floor();

                hold(&mut state, &tuning(), -side * wanted, 12, &wall_at(wall_x));

                let reach = (wanted * UPRIGHT_DISTANCE).min(gap);
                prop_assert!(
                    (state.eye.x - side * reach).abs() < 1e-3,
                    "eye {:?} for gap {gap}, wanted {wanted}", state.eye
                );
                prop_assert!(
                    side * (state.eye.x + side * HEAD_RADIUS) <= side * wall_x + 1e-3,
                    "the eye's ball reaches into the wall at {wall_x}: {:?}", state.eye
                );
            }
        }
    }
}
