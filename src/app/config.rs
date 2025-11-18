//! Application configuration that affects initialization.
//!
//! This module centralizes all configuration parameters used during app initialization,
//! providing a single source of truth for settings that determine how the application
//! is set up and configured at runtime.

use moho_ui::prefs::Prefs;

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
    /// Mouse sensitivity multiplier (applied as: sensitivity * 0.002)
    pub mouse_sensitivity: f32,

    /// Whether input filtering is enabled in the input system
    pub input_filtering_enabled: bool,

    /// The filter preset to use for input processing
    pub filter_preset: moho_core::input::FilterPreset,

    /// Preferences object (contains keybindings and other settings)
    pub prefs: Prefs,
}

impl Default for AppConfig {
    fn default() -> Self {
        let prefs = Prefs::default();

        Self {
            mouse_sensitivity: prefs.mouse_sensitivity * 0.002,
            input_filtering_enabled: prefs.input_filtering_enabled,
            filter_preset: moho_core::input::FilterPreset::Default,
            prefs,
        }
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
        let mouse_sensitivity = prefs.mouse_sensitivity * 0.002;
        let input_filtering_enabled = prefs.input_filtering_enabled;

        Self {
            mouse_sensitivity,
            input_filtering_enabled,
            filter_preset: moho_core::input::FilterPreset::Default,
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
    filter_preset: Option<moho_core::input::FilterPreset>,
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

    /// Set the input filter preset.
    pub fn filter_preset(mut self, preset: moho_core::input::FilterPreset) -> Self {
        self.filter_preset = Some(preset);
        self
    }

    /// Set the preferences object directly.
    pub fn prefs(mut self, prefs: Prefs) -> Self {
        self.prefs = Some(prefs);
        self
    }

    /// Build the AppConfig with the specified settings.
    ///
    /// Any unset values will use defaults from AppConfig::default().
    pub fn build(self) -> AppConfig {
        let defaults = AppConfig::default();

        let prefs = self.prefs.unwrap_or(defaults.prefs);
        let mouse_sensitivity = self.mouse_sensitivity.unwrap_or(defaults.mouse_sensitivity);

        AppConfig {
            mouse_sensitivity,
            input_filtering_enabled: self
                .input_filtering_enabled
                .unwrap_or(defaults.input_filtering_enabled),
            filter_preset: self.filter_preset.unwrap_or(defaults.filter_preset),
            prefs,
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
        assert!(matches!(
            config.filter_preset,
            moho_core::input::FilterPreset::Default
        ));
    }

    #[test]
    fn test_builder_customization() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.5)
            .input_filtering(false)
            .filter_preset(moho_core::input::FilterPreset::Default)
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert_eq!(config.input_filtering_enabled, false);
    }

    #[test]
    fn test_builder_partial_customization() {
        let config = AppConfig::builder().mouse_sensitivity(0.75).build();

        assert_eq!(config.mouse_sensitivity, 0.75);
        assert!(config.input_filtering_enabled); // Uses default
    }

    #[test]
    fn test_from_prefs_struct() {
        let mut prefs = Prefs::default();
        prefs.mouse_sensitivity = 2.0;
        prefs.input_filtering_enabled = false;

        let config = AppConfig::from_prefs_struct(prefs.clone());

        assert_eq!(config.mouse_sensitivity, 2.0 * 0.002);
        assert_eq!(config.input_filtering_enabled, false);
        assert_eq!(config.prefs.mouse_sensitivity, 2.0);
    }

    #[test]
    fn test_builder_with_prefs() {
        let mut prefs = Prefs::default();
        prefs.mouse_sensitivity = 3.0;

        let config = AppConfig::builder()
            .prefs(prefs.clone())
            .mouse_sensitivity(0.5) // Override extracted sensitivity
            .build();

        assert_eq!(config.mouse_sensitivity, 0.5);
        assert_eq!(config.prefs.mouse_sensitivity, 3.0);
    }
}
