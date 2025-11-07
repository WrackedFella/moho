//! Window Event Handling
//!
//! Handles window-level events like resize, redraw, close, and keyboard input.

use crate::App;
use winit::event::{DeviceEvent, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;

/// Handles window event processing
pub struct WindowEventHandler;

impl WindowEventHandler {
    /// Create a new window event handler
    pub fn new() -> Self {
        Self
    }

    /// Handle a window event
    pub fn handle_window_event(
        &self,
        app: &mut App,
        event_loop: &ActiveEventLoop,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.handle_close_requested(app, event_loop);
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(app, size.width, size.height);
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard_input(app, &key_event);
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw_requested(app);
            }
            _ => {}
        }
    }

    /// Handle window close request
    fn handle_close_requested(&self, app: &mut App, event_loop: &ActiveEventLoop) {
        // Auto-save before close
        if let Err(e) = app.auto_save_on_shutdown() {
            log::warn!("Failed to auto-save on close: {}", e);
        }
        event_loop.exit();
    }

    /// Handle window resize
    fn handle_resize(&self, app: &mut App, width: u32, height: u32) {
        if let Some(ref mut wr) = app.window_renderer {
            wr.renderer.resize(width, height);
        }
    }

    /// Handle keyboard input
    fn handle_keyboard_input(&self, app: &mut App, key_event: &KeyEvent) {
        app.handle_keyboard_input(key_event);
    }

    /// Handle redraw request
    fn handle_redraw_requested(&self, app: &mut App) {
        log::debug!("RedrawRequested - rendering frame");
        if let Some(ref mut wr) = app.window_renderer {
            app.scene.render(
                &mut *wr.renderer,
                &mut app.world,
                wr.mesh_handle,
                wr.cube_mesh_handle,
                app.camera,
            );
        }

        // Recall staging belt after render
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.recall_staging_belt();
        }
    }

    /// Handle device event (raw input)
    pub fn handle_device_event(&self, app: &mut App, event: DeviceEvent) {
        // Handle raw mouse motion for camera look
        if let DeviceEvent::MouseMotion { delta } = event {
            app.handle_mouse_motion(delta);
        }
    }
}

impl Default for WindowEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_event_handler_creation() {
        let handler = WindowEventHandler::new();
        assert_eq!(std::mem::size_of_val(&handler), 0); // Zero-sized type
    }

    #[test]
    fn test_window_event_handler_default() {
        let _handler = WindowEventHandler::default();
        // Just verify it compiles and constructs
    }
}
