//! Application initialization orchestration using builder pattern.
//!
//! This module provides a structured way to initialize the application by composing
//! all the extracted initialization modules (event bus, audio, camera, config) into
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

use legion::World;
use moho_core::input::InputSystem;
use moho_sim::SimulationController;
use std::sync::Arc;
use std::time::{Duration, Instant};

use moho_ui::prefs::Prefs;

use super::audio_init::initialize_audio_system;
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
            AppInitError::EventBusSetup(msg) => write!(f, "Event bus setup failed: {}", msg),
            AppInitError::CameraSetup(msg) => write!(f, "Camera setup failed: {}", msg),
            AppInitError::InvalidConfig(msg) => write!(f, "Invalid configuration: {}", msg),
        }
    }
}

impl std::error::Error for AppInitError {}

/// Result of the initialization process, containing all initialized systems.
///
/// This struct holds all the components needed to construct an App instance.
pub struct InitializedApp {
    pub world: World,
    pub scene: moho_renderer::Scene,
    pub camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    pub event_bus: Arc<moho_core::EventBus>,

    pub ui_event_rx: crossbeam_channel::Receiver<moho_core::events::UiEvent>,
    pub audio_event_rx: crossbeam_channel::Receiver<moho_core::events::AudioEvent>,
    pub graphics_event_rx: crossbeam_channel::Receiver<moho_core::events::GraphicsEvent>,
    pub world_event_rx: crossbeam_channel::Receiver<moho_core::events::WorldEvent>,
    pub debug_event_rx: crossbeam_channel::Receiver<moho_core::events::DebugEvent>,

    pub audio_system: Option<moho_audio::AudioSystem>,
    pub simulation: SimulationController,
    pub input_system: InputSystem,

    pub prefs: Prefs,

    pub frame_duration: Duration,
    pub last_frame: Instant,
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
    /// 1. Initialize logging (if not already initialized)
    /// 2. Create world and scene
    /// 3. Setup camera
    /// 4. Setup event bus
    /// 5. Initialize audio system (optional)
    /// 6. Create simulation controller
    /// 7. Setup input system
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
        // Initialize logging (ignore error if already initialized for tests)
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
        log::info!("Starting application initialization");

        // Create basic world and scene (minimal setup)
        let world = World::default();
        let scene = moho_renderer::Scene::new();
        log::debug!("Created world and scene");

        // Initialize camera with default voxel terrain view
        let camera = create_default_camera();
        log::debug!("Initialized camera at position {:?}", camera.2);

        // Initialize simulation wrapper (owns controller + input state)
        let simulation = SimulationController::new(camera.2);
        log::debug!("Created simulation controller");

        // Initialize event bus and subscribers
        let event_bus_setup = setup_event_bus();
        let event_bus = event_bus_setup.event_bus;
        let ui_event_rx = event_bus_setup.ui_event_rx;
        let audio_event_rx = event_bus_setup.audio_event_rx;
        let graphics_event_rx = event_bus_setup.graphics_event_rx;
        let world_event_rx = event_bus_setup.world_event_rx;
        let debug_event_rx = event_bus_setup.debug_event_rx;
        log::debug!("Event bus initialized with subscribers");

        // Initialize audio system (optional - graceful failure).
        // Tests can skip this via AppConfig::init_audio = false: concurrent
        // WASAPI init across parallel tests can crash on Windows.
        let audio_system = if self.config.init_audio {
            let audio = initialize_audio_system();
            if audio.is_some() {
                log::info!("Audio system initialized successfully");
            } else {
                log::warn!("Audio system initialization failed - continuing without audio");
            }
            audio
        } else {
            log::debug!("Audio system initialization skipped (init_audio = false)");
            None
        };

        // Create input system with config values
        let mut input_system =
            InputSystem::new_with_preset(self.config.mouse_sensitivity, self.config.filter_preset);
        input_system.set_filter_enabled(self.config.input_filtering_enabled);
        log::debug!("Input system initialized");

        log::info!("Application initialization complete");

        Ok(InitializedApp {
            world,
            scene,
            camera,
            event_bus,

            ui_event_rx,
            audio_event_rx,
            graphics_event_rx,
            world_event_rx,
            debug_event_rx,

            audio_system,
            simulation,
            input_system,

            prefs: self.config.prefs,

            frame_duration: Duration::from_secs_f64(1.0 / 60.0),
            last_frame: Instant::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build tests skip audio init: concurrent WASAPI init across parallel
    // tests crashes on Windows runners.
    fn test_config() -> AppConfig {
        AppConfig::builder().init_audio(false).build()
    }

    #[test]
    fn test_initializer_with_default_config() {
        let config = AppConfig::default();
        let _initializer = AppInitializer::new(config);
        // Should be able to create initializer without panicking
    }

    #[test]
    fn test_initializer_with_custom_config() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.75)
            .input_filtering(false)
            .build();

        let _initializer = AppInitializer::new(config);
        // Should be able to create initializer without panicking
    }

    #[test]
    fn test_build_creates_all_systems() {
        let result = AppInitializer::new(test_config()).build();

        assert!(result.is_ok(), "Initialization should succeed");

        let initialized = result.unwrap();

        assert_eq!(
            initialized.frame_duration,
            Duration::from_secs_f64(1.0 / 60.0)
        );
    }

    #[test]
    fn test_build_respects_config_values() {
        let config = AppConfig::builder()
            .mouse_sensitivity(0.5)
            .input_filtering(false)
            .init_audio(false)
            .build();

        let result = AppInitializer::new(config).build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_initializes_camera() {
        let result = AppInitializer::new(test_config()).build();

        assert!(result.is_ok());
        let initialized = result.unwrap();

        // Camera tuple should contain valid matrices and position
        let (view, proj, eye) = initialized.camera;
        assert_ne!(
            view,
            glam::Mat4::IDENTITY,
            "View matrix should not be identity"
        );
        assert_ne!(
            proj,
            glam::Mat4::IDENTITY,
            "Projection matrix should not be identity"
        );
        assert_ne!(
            eye,
            glam::Vec3::ZERO,
            "Camera position should not be at origin"
        );
    }

    #[test]
    fn test_build_creates_simulation() {
        let result = AppInitializer::new(test_config()).build();

        assert!(result.is_ok());
        let initialized = result.unwrap();

        // Simulation should be created (can't test much without exposing internals)
        // Just verify it was constructed
        let _ = initialized.simulation;
    }

    #[test]
    fn test_multiple_builds_with_same_config() {
        let config = test_config();

        // Should be able to build multiple times (though in practice you'd only build once)
        let result1 = AppInitializer::new(config.clone()).build();
        let result2 = AppInitializer::new(config).build();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[test]
    fn test_build_includes_prefs() {
        let prefs = Prefs::default().with_mouse_sensitivity(2.5);

        let mut config = AppConfig::from_prefs_struct(prefs.clone());
        config.init_audio = false;
        let result = AppInitializer::new(config).build();

        assert!(result.is_ok());
        let initialized = result.unwrap();

        assert_eq!(initialized.prefs.mouse_sensitivity(), 2.5);
    }
}
