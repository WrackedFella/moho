//! In-game clock and celestial mechanics for day/night cycles.
//!
//! This module provides a configurable game clock that tracks time on a 24-hour cycle
//! and calculates sun and moon positions for realistic day/night transitions.

use glam::Vec3;
use std::f32::consts::PI;

/// In-game clock tracking time of day and celestial mechanics.
///
/// The clock operates on a 24-hour cycle (0.0 - 24.0) with configurable
/// day and night lengths. It automatically calculates sun and moon positions
/// based on the current time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameClock {
    /// Current time of day in hours (0.0 = midnight, 12.0 = noon, 24.0 wraps to 0.0)
    time_of_day: f32,

    /// Length of daytime in simulated seconds at time scale 1.0
    day_length_seconds: f32,

    /// Length of nighttime in simulated seconds at time scale 1.0
    night_length_seconds: f32,

    /// Total simulated time in seconds (for debugging/stats)
    elapsed_seconds: f64,

    /// Multiplier applied to every advance; 0.0 freezes the clock
    time_scale: f32,
}

impl GameClock {
    /// Create a new game clock with specified configuration.
    ///
    /// # Arguments
    /// * `initial_time` - Starting time in hours (0.0-24.0)
    /// * `day_length` - Length of daytime in simulated seconds at time scale 1.0
    /// * `night_length` - Length of nighttime in simulated seconds at time scale 1.0
    ///
    /// # Example
    /// ```
    /// use moho_app::GameClock;
    ///
    /// // 10 minute days, 7 minute nights, starting at dawn
    /// let clock = GameClock::new(6.0, 600.0, 420.0);
    /// ```
    pub fn new(initial_time: f32, day_length: f32, night_length: f32) -> Self {
        Self {
            time_of_day: initial_time.rem_euclid(24.0),
            day_length_seconds: day_length,
            night_length_seconds: night_length,
            elapsed_seconds: 0.0,
            time_scale: 1.0,
        }
    }

    /// Advance the clock by `dt` seconds of simulated time, scaled by the time scale.
    ///
    /// The speed of time progression depends on whether it's currently day or night.
    /// This allows for asymmetric day/night cycles.
    pub(crate) fn advance(&mut self, dt: f32) {
        let dt = dt * self.time_scale;
        self.elapsed_seconds += f64::from(dt);

        let time_speed = if self.is_daytime() {
            12.0 / self.day_length_seconds
        } else {
            12.0 / self.night_length_seconds
        };

        self.time_of_day = (self.time_of_day + dt * time_speed).rem_euclid(24.0);
    }

    /// Set the multiplier applied to every advance. Negative or non-finite
    /// values are stored as 0.0 (frozen).
    pub fn set_time_scale(&mut self, scale: f32) {
        self.time_scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            0.0
        };
    }

    /// The multiplier applied to every advance (default 1.0).
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    /// Get the current time of day in hours (0.0-24.0).
    pub fn time_of_day(&self) -> f32 {
        self.time_of_day
    }

    /// Get the total elapsed simulated time in seconds (scaled by the time scale).
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    /// Jump to `time` hours, wrapped into 0.0-24.0. Keeps the time scale and
    /// the elapsed total.
    pub fn reset_to(&mut self, time: f32) {
        self.time_of_day = time.rem_euclid(24.0);
    }

    /// Check if it's currently daytime (6:00 - 18:00).
    pub fn is_daytime(&self) -> bool {
        self.time_of_day >= 6.0 && self.time_of_day < 18.0
    }

    /// Check if it's currently nighttime (18:00 - 6:00).
    pub fn is_nighttime(&self) -> bool {
        !self.is_daytime()
    }

    /// Calculate the sun's direction vector based on current time.
    ///
    /// The sun:
    /// - Rises in the east (6:00 AM)
    /// - Reaches zenith in the south (12:00 PM)
    /// - Sets in the west (6:00 PM)
    /// - Is below horizon at night
    ///
    /// # Returns
    /// Normalized direction vector pointing toward the sun, or downward vector if sun is below horizon.
    pub fn sun_direction(&self) -> Vec3 {
        self.calculate_celestial_direction(self.time_of_day)
    }

    /// Calculate the moon's direction vector based on current time.
    ///
    /// The moon is always opposite the sun (+12 hours offset).
    ///
    /// # Returns
    /// Normalized direction vector pointing toward the moon, or downward vector if moon is below horizon.
    pub fn moon_direction(&self) -> Vec3 {
        let moon_time = (self.time_of_day + 12.0).rem_euclid(24.0);
        self.calculate_celestial_direction(moon_time)
    }

    /// Internal method to calculate celestial body direction for a given time.
    ///
    /// Uses a simplified celestial sphere model:
    /// - Azimuth: East (90°) -> South (0°) -> West (-90°)
    /// - Elevation: 0° at horizon, max 60° at zenith
    ///
    /// # Arguments
    /// * `time` - Time in hours (0.0-24.0)
    ///
    /// # Returns
    /// Normalized direction vector, or Vec3::NEG_Y if below horizon
    fn calculate_celestial_direction(&self, time: f32) -> Vec3 {
        // Early out if body is below horizon
        if !(6.0..18.0).contains(&time) {
            return Vec3::NEG_Y; // Point down when below horizon
        }

        // Map time to angle: 6:00 = -90° (east), 12:00 = 0° (south), 18:00 = 90° (west)
        // Time range 6-18 maps to angle range -90° to +90° (π/2 to -π/2)
        let hours_since_dawn = time - 6.0; // 0.0 to 12.0
        let progress = hours_since_dawn / 12.0; // 0.0 to 1.0

        // Azimuth: sweep from east (-π/2) through south (0) to west (π/2)
        let azimuth = (progress - 0.5) * PI; // -π/2 to π/2

        // Elevation: parabolic arc, max elevation at noon
        // Use sine curve for smooth arc: 0° at horizon, max 60° at zenith
        let elevation = (progress * PI).sin() * (PI / 3.0); // 0 to 60° and back to 0

        // Convert spherical coordinates to Cartesian (x=east, y=up, z=north)
        let x = azimuth.sin() * elevation.cos(); // East-west component
        let y = elevation.sin(); // Elevation component
        let z = azimuth.cos() * elevation.cos(); // North-south component

        Vec3::new(x, y, z).normalize()
    }

    /// Get both sun and moon directions in one call for efficiency.
    ///
    /// # Returns
    /// Tuple of (sun_direction, moon_direction)
    pub fn celestial_directions(&self) -> (Vec3, Vec3) {
        (self.sun_direction(), self.moon_direction())
    }

    /// Get a human-readable time string (HH:MM format).
    pub fn time_string(&self) -> String {
        let hours = self.time_of_day.floor() as u32;
        let minutes = ((self.time_of_day.fract() * 60.0).floor() as u32).min(59);
        format!("{hours:02}:{minutes:02}")
    }
}

impl Default for GameClock {
    fn default() -> Self {
        Self::new(6.0, 600.0, 420.0) // Start at dawn, 10min days, 7min nights
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glam::Vec3;

    use super::*;
    use crate::{FrameContext, Game, HeadlessLoop, LoopConfig, TickContext};

    const TOLERANCE: f32 = 1e-4;

    struct ScaledGame {
        scale: f32,
        seen_in_tick: Vec<f32>,
        seen_in_frame: Vec<f32>,
    }

    impl ScaledGame {
        fn new(scale: f32) -> Self {
            Self {
                scale,
                seen_in_tick: Vec::new(),
                seen_in_frame: Vec::new(),
            }
        }
    }

    impl Game for ScaledGame {
        type Command = ();

        fn command(&mut self) {}

        fn tick(&mut self, ctx: &mut TickContext<'_>, _command: &()) {
            ctx.clock.set_time_scale(self.scale);
            self.seen_in_tick.push(ctx.clock.time_of_day());
        }

        fn frame(&mut self, ctx: &mut FrameContext<'_>, _alpha: f32) {
            self.seen_in_frame.push(ctx.clock().time_of_day());
        }
    }

    /// Day runs 12 h per 120 s, so one second of simulated time is 0.1 h.
    fn noon_loop() -> HeadlessLoop<ScaledGame> {
        let clock = GameClock::new(12.0, 120.0, 60.0);
        HeadlessLoop::new(LoopConfig::new(60).with_clock(clock))
    }

    fn hours_advanced_in_60_ticks(scale: f32) -> f32 {
        let mut game = ScaledGame::new(scale);
        let mut sim = noon_loop();

        sim.step(&mut game, 60);

        sim.clock().time_of_day() - 12.0
    }

    #[test]
    fn frame_without_tick_leaves_clock_unchanged() {
        // A frame shorter than a tick leaves the clock alone; a 1 s frame runs
        // 60 ticks, which is 0.1 h on a 120 s day.
        let cases = [(1_000_000_000_u64 / 144, 12.0_f32), (1_000_000_000, 12.1)];

        for (frame_ns, expected_hours) in cases {
            let mut game = ScaledGame::new(1.0);
            let mut sim = HeadlessLoop::new(
                LoopConfig {
                    max_catch_up_ticks: 100,
                    ..LoopConfig::new(60)
                }
                .with_clock(GameClock::new(12.0, 120.0, 60.0)),
            );

            sim.advance(&mut game, Duration::from_nanos(frame_ns));

            assert!(
                (sim.clock().time_of_day() - expected_hours).abs() < TOLERANCE,
                "frame {frame_ns} ns: got {}",
                sim.clock().time_of_day()
            );
            assert_eq!(game.seen_in_frame, vec![sim.clock().time_of_day()]);
        }
    }

    #[test]
    fn time_scale_scales_the_advance() {
        let baseline = hours_advanced_in_60_ticks(1.0);

        let frozen = hours_advanced_in_60_ticks(0.0);
        let doubled = hours_advanced_in_60_ticks(2.0);

        assert!((baseline - 0.1).abs() < TOLERANCE, "baseline {baseline}");
        assert_eq!(frozen, 0.0);
        assert!(
            (doubled - 2.0 * baseline).abs() < TOLERANCE,
            "doubled {doubled}"
        );
    }

    #[test]
    fn a_tick_advances_the_clock_after_the_game_tick_has_run() {
        let mut game = ScaledGame::new(1.0);
        let mut sim = noon_loop();

        sim.step(&mut game, 2);

        assert_eq!(game.seen_in_tick[0], 12.0);
        assert!(game.seen_in_tick[1] > 12.0, "first tick must have advanced");
        assert!(sim.clock().time_of_day() > game.seen_in_tick[1]);
    }

    #[test]
    fn loop_starts_from_the_clock_in_its_config() {
        let sim = HeadlessLoop::<ScaledGame>::new(
            LoopConfig::new(60).with_clock(GameClock::new(17.5, 120.0, 60.0)),
        );

        assert_eq!(sim.clock().time_of_day(), 17.5);
    }

    #[test]
    fn reset_to_sets_time_without_a_tick() {
        let mut clock = GameClock::new(12.0, 120.0, 60.0);

        clock.reset_to(18.5);

        assert_eq!(clock.time_of_day(), 18.5);
        assert_eq!(clock.elapsed_seconds(), 0.0);
    }

    #[test]
    fn reset_to_keeps_the_time_scale() {
        let mut clock = GameClock::new(12.0, 120.0, 60.0);
        clock.set_time_scale(3.0);

        clock.reset_to(1.0);

        assert_eq!(clock.time_scale(), 3.0);
    }

    #[test]
    fn time_scale_defaults_to_one() {
        assert_eq!(GameClock::default().time_scale(), 1.0);
        assert_eq!(GameClock::new(3.0, 120.0, 60.0).time_scale(), 1.0);
    }

    #[test]
    fn negative_or_nan_time_scale_clamps_to_zero() {
        let cases = [
            (2.5, 2.5),
            (0.0, 0.0),
            (-1.0, 0.0),
            (f32::NEG_INFINITY, 0.0),
            (f32::NAN, 0.0),
            (f32::INFINITY, 0.0),
        ];

        for (set, stored) in cases {
            let mut clock = GameClock::default();

            clock.set_time_scale(set);

            assert_eq!(clock.time_scale(), stored, "set {set}");
        }
    }

    #[test]
    fn test_time_wrapping() {
        let mut clock = GameClock::new(23.5, 600.0, 300.0);
        assert_eq!(clock.time_of_day(), 23.5);

        clock.reset_to(25.0);
        assert_eq!(clock.time_of_day(), 1.0);

        clock.reset_to(-1.0);
        assert_eq!(clock.time_of_day(), 23.0);

        assert_eq!(GameClock::new(30.0, 600.0, 300.0).time_of_day(), 6.0);
        assert_eq!(GameClock::new(-1.0, 600.0, 300.0).time_of_day(), 23.0);
    }

    #[test]
    fn test_daytime_nighttime() {
        let mut clock = GameClock::new(12.0, 600.0, 300.0);
        assert!(clock.is_daytime());
        assert!(!clock.is_nighttime());

        clock.reset_to(22.0);
        assert!(!clock.is_daytime());
        assert!(clock.is_nighttime());

        clock.reset_to(6.0);
        assert!(clock.is_daytime(), "6.0 is the first daytime instant");
        assert!(!clock.is_nighttime());

        clock.reset_to(18.0);
        assert!(clock.is_nighttime(), "18.0 is the first nighttime instant");
        assert!(!clock.is_daytime());
    }

    #[test]
    fn test_sun_direction_noon() {
        let clock = GameClock::new(12.0, 600.0, 300.0);
        let sun = clock.sun_direction();

        // At noon, sun should be high (positive Y) and pointing south (positive Z)
        assert!(sun.y > 0.7, "Sun should be high at noon");
        assert!(sun.z > 0.0, "Sun should be in south at noon");
    }

    #[test]
    fn test_sun_direction_dawn() {
        let clock = GameClock::new(6.0, 600.0, 300.0);
        let sun = clock.sun_direction();

        // At dawn, sun should be at horizon in the east
        assert!(sun.y.abs() < 0.2, "Sun should be near horizon at dawn");
        assert!(sun.x < -0.5, "Sun should be in east at dawn");
    }

    #[test]
    fn test_sun_below_horizon() {
        let clock = GameClock::new(22.0, 600.0, 300.0);
        let sun = clock.sun_direction();

        // At night, sun should point down
        assert_eq!(sun, Vec3::NEG_Y);
    }

    #[test]
    fn test_moon_opposite_sun() {
        let clock = GameClock::new(12.0, 600.0, 300.0);
        let moon = clock.moon_direction();

        // At noon, moon should be below horizon (midnight for moon)
        assert_eq!(moon, Vec3::NEG_Y);

        let clock = GameClock::new(0.0, 600.0, 300.0);
        let moon = clock.moon_direction();

        // At midnight, moon should be high (noon for moon)
        assert!(moon.y > 0.7, "Moon should be high at midnight");
    }

    #[test]
    fn celestial_directions_pairs_sun_and_moon() {
        let clock = GameClock::new(9.0, 600.0, 300.0);

        assert_eq!(
            clock.celestial_directions(),
            (clock.sun_direction(), clock.moon_direction())
        );
    }

    #[test]
    fn test_clock_advance() {
        // (start hour, hour after 1 s): day runs 12 h per 120 s, night 12 h
        // per 60 s; 23.95 wraps past midnight.
        let cases = [(12.0, 12.1), (0.0, 0.2), (23.95, 0.15)];

        for (start, expected) in cases {
            let mut clock = GameClock::new(start, 120.0, 60.0);

            clock.advance(1.0);

            assert!(
                (clock.time_of_day() - expected).abs() < TOLERANCE,
                "from {start}: got {}, expected {expected}",
                clock.time_of_day()
            );
            assert_eq!(clock.elapsed_seconds(), 1.0);
        }
    }

    #[test]
    fn test_time_string() {
        let clock = GameClock::new(14.5, 600.0, 300.0);
        assert_eq!(clock.time_string(), "14:30");

        let clock = GameClock::new(9.0, 600.0, 300.0);
        assert_eq!(clock.time_string(), "09:00");
    }
}
