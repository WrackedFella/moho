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
    /// Add to the velocity.
    AddVelocity(Vec3),
    /// Replace the velocity.
    SetVelocity(Vec3),
    /// Set the position; zero the velocity unless `keep_velocity`; clear `grounded`.
    Teleport { position: Vec3, keep_velocity: bool },
    /// Replace the look, with pitch clamped to [`PITCH_LIMIT`].
    SetLook { yaw: f32, pitch: f32 },
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
    /// Jump if grounded.
    pub jump: bool,
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
    /// Downward acceleration while airborne, m/s².
    pub gravity: f32,
    /// Upward speed a jump sets, m/s.
    pub jump_speed: f32,
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
    pub grounded: bool,
    /// The walkable surface under the capsule after the slide, within [`STEP_HEIGHT`]; `None`
    /// when there is none or the character started the slide airborne.
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
/// Absorbs float noise in the controller's translation so a grazed surface is not blocking.
const BLOCKED_EPSILON: f32 = 1e-4;

/// Smallest `normal.y` of a surface a character can walk on (cos 45 degrees).
///
/// The controller's `max_slope_climb_angle` is built from it, so both agree.
pub(crate) const WALKABLE_NORMAL_Y: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Furthest, in metres, a walker is pulled down onto ground, and the largest [`Ground::gap`]
/// at which a slide ending ungrounded still counts as standing.
///
/// The controller's `snap_to_ground` is built from it, so both agree. Otherwise a walker
/// carrying a ramp's upward speed over a crest asks an upward move, which the controller
/// does not snap, and hops.
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

/// Remove the velocity into every surface that blocked the slide, reporting each as an impact.
///
/// A hit blocks when the asked motion faces it and the slide moved less into it than asked,
/// or when it is the surface an airborne character just landed on. A walker (`walking`) is
/// never blocked by walkable ground it ends the slide standing on: slopes and the rounded
/// edges of steps neither stop nor clip it. Every other surface is judged on the horizontal
/// plane for a walker, as in Source's `WalkMove`, so a lift over a step never reads as
/// blocking and clipping never gives a walker vertical velocity.
fn clip(
    velocity: &mut Vec3,
    asked: Vec3,
    slid: &Slide,
    walking: bool,
    airborne_before: bool,
) -> Vec<Impact> {
    let mut impacts = Vec::new();
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
        let into = velocity.dot(normal);
        if blocked && into < 0.0 {
            impacts.push(Impact {
                collider,
                normal: surface,
                speed: -into,
            });
            *velocity -= normal * into;
        }
    }
    impacts
}

/// `look` (yaw, pitch) turned by `delta` with pitch clamped at [`PITCH_LIMIT`]; the one
/// place the turn is computed, so a drawn view and a stepped state agree bit for bit.
pub(crate) fn turned_look(look: (f32, f32), delta: (f32, f32)) -> (f32, f32) {
    (
        look.0 + delta.0,
        (look.1 + delta.1).clamp(-PITCH_LIMIT, PITCH_LIMIT),
    )
}

/// Apply one queued effect to `state`.
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

/// Advance `state` by one tick of `dt` seconds and report the impacts.
///
/// `slide` receives the displacement the tick asks for and reports what the world
/// allowed, along with the state it starts from, which a teleport may have moved. The queued
/// `effects` apply first, in order, so a replay of commands and effects reproduces the run.
/// The look turns next, so a turn and a move in one command face the new way.
///
/// The order within a tick:
/// 0. the queued effects apply in order; a teleport clears `grounded`, so that tick runs
///    airborne and settle finds the ground;
/// 1. the look turns;
/// 2. a jump adds `jump_speed` to the vertical velocity;
/// 3. half a tick of gravity (`v.y -= g·dt/2`), unless walking;
/// 4. friction and acceleration (walking) or air acceleration;
/// 5. `slide` receives `asked = v·dt`;
/// 6. the velocity is clipped against what the slide hit;
/// 7. the other half of gravity, unless walking;
/// 8. settle: a walker that ends the tick grounded moves along its ground, keeping its
///    horizontal velocity; a landing zeroes the vertical velocity.
///
/// The two halves around the move give an airborne tick the exact displacement under constant
/// gravity at any tick rate. A landing's impact speed is therefore the mid-tick downward speed.
pub fn step(
    state: &mut CharacterState,
    command: &CharacterCommand,
    effects: &[CharacterEffect],
    settings: &MoveSettings,
    dt: f32,
    slide: impl FnOnce(&CharacterState, Vec3) -> Slide,
) -> Vec<Impact> {
    for effect in effects {
        apply(state, *effect);
    }

    (state.yaw, state.pitch) = turned_look(
        (state.yaw, state.pitch),
        (command.yaw_delta, command.pitch_delta),
    );

    let (sin, cos) = state.yaw.sin_cos();
    let wish_vector =
        Vec3::new(sin, 0.0, cos) * command.forward + Vec3::new(-cos, 0.0, sin) * command.right;
    let wish = wish_vector.normalize_or_zero();
    let magnitude = wish_vector.length().min(1.0);

    let jumped = state.grounded && command.jump;
    if jumped {
        state.velocity.y += settings.jump_speed;
    }

    let airborne_before = !state.grounded;
    // A walker may climb a slope, so its upward speed only counts as leaving the ground once
    // it exceeds what its horizontal speed explains.
    let walking = state.grounded
        && !jumped
        && state.velocity.y <= state.velocity.with_y(0.0).length() + REST_SPEED;
    let half_gravity = settings.gravity * dt / 2.0;
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

    let asked = state.velocity * dt;
    let slid = slide(state, asked);
    state.position += slid.translation;

    let impacts = clip(&mut state.velocity, asked, &slid, walking, airborne_before);
    if !walking {
        state.velocity.y -= half_gravity;
    }

    settle(state, &slid, walking, asked, dt);
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
    state.grounded =
        (slid.grounded || hanging.is_some()) && (walking || state.velocity.y <= REST_SPEED);
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
        super::step(state, command, &[], settings, dt, |_, asked| slide(asked))
    }

    fn state_at(position: Vec3, velocity: Vec3, grounded: bool) -> CharacterState {
        CharacterState {
            position,
            velocity,
            yaw: 0.0,
            pitch: 0.0,
            grounded,
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
                    gravity: 18.0,
                    jump_speed: 8.0,
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
                gravity: 18.0,
                jump_speed: 6.5,
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
        fn walker_without_a_ground_normal_keeps_its_vertical_speed() {
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
            assert!(state.grounded);
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
                (asked.y - (settings.jump_speed - settings.gravity * DT / 2.0) * DT).abs() < 1e-6,
                "the jump tick asks (jump - g*dt/2)*dt, asked y = {}",
                asked.y
            );
            assert!(
                (state.velocity.y - (settings.jump_speed - settings.gravity * DT)).abs() < 1e-5,
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
                    ground: None,
                    hits: Vec::new(),
                },
            );

            assert!(state.grounded);
            assert_eq!(state.velocity.y, 0.0);
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
                (impacts[0].speed - (12.0 + settings.gravity * DT / 2.0)).abs() < 1e-4,
                "half gravity before the slide, so the impact is 12.15: {:?}",
                impacts[0]
            );
            assert_eq!(state.velocity.y, 0.0);
            assert!(state.grounded);
        }

        #[test]
        fn landing_hit_reports_fall_speed_and_zeroes_vertical_velocity() {
            let settings = MoveSettings {
                gravity: 0.0,
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
                &test_settings(),
                DT,
                |from, asked| flat_floor(from.position.y)(asked),
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
                &test_settings(),
                DT,
                |from, asked| {
                    seen = Some((asked, *from));
                    open_air(asked)
                },
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
                (asked.y - (4.0 - settings.gravity * DT / 2.0) * DT).abs() < 1e-6,
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
                &test_settings(),
                DT,
                |from, asked| {
                    started_from = Some(*from);
                    flat_floor(from.position.y)(asked)
                },
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
}
