// Ensure the public API for the renderer factory and trait type-checks.
// This test intentionally does not call into GPU initialization to avoid platform flakiness.

#[test]
fn api_compiles() {
    // Use the symbols so the compiler verifies their existence and signatures.
    use engine_renderer::{RendererBackend, create_renderer};

    // The factory signature differs depending on whether the backend-wgpu
    // feature (and therefore winit) is enabled. We assert the expected
    // signature for each configuration so the test compiles in CI.
    #[cfg(feature = "backend-wgpu")]
    let _factory: for<'a> fn(
        &'a winit::event_loop::EventLoop<()>,
        winit::window::Window,
    ) -> Box<dyn RendererBackend> = create_renderer;

    #[cfg(not(feature = "backend-wgpu"))]
    let _factory: fn() -> Box<dyn RendererBackend> = create_renderer;
}
