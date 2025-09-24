// Thin shim: re-export the renderer implementation from the `engine_renderer`
// crate so the binary can continue to reference `crate::gpu::Renderer`.
// This avoids duplicating the WGPU implementation in two places.

#[cfg(feature = "backend-wgpu")]
pub use engine_renderer::Renderer;

#[cfg(not(feature = "backend-wgpu"))]
pub use engine_renderer::Renderer;
