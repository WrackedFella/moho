//! Per-tick character movement rules (Quake/Source style).
//!
//! [`step`] is a pure function of the character's state, the tick's command, the
//! game's settings and one collide-and-slide query, so it can be re-run for prediction.

use crate::world::CharacterState;
use glam::Vec3;
use rapier3d::geometry::ColliderHandle;

/// Largest pitch magnitude in radians, short of straight up or down.
pub const PITCH_LIMIT: f32 = 1.54;

/// One tick's movement intent for a character.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    /// Most speed ground input adds per second, m/s².
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
    /// Each surface touched, with its normal.
    pub hits: Vec<(ColliderHandle, Vec3)>,
}

/// Tolerance, in metres, on "the slide moved less into a surface than asked".
///
/// Absorbs float noise in the controller's translation so a grazed surface is not blocking.
const BLOCKED_EPSILON: f32 = 1e-4;

/// Smallest `normal.y` of a surface a character can walk on (cos 45 degrees).
///
/// The controller's `max_slope_climb_angle` is built from it, so both agree.
pub(crate) const WALKABLE_NORMAL_Y: f32 = std::f32::consts::FRAC_1_SQRT_2;

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

/// Friction on the horizontal speed, then acceleration toward the wished ground velocity.
///
/// Held input also pulls an overspeed back toward `max_speed`, by up to `ground_accel * dt`.
fn ground_move(velocity: &mut Vec3, wish: Vec3, magnitude: f32, settings: &MoveSettings, dt: f32) {
    let speed = velocity.with_y(0.0).length();
    if speed > 0.0 {
        let drop = speed.max(settings.stop_speed) * settings.friction * dt;
        let scale = (speed - drop).max(0.0) / speed;
        velocity.x *= scale;
        velocity.z *= scale;
    }
    if magnitude > 0.0 {
        let target = wish * settings.max_speed * magnitude;
        let to_target = (target - *velocity).with_y(0.0);
        let distance = to_target.length();
        if distance > 0.0 {
            let gain = distance.min(settings.ground_accel * dt);
            *velocity += to_target * (gain / distance);
        }
    }
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

/// Advance `state` by one tick of `dt` seconds and report the impacts.
///
/// `slide` receives the displacement the tick asks for and reports what the world
/// allowed. The look turns first, so a turn and a move in one command face the new way.
pub fn step(
    state: &mut CharacterState,
    command: &CharacterCommand,
    settings: &MoveSettings,
    dt: f32,
    slide: impl FnOnce(Vec3) -> Slide,
) -> Vec<Impact> {
    state.yaw += command.yaw_delta;
    state.pitch = (state.pitch + command.pitch_delta).clamp(-PITCH_LIMIT, PITCH_LIMIT);

    let (sin, cos) = state.yaw.sin_cos();
    let wish_vector =
        Vec3::new(sin, 0.0, cos) * command.forward + Vec3::new(-cos, 0.0, sin) * command.right;
    let wish = wish_vector.normalize_or_zero();
    let magnitude = wish_vector.length().min(1.0);

    if state.grounded && command.jump {
        state.velocity.y = settings.jump_speed;
    }

    let airborne_before = !state.grounded;
    let walking = state.grounded && state.velocity.y <= REST_SPEED;
    if walking {
        ground_move(&mut state.velocity, wish, magnitude, settings, dt);
    } else {
        accelerate(
            &mut state.velocity,
            wish,
            settings.air_speed_cap * magnitude,
            settings.air_accel,
            dt,
        );
        state.velocity.y -= settings.gravity * dt;
    }

    let asked = state.velocity * dt;
    let slid = slide(asked);
    state.position += slid.translation;

    let impacts = clip(&mut state.velocity, asked, &slid, walking, airborne_before);

    state.grounded = slid.grounded && state.velocity.y <= REST_SPEED;
    if state.grounded {
        state.velocity.y = 0.0;
    }
    impacts
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

    fn state_at(position: Vec3, velocity: Vec3, grounded: bool) -> CharacterState {
        CharacterState {
            position,
            velocity,
            yaw: 0.0,
            pitch: 0.0,
            grounded,
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
                hits,
            }
        }
    }

    fn walk(
        state: &mut CharacterState,
        command: &CharacterCommand,
        settings: &MoveSettings,
    ) -> Vec<Impact> {
        step(state, command, settings, DT, flat_floor(state.position.y))
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
            step(state, command, &test_settings(), DT, open_air_slide);
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

            step(&mut state, &jump, &test_settings(), DT, flat_floor(0.0));
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
        fn airborne_character_gains_downward_speed_from_gravity() {
            let mut state = state_at(Vec3::new(0.0, 50.0, 0.0), Vec3::ZERO, false);

            drift(&mut state, &CharacterCommand::default());

            assert!(
                (state.velocity.y + 18.0 * DT).abs() < 1e-4,
                "{:?}",
                state.velocity
            );
            assert!(
                (state.position.y - (50.0 - 18.0 * DT * DT)).abs() < 1e-4,
                "gravity applies before the slide, y = {}",
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

            step(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
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

            step(
                &mut state,
                &CharacterCommand::default(),
                &test_settings(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked * 0.25,
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: Vec3::new(asked.x, 0.0, 0.0),
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked + Vec3::new(0.0, 0.45, 0.0),
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |_| Slide {
                    translation: Vec3::ZERO,
                    grounded: true,
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

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked * 0.25,
                    grounded: true,
                    hits: vec![(collider(), wall_normal()), (collider(), wall_normal())],
                },
            );

            assert_eq!(impacts.len(), 1, "{impacts:?}");
        }

        #[test]
        fn landing_beside_a_wall_the_slide_was_not_stopped_by_reports_no_wall_impact() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, -2.0, 3.0), false);

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked,
                    grounded: true,
                    hits: vec![(collider(), wall_normal())],
                },
            );

            assert!(impacts.is_empty(), "{impacts:?}");
            assert_eq!(state.velocity.z, 3.0);
        }

        #[test]
        fn moving_short_by_exactly_the_tolerance_is_not_blocked() {
            let mut state = state_at(Vec3::ZERO, Vec3::new(0.0, 0.0, 30.0), false);

            let impacts = step(
                &mut state,
                &CharacterCommand::default(),
                &frictionless(),
                DT,
                |asked| Slide {
                    translation: asked.with_z(-(asked.dot(wall_normal()) + BLOCKED_EPSILON)),
                    grounded: false,
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
                hits: vec![(collider(), Vec3::Z)],
            };

            let impacts = clip(&mut velocity, Vec3::new(0.1, 0.0, 0.0), &slid, false, false);

            assert!(impacts.is_empty(), "{impacts:?}");
            assert_eq!(velocity, Vec3::new(1.0, 0.0, -1.0));
        }

        #[test]
        fn landing_hit_reports_fall_speed_and_zeroes_vertical_velocity() {
            let settings = MoveSettings {
                gravity: 0.0,
                ..test_settings()
            };
            let mut state = state_at(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -12.0, 0.0), false);

            let impacts = step(
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

        let impacts = step(&mut state, &command, &test_settings(), 0.0, flat_floor(0.0));

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

        let first_impacts = step(&mut first, &command, &test_settings(), DT, flat_floor(0.0));
        let second_impacts = step(&mut second, &command, &test_settings(), DT, flat_floor(0.0));

        assert_eq!(first, second);
        assert_eq!(first_impacts, second_impacts);
        assert_ne!(first, start, "the step did change the state it was given");
    }

    proptest! {
        #[test]
        fn ground_speed_from_input_never_exceeds_max_speed(
            forward in -2.0f32..2.0,
            right in -2.0f32..2.0,
            yaw in -7.0f32..7.0,
            start_speed in 0.0f32..4.0,
            start_angle in -7.0f32..7.0,
        ) {
            let settings = test_settings();
            let command = CharacterCommand { forward, right, ..CharacterCommand::default() };
            let velocity = Vec3::new(start_angle.sin(), 0.0, start_angle.cos()) * start_speed;
            let mut state = state_at(Vec3::ZERO, velocity, true);
            state.yaw = yaw;

            for _ in 0..120 {
                walk(&mut state, &command, &settings);
                prop_assert!(
                    horizontal(state.velocity).length() <= settings.max_speed + 1e-3,
                    "{:?}", state.velocity
                );
            }
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

            step(&mut state, &command, &settings, DT, open_air);

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
}
