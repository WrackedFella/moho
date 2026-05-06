//! Window Management
//!
//! Handles window creation, renderer setup, and initial rendering.

use crate::App;
use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

/// Result type for window operations
pub type WindowResult<T> = Result<T, WindowError>;

/// Errors that can occur during window operations
#[derive(Debug)]
pub enum WindowError {
    /// Failed to create the window
    CreateFailed(String),
    /// Failed to setup renderer or UI
    SetupFailed(String),
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WindowError::CreateFailed(msg) => write!(f, "Failed to create window: {}", msg),
            WindowError::SetupFailed(msg) => write!(f, "Failed to setup renderer/UI: {}", msg),
        }
    }
}

impl std::error::Error for WindowError {}

/// Handles window initialization and management
pub struct WindowManager;

impl WindowManager {
    /// Create a new window manager
    pub fn new() -> Self {
        Self
    }

    /// Create and configure the main application window
    pub fn create_window(&self, event_loop: &ActiveEventLoop) -> WindowResult<Arc<Window>> {
        let mut window_attributes = WindowAttributes::default();
        window_attributes.title = "Project: Moho - Prototype".into();

        event_loop
            .create_window(window_attributes)
            .map(Arc::new)
            .map_err(|e| WindowError::CreateFailed(e.to_string()))
    }

    /// Setup renderer and UI for the given window
    pub fn setup_renderer_and_ui(&self, app: &mut App, window: Arc<Window>) -> WindowResult<()> {
        app.setup_renderer_and_ui(window)
            .map_err(|e| WindowError::SetupFailed(e.to_string()))
    }

    /// Perform initial render after window creation
    pub fn initial_render(&self, app: &mut App) {
        log::info!("Initial render");
        if let Some(ref mut wr) = app.window_renderer {
            app.scene.render(
                &mut *wr.renderer,
                &mut app.world,
                wr.mesh_handle,
                wr.cube_mesh_handle,
                app.camera,
            );
            wr.window.request_redraw();
        }
    }

    /// Handle the window resumed event (creates window if needed)
    pub fn handle_resumed(&self, app: &mut App, event_loop: &ActiveEventLoop) -> WindowResult<()> {
        if app.window_renderer.is_none() {
            let window = self.create_window(event_loop)?;
            self.setup_renderer_and_ui(app, window)?;
            self.apply_video_settings(app);
            self.initial_render(app);
        }
        Ok(())
    }

    /// Apply saved video settings (window mode and resolution) from prefs to the window.
    fn apply_video_settings(&self, app: &mut App) {
        use moho_core::prefs::WindowMode;
        use winit::dpi::PhysicalSize;
        use winit::window::Fullscreen;

        let Some(ref wr) = app.window_renderer else {
            return;
        };

        match app.prefs.window_mode() {
            WindowMode::Fullscreen | WindowMode::Borderless => {
                wr.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
            WindowMode::Windowed => {
                let (w, h) = app.prefs.window_resolution();
                let _ = wr.window.request_inner_size(PhysicalSize::new(w, h));
            }
        }
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_manager_creation() {
        let manager = WindowManager::new();
        assert_eq!(std::mem::size_of_val(&manager), 0); // Zero-sized type
    }

    #[test]
    fn test_window_manager_default() {
        let _manager = WindowManager;
        // Just verify it compiles and constructs
    }

    #[test]
    fn test_window_error_display() {
        let err = WindowError::CreateFailed("test error".into());
        assert_eq!(err.to_string(), "Failed to create window: test error");

        let err = WindowError::SetupFailed("setup error".into());
        assert_eq!(err.to_string(), "Failed to setup renderer/UI: setup error");
    }
}
