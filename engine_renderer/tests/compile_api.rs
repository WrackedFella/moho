// Ensure the public API for the renderer factory and trait type-checks.
// This test intentionally does not call into GPU initialization to avoid platform flakiness.

// The API compile test should pass both with and without the `backend-wgpu`
// feature. When the backend is enabled the factory signature takes an
// EventLoop/Window; otherwise it is a parameterless factory returning a
// boxed `RendererBackend`.
#[test]
fn api_compiles() {
    use engine_renderer::RendererBackend;
    #[cfg(feature = "backend-wgpu")]
    {
        use engine_renderer::create_renderer;
        // Validate the `backend-wgpu` factory signature.
        let _factory: fn(Option<&winit::window::Window>) -> Box<dyn RendererBackend> =
            create_renderer;
    }
    #[cfg(not(feature = "backend-wgpu"))]
    {
        use engine_renderer::create_renderer;
        let _factory: fn(Option<std::sync::Arc<()>>) -> Box<dyn RendererBackend> = create_renderer;
    }
}
