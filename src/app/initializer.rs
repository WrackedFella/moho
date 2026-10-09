//! Application initialization orchestration using builder pattern.
//!
//! This module provides a structured way to initialize the application by composing
//! all the extracted initialization modules (event bus, camera, config) into
//! a cohesive initialization flow.
//!
//! # Example
//! ```no_run
//! use moho::app::initializer::AppInitializer;
//! use moho::app::config::AppConfig;
//!
//! let config = AppConfig::from_prefs();
//! let app = AppInitializer::new(config)
//!     .build()
//!     .expect("Failed to initialize application");
//! ```

use moho_game::scene::SceneEntities;
use moho_game::simulation::SimulationController;
use moho_input::action_map::ActionMap;
use moho_ui::actions::{StrategyAction, load_bindings};
use std::sync::Arc;

use moho_core::prefs::Prefs;

use super::camera::create_default_camera;
use super::config::AppConfig;
use super::event_setup::setup_event_bus;

/// Error type for application initialization failures.
#[derive(Debug)]
#[allow(dead_code)] // Builder error variants for future use
pub enum AppInitError {
    /// Failed to initialize the event bus
    EventBusSetup(String),

    /// Failed to initialize camera
    CameraSetup(String),

    /// Configuration is invalid
    InvalidConfig(String),
}

impl std::fmt::Display for AppInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppInitError::EventBusSetup(msg) => write!(f, "Event bus setup failed: {msg}"),
            AppInitError::CameraSetup(msg) => write!(f, "Camera setup failed: {msg}"),
            AppInitError::InvalidConfig(msg) => write!(f, "Invalid configuration: {msg}"),
        }
    }
}

impl std::error::Error for AppInitError {}

/// Result of the initialization process, containing all initialized systems.
///
/// This struct holds all the components needed to construct an App instance.
pub struct InitializedApp {
    pub entities: SceneEntities,
    pub scene: moho_renderer::Scene,
    pub terrain_material_idx: u32,
    pub camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    pub event_bus: Arc<moho_core::EventBus>,

    pub ui_event_rx: std::sync::mpsc::Receiver<moho_core::events::UiEvent>,
    pub audio_event_rx: std::sync::mpsc::Receiver<moho_core::events::AudioEvent>,
    pub graphics_event_rx: std::sync::mpsc::Receiver<moho_core::events::GraphicsEvent>,
    pub world_event_rx: std::sync::mpsc::Receiver<moho_voxel::WorldEvent>,
    pub debug_event_rx: std::sync::mpsc::Receiver<moho_core::events::DebugEvent>,

    pub simulation: SimulationController,
    pub actions: ActionMap<StrategyAction>,

    pub prefs: Prefs,
}

/// Builder for initializing the application in stages.
///
/// This builder pattern makes initialization more testable and provides
/// clear separation between different initialization phases.
///
/// # Example
/// ```no_run
/// use moho::app::initializer::AppInitializer;
/// use moho::app::config::AppConfig;
///
/// let config = AppConfig::builder()
///     .mouse_sensitivity(0.5)
///     .build();
///
/// let initialized = AppInitializer::new(config)
///     .build()
///     .expect("Initialization failed");
/// ```
pub struct AppInitializer {
    config: AppConfig,
}

impl AppInitializer {
    /// Create a new initializer with the given configuration.
    ///
    /// # Example
    /// ```
    /// use moho::app::initializer::AppInitializer;
    /// use moho::app::config::AppConfig;
    ///
    /// let config = AppConfig::default();
    /// let initializer = AppInitializer::new(config);
    /// ```
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Build the application by initializing all systems.
    ///
    /// This performs the full initialization sequence:
    /// 1. Create world and scene
    /// 2. Setup camera
    /// 3. Setup event bus
    /// 4. Create simulation controller
    /// 5. Setup input system
    ///
    /// It installs no `tracing` subscriber; the caller does.
    ///
    /// # Errors
    ///
    /// Returns `AppInitError` if any initialization step fails.
    ///
    /// # Example
    /// ```no_run
    /// use moho::app::initializer::AppInitializer;
    /// use moho::app::config::AppConfig;
    ///
    /// let config = AppConfig::default();
    /// match AppInitializer::new(config).build() {
    ///     Ok(initialized) => println!("App initialized successfully"),
    ///     Err(e) => eprintln!("Initialization failed: {}", e),
    /// }
    /// ```
    pub fn build(self) -> Result<InitializedApp, AppInitError> {
        tracing::info!("Starting application initialization");

        // Create basic world and scene (minimal setup)
        let entities = SceneEntities::default();
        let mut scene = moho_renderer::Scene::new();
        let terrain_material_idx =
            crate::app::world_geometry::register_terrain_material(&mut scene);
        tracing::debug!("Created world and scene");

        // Initialize camera with default voxel terrain view
        let camera = create_default_camera();
        tracing::debug!(pos = ?camera.2, "Initialized camera");

        // Initialize simulation wrapper (owns controller + input state)
        let simulation = SimulationController::new(camera.2);
        tracing::debug!("Created simulation controller");

        // Initialize event bus and subscribers
        let event_bus_setup = setup_event_bus();
        let event_bus = event_bus_setup.event_bus;
        let ui_event_rx = event_bus_setup.ui_event_rx;
        let audio_event_rx = event_bus_setup.audio_event_rx;
        let graphics_event_rx = event_bus_setup.graphics_event_rx;
        let world_event_rx = event_bus_setup.world_event_rx;
        let debug_event_rx = event_bus_setup.debug_event_rx;
        tracing::debug!("Event bus initialized with subscribers");

        // Create input system with config values
        let mut actions = ActionMap::new(
            load_bindings(&self.config.prefs),
            self.config.mouse_sensitivity,
        );
        actions.set_filtering(self.config.input_filtering_enabled);
        tracing::debug!("Input system initialized");

        tracing::info!("Application initialization complete");

        Ok(InitializedApp {
            entities,
            scene,
            terrain_material_idx,
            camera,
            event_bus,

            ui_event_rx,
            audio_event_rx,
            graphics_event_rx,
            world_event_rx,
            debug_event_rx,

            simulation,
            actions,

            prefs: self.config.prefs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_applies_config_sensitivity_and_filtering() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.5)
            .input_filtering(false)
            .build();
        let mut initialized = AppInitializer::new(config)
            .build()
            .expect("non-default config builds");

        initialized.actions.mouse_motion(10.0, 0.0);

        assert_eq!(initialized.actions.end_tick().look(), (5.0, 0.0));
    }

    #[test]
    fn test_build_includes_prefs() {
        let prefs = Prefs::default().with_mouse_sensitivity(2.5);

        let config = AppConfig::from_prefs_struct(prefs.clone());
        let result = AppInitializer::new(config).build();

        assert!(result.is_ok());
        let initialized = result.unwrap();

        assert_eq!(initialized.prefs.mouse_sensitivity(), 2.5);
    }
}
