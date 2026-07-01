//! In-game clock and celestial mechanics for day/night cycles.
//!
//! This module provides a configurable game clock that tracks time on a 24-hour cycle
//! and calculates sun and moon positions for realistic day/night transitions.

use bincode::{Decode, Encode};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// In-game clock tracking time of day and celestial mechanics.
///
/// The clock operates on a 24-hour cycle (0.0 - 24.0) with configurable
/// day and night lengths. It automatically calculates sun and moon positions
/// based on the current time.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct GameClock {
    /// Current time of day in hours (0.0 = midnight, 12.0 = noon, 24.0 wraps to 0.0)
    time_of_day: f32,

    /// Length of daytime in real-world seconds
    day_length_seconds: f32,

    /// Length of nighttime in real-world seconds
    night_length_seconds: f32,

    /// Total elapsed time in seconds (for debugging/stats)
    elapsed_seconds: f64,
}

impl GameClock {
    /// Create a new game clock with specified configuration.
    ///
    /// # Arguments
    /// * `initial_time` - Starting time in hours (0.0-24.0)
    /// * `day_length` - Length of daytime in real seconds
    /// * `night_length` - Length of nighttime in real seconds
    ///
    /// # Example
    /// ```
    /// use moho_game::game_clock::GameClock;
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
        }
    }

    /// Advance the clock by the given delta time (in seconds).
    ///
    /// The speed of time progression depends on whether it's currently day or night.
    /// This allows for asymmetric day/night cycles.
    ///
    /// # Arguments
    /// * `dt` - Delta time in real-world seconds
    pub fn tick(&mut self, dt: f32) {
        self.elapsed_seconds += dt as f64;

        // Determine current phase and corresponding speed
        let time_speed = if self.is_daytime() {
            // Day phase: 6:00 - 18:00 (12 hours)
            12.0 / self.day_length_seconds
        } else {
            // Night phase: 18:00 - 6:00 (12 hours)
            12.0 / self.night_length_seconds
        };

        // Advance time and wrap at 24 hours
        self.time_of_day += dt * time_speed;
        self.time_of_day = self.time_of_day.rem_euclid(24.0);
    }

    /// Get the current time of day in hours (0.0-24.0).
    pub fn time_of_day(&self) -> f32 {
        self.time_of_day
    }

    /// Get the total elapsed real-world time in seconds.
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed_seconds
    }

    /// Set the time of day directly (useful for testing/debugging).
    ///
    /// # Arguments
    /// * `time` - Time in hours (0.0-24.0), will be wrapped to valid range
    pub fn set_time(&mut self, time: f32) {
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
        format!("{:02}:{:02}", hours, minutes)
    }
}

impl Default for GameClock {
    fn default() -> Self {
        Self::new(6.0, 600.0, 420.0) // Start at dawn, 10min days, 7min nights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_creation() {
        let clock = GameClock::new(12.0, 600.0, 300.0);
        assert_eq!(clock.time_of_day(), 12.0);
        assert!(clock.is_daytime());
    }

    #[test]
    fn test_time_wrapping() {
        let mut clock = GameClock::new(23.5, 600.0, 300.0);
        assert_eq!(clock.time_of_day(), 23.5);

        clock.set_time(25.0); // Should wrap to 1.0
        assert_eq!(clock.time_of_day(), 1.0);
    }

    #[test]
    fn test_daytime_nighttime() {
        let mut clock = GameClock::new(12.0, 600.0, 300.0);
        assert!(clock.is_daytime());
        assert!(!clock.is_nighttime());

        clock.set_time(22.0);
        assert!(!clock.is_daytime());
        assert!(clock.is_nighttime());
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
    fn test_clock_tick() {
        let mut clock = GameClock::new(12.0, 120.0, 120.0); // Fast 2-minute cycles
        let initial_time = clock.time_of_day();

        clock.tick(1.0); // Advance 1 second

        assert!(clock.time_of_day() > initial_time);
        assert_eq!(clock.elapsed_seconds(), 1.0);
    }

    #[test]
    fn test_time_string() {
        let clock = GameClock::new(14.5, 600.0, 300.0);
        assert_eq!(clock.time_string(), "14:30");

        let clock = GameClock::new(9.0, 600.0, 300.0);
        assert_eq!(clock.time_string(), "09:00");
    }
}
