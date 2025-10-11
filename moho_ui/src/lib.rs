//! Minimal `moho_ui` crate.
//!
//! This crate will implement an object-safe FrameCallback that the
//! application can pass into the renderer to composite UI in the final
//! command encoder. For now provide a stub implementation that does
//! nothing so the project can build while we iterate.

use engine_renderer::FrameCallback;
// Ensure wgpu types are available for the FrameCallback signature when
// the optional iced/ui feature is enabled. The dependency is optional in
// Cargo.toml but importing the crate here ensures the symbols are linked
// when enabled. (no direct `use wgpu;` needed)

pub struct StubUi;

impl StubUi {
    pub fn new() -> Self {
        StubUi {}
    }
}

impl Default for StubUi {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameCallback for StubUi {
    fn call(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _view: &wgpu::TextureView,
        _encoder: &mut wgpu::CommandEncoder,
    ) {
        // no-op for now
    }
}

// Re-export adapter and types for the egui feature. Also make them
// available during `cargo test` so integration tests can reference
// the adapter symbols even when the feature gate is not enabled in
// the default test configuration.
#[cfg(any(feature = "ui-egui", test))]
pub mod egui_adapter;

#[cfg(any(feature = "ui-egui", test))]
pub use egui_adapter::{EguiUi, UI_OVERLAY_VISIBLE, UiEvent, UiReceiver};

#[cfg(any(feature = "ui-egui", test))]
pub mod menus;
