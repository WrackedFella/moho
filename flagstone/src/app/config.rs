//! Application configuration that affects initialization.
//!
//! This module centralizes all configuration parameters used during app initialization,
//! providing a single source of truth for settings that determine how the application
//! is set up and configured at runtime.

use moho_core::prefs::Prefs;
use moho_input::LookSettings;
use moho_input::action_map::source_look_per_count;

/// The look settings `prefs` ask for, with sensitivity converted from Source scale to
/// radians per count. Acceleration is zero while off or when the strength is negative.
pub fn look_settings(prefs: &Prefs) -> LookSettings {
    LookSettings {
        look_per_count: source_look_per_count(prefs.mouse_sensitivity()),
        smoothing: prefs.mouse_smoothing(),
        invert_y: prefs.invert_mouse_y(),
        acceleration: if prefs.mouse_acceleration() {
            source_look_per_count(prefs.mouse_acceleration_strength().max(0.0))
        } else {
            0.0
        },
    }
}

/// Central configuration for application initialization.
///
/// This struct aggregates all settings that affect how the application is initialized,
/// making it easy to configure the app and test different configurations.
///
/// # Example
/// ```no_run
/// use flagstone::app::config::AppConfig;
///
/// // Load configuration from preferences file
/// let config = AppConfig::from_prefs();
///
/// // Or create with custom settings
/// let config = AppConfig::builder()
///     .mouse_sensitivity(0.5)
///     .mouse_smoothing(true)
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Radians of look per mouse count: the prefs sensitivity on Source's scale.
    pub mouse_sensitivity: f32,

    /// Whether mouse smoothing is on.
    pub mouse_smoothing: bool,

    /// Whether the mouse's Y axis is inverted.
    pub invert_mouse_y: bool,

    /// Extra look units per count for each count per millisecond; 0 is off.
    pub mouse_acceleration: f32,

    /// Preferences object (contains keybindings and other settings)
    pub prefs: Prefs,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::from_prefs_struct(Prefs::default())
    }
}

impl AppConfig {
    /// Load configuration from the preferences file.
    ///
    /// This reads from `config/prefs.ini` and applies all user settings.
    /// If the file doesn't exist, it creates it with default values.
    ///
    /// # Example
    /// ```no_run
    /// use flagstone::app::config::AppConfig;
    ///
    /// let config = AppConfig::from_prefs();
    /// assert!(config.mouse_sensitivity > 0.0);
    /// ```
    pub fn from_prefs() -> Self {
        let prefs = Prefs::load();
        Self::from_prefs_struct(prefs)
    }

    /// Create configuration from an existing Prefs struct.
    ///
    /// This is useful for testing or when you already have a Prefs instance.
    pub fn from_prefs_struct(prefs: Prefs) -> Self {
        let look = look_settings(&prefs);
        Self {
            mouse_sensitivity: look.look_per_count,
            mouse_smoothing: look.smoothing,
            invert_mouse_y: look.invert_y,
            mouse_acceleration: look.acceleration,
            prefs,
        }
    }

    /// Create a builder for customizing configuration.
    ///
    /// # Example
    /// ```
    /// use flagstone::app::config::AppConfig;
    ///
    /// let config = AppConfig::builder()
    ///     .mouse_sensitivity(0.5)
    ///     .mouse_smoothing(false)
    ///     .build();
    ///
    /// assert_eq!(config.mouse_sensitivity, 0.5);
    /// assert!(!config.mouse_smoothing);
    /// ```
    #[allow(dead_code)] // Builder API for future use
    pub fn builder() -> AppConfigBuilder {
        AppConfigBuilder::default()
    }
}

/// Builder for creating custom AppConfig instances.
///
/// This is particularly useful for testing different configurations
/// without needing to manipulate preference files.
#[derive(Default)]
#[allow(dead_code)] // Builder for future use
pub struct AppConfigBuilder {
    mouse_sensitivity: Option<f32>,
    mouse_smoothing: Option<bool>,
    prefs: Option<Prefs>,
}

#[allow(dead_code)] // Builder API for future use
impl AppConfigBuilder {
    /// Set the mouse sensitivity multiplier.
    pub fn mouse_sensitivity(mut self, sensitivity: f32) -> Self {
        self.mouse_sensitivity = Some(sensitivity);
        self
    }

    /// Set whether mouse smoothing is on.
    pub fn mouse_smoothing(mut self, enabled: bool) -> Self {
        self.mouse_smoothing = Some(enabled);
        self
    }

    /// Set the preferences object directly.
    pub fn prefs(mut self, prefs: Prefs) -> Self {
        self.prefs = Some(prefs);
        self
    }

    /// Build the AppConfig with the specified settings.
    ///
    /// Unset values come from the supplied prefs, or from `Prefs::default()`
    /// when none were supplied.
    pub fn build(self) -> AppConfig {
        let base = AppConfig::from_prefs_struct(self.prefs.unwrap_or_default());

        AppConfig {
            mouse_sensitivity: self.mouse_sensitivity.unwrap_or(base.mouse_sensitivity),
            mouse_smoothing: self.mouse_smoothing.unwrap_or(base.mouse_smoothing),
            invert_mouse_y: base.invert_mouse_y,
            mouse_acceleration: base.mouse_acceleration,
            prefs: base.prefs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_input::action_map::source_look_per_count;

    #[test]
    fn prefs_file_stays_config_prefs_ini() {
        assert_eq!(
            Prefs::config_path(),
            std::path::Path::new("config/prefs.ini")
        );
    }

    #[test]
    fn look_settings_take_smoothing_from_prefs() {
        let on = Prefs::default()
            .with_mouse_sensitivity(2.0)
            .with_mouse_smoothing(true);
        let off = Prefs::default()
            .with_mouse_sensitivity(2.0)
            .with_mouse_smoothing(false);

        assert_eq!(
            look_settings(&on),
            LookSettings {
                look_per_count: source_look_per_count(2.0),
                smoothing: true,
                invert_y: false,
                acceleration: 0.0,
            }
        );
        assert_eq!(
            look_settings(&off),
            LookSettings {
                look_per_count: source_look_per_count(2.0),
                smoothing: false,
                invert_y: false,
                acceleration: 0.0,
            }
        );
    }

    #[test]
    fn look_settings_pass_strength_only_when_on() {
        let on = Prefs::default()
            .with_mouse_acceleration(true)
            .with_mouse_acceleration_strength(0.3);
        let off = Prefs::default()
            .with_mouse_acceleration(false)
            .with_mouse_acceleration_strength(0.3);
        let negative = Prefs::default()
            .with_mouse_acceleration(true)
            .with_mouse_acceleration_strength(-0.2);

        assert_eq!(look_settings(&on).acceleration, source_look_per_count(0.3));
        assert_eq!(look_settings(&off).acceleration, 0.0);
        assert_eq!(look_settings(&negative).acceleration, 0.0);
    }

    #[test]
    fn look_settings_take_invert_y_from_prefs() {
        let inverted = Prefs::default().with_invert_mouse_y(true);

        assert!(look_settings(&inverted).invert_y);
        assert!(!look_settings(&Prefs::default()).invert_y);
    }

    #[test]
    fn default_prefs_look_turns_sixty_six_degrees_per_thousand_counts() {
        let look = look_settings(&Prefs::default());

        let degrees = (1000.0 * look.look_per_count).to_degrees();

        assert!((degrees - 66.0).abs() < 1e-3, "turned {degrees}");
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.mouse_sensitivity > 0.0);
        assert!(!config.mouse_smoothing);
    }

    #[test]
    fn test_builder_customization() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.5)
            .mouse_smoothing(true)
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert!(config.mouse_smoothing);
    }

    #[test]
    fn test_builder_partial_customization() {
        let config = AppConfig::builder().mouse_sensitivity(0.75).build();

        assert_eq!(config.mouse_sensitivity, 0.75);
        assert!(!config.mouse_smoothing); // Uses default
    }

    #[test]
    fn test_from_prefs_struct() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.0)
            .with_mouse_smoothing(true);

        let config = AppConfig::from_prefs_struct(prefs.clone());

        assert_eq!(config.mouse_sensitivity, source_look_per_count(2.0));
        assert!(config.mouse_smoothing);
        assert_eq!(config.prefs.mouse_sensitivity(), 2.0);
    }

    #[test]
    fn builder_with_prefs_derives_unset_fields_from_them() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.5)
            .with_mouse_smoothing(true);

        let config = AppConfig::builder().prefs(prefs).build();

        assert_eq!(config.mouse_sensitivity, source_look_per_count(2.5));
        assert!(config.mouse_smoothing);
    }

    #[test]
    fn builder_with_prefs_and_explicit_sensitivity_takes_smoothing_from_prefs() {
        let prefs = Prefs::default().with_mouse_smoothing(true);

        let config = AppConfig::builder()
            .prefs(prefs)
            .mouse_sensitivity(0.5)
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert!(config.mouse_smoothing);
    }

    #[test]
    fn builder_with_prefs_and_explicit_smoothing_takes_sensitivity_from_prefs() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.5)
            .with_mouse_smoothing(true);

        let config = AppConfig::builder()
            .prefs(prefs)
            .mouse_smoothing(false)
            .build();

        assert_eq!(config.mouse_sensitivity, source_look_per_count(2.5));
        assert!(!config.mouse_smoothing);
    }

    #[test]
    fn test_builder_with_prefs() {
        let prefs = Prefs::default().with_mouse_sensitivity(3.0);

        let config = AppConfig::builder()
            .prefs(prefs.clone())
            .mouse_sensitivity(0.5) // Override extracted sensitivity
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert_eq!(config.prefs.mouse_sensitivity(), 3.0);
    }
}
