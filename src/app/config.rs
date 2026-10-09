//! Application configuration that affects initialization.
//!
//! This module centralizes all configuration parameters used during app initialization,
//! providing a single source of truth for settings that determine how the application
//! is set up and configured at runtime.

use moho_core::prefs::Prefs;

/// Central configuration for application initialization.
///
/// This struct aggregates all settings that affect how the application is initialized,
/// making it easy to configure the app and test different configurations.
///
/// # Example
/// ```no_run
/// use moho::app::config::AppConfig;
///
/// // Load configuration from preferences file
/// let config = AppConfig::from_prefs();
///
/// // Or create with custom settings
/// let config = AppConfig::builder()
///     .mouse_sensitivity(0.5)
///     .input_filtering(true)
///     .build();
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Per-pixel look factor: the prefs mouse sensitivity scaled by 0.002.
    pub mouse_sensitivity: f32,

    /// Whether input filtering is enabled in the input system
    pub input_filtering_enabled: bool,

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
    /// use moho::app::config::AppConfig;
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
        Self {
            mouse_sensitivity: prefs.mouse_sensitivity() * 0.002,
            input_filtering_enabled: prefs.input_filtering_enabled(),
            prefs,
        }
    }

    /// Create a builder for customizing configuration.
    ///
    /// # Example
    /// ```
    /// use moho::app::config::AppConfig;
    ///
    /// let config = AppConfig::builder()
    ///     .mouse_sensitivity(0.5)
    ///     .input_filtering(false)
    ///     .build();
    ///
    /// assert_eq!(config.mouse_sensitivity, 0.5);
    /// assert_eq!(config.input_filtering_enabled, false);
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
    input_filtering_enabled: Option<bool>,
    prefs: Option<Prefs>,
}

#[allow(dead_code)] // Builder API for future use
impl AppConfigBuilder {
    /// Set the mouse sensitivity multiplier.
    pub fn mouse_sensitivity(mut self, sensitivity: f32) -> Self {
        self.mouse_sensitivity = Some(sensitivity);
        self
    }

    /// Set whether input filtering is enabled.
    pub fn input_filtering(mut self, enabled: bool) -> Self {
        self.input_filtering_enabled = Some(enabled);
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
            input_filtering_enabled: self
                .input_filtering_enabled
                .unwrap_or(base.input_filtering_enabled),
            prefs: base.prefs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert!(config.mouse_sensitivity > 0.0);
        assert!(config.input_filtering_enabled);
    }

    #[test]
    fn test_builder_customization() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.5)
            .input_filtering(false)
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert!(!config.input_filtering_enabled);
    }

    #[test]
    fn test_builder_partial_customization() {
        let config = AppConfig::builder().mouse_sensitivity(0.75).build();

        assert_eq!(config.mouse_sensitivity, 0.75);
        assert!(config.input_filtering_enabled); // Uses default
    }

    #[test]
    fn test_from_prefs_struct() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.0)
            .with_input_filtering_enabled(false);

        let config = AppConfig::from_prefs_struct(prefs.clone());

        assert_eq!(config.mouse_sensitivity, 2.0 * 0.002);
        assert!(!config.input_filtering_enabled);
        assert_eq!(config.prefs.mouse_sensitivity(), 2.0);
    }

    #[test]
    fn builder_with_prefs_derives_unset_fields_from_them() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.5)
            .with_input_filtering_enabled(false);

        let config = AppConfig::builder().prefs(prefs).build();

        assert_eq!(config.mouse_sensitivity, 2.5 * 0.002);
        assert!(!config.input_filtering_enabled);
    }

    #[test]
    fn builder_with_prefs_and_explicit_sensitivity_takes_filtering_from_prefs() {
        let prefs = Prefs::default().with_input_filtering_enabled(false);

        let config = AppConfig::builder()
            .prefs(prefs)
            .mouse_sensitivity(0.5)
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert!(!config.input_filtering_enabled);
    }

    #[test]
    fn builder_with_prefs_and_explicit_filtering_takes_sensitivity_from_prefs() {
        let prefs = Prefs::default()
            .with_mouse_sensitivity(2.5)
            .with_input_filtering_enabled(false);

        let config = AppConfig::builder()
            .prefs(prefs)
            .input_filtering(true)
            .build();

        assert_eq!(config.mouse_sensitivity, 2.5 * 0.002);
        assert!(config.input_filtering_enabled);
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
