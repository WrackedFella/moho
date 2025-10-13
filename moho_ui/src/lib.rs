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
        _surface_width: u32,
        _surface_height: u32,
    ) {
        // no-op for now
    }
}

// Re-export adapter and types for the egui feature.
// Keep the public API surface minimal: everything below is only
// available when the `ui-egui` feature is enabled.
#[cfg(feature = "ui-egui")]
pub mod egui_adapter;

#[cfg(feature = "ui-egui")]
pub use egui_adapter::{EguiUi, UI_OVERLAY_VISIBLE, UiEvent, UiReceiver, build_adapter};

#[cfg(feature = "ui-egui")]
pub mod menus;
