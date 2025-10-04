//! Minimal `moho_ui` crate.
//!
//! This crate will implement an object-safe FrameCallback that the
//! application can pass into the renderer to composite UI in the final
//! command encoder. For now provide a stub implementation that does
//! nothing so the project can build while we iterate.

use engine_renderer::FrameCallback;

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
