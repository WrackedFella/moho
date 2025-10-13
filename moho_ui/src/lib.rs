//! Modern, modular UI system for the Moho game engine.
//!
//! This crate provides a flexible, scalable UI system based on egui with support
//! for multiple menu types and easy extensibility for new UI components.

use engine_renderer::FrameCallback;

/// Stub UI implementation for when no UI features are enabled
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
        // no-op when no UI is available
    }
}

// Modern egui-based UI system
#[cfg(feature = "ui-egui")]
pub mod adapter;

#[cfg(feature = "ui-egui")]
pub mod menus;

// Re-export the main types for easy access
#[cfg(feature = "ui-egui")]
pub use adapter::{EguiAdapter, UI_OVERLAY_VISIBLE, UiEvent, UiReceiver, build_adapter};

#[cfg(feature = "ui-egui")]
pub use adapter::EguiAdapter as EguiUi;

#[cfg(feature = "ui-egui")]
pub use menus::{Menu, MenuAction, MenuItem, StartMenu};
