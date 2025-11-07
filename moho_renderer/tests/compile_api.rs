// Ensure the public API for the renderer factory and trait type-checks.
// This test intentionally does not call into GPU initialization to avoid platform flakiness.

// The API always uses wgpu backend now. The factory signature takes a Window reference
// and returns a boxed `RendererBackend` with the same lifetime.
#[test]
fn api_compiles() {
    use moho_renderer::RendererBackend;
    use moho_renderer::create_renderer;
    use moho_renderer::create_renderer_from_arc;
    
    // Validate the wgpu factory signature (lifetime-bearing).
    let _factory: for<'a> fn(
        Option<&'a winit::window::Window>,
    ) -> Result<
        Box<dyn RendererBackend + 'a>,
        Box<dyn std::error::Error>,
    > = create_renderer;
    
    // Validate the helper that accepts an Arc<Window> and forwards a borrow (lifetime-bearing).
    let _helper: for<'a> fn(
        &'a std::sync::Arc<winit::window::Window>,
    ) -> Result<
        Box<dyn RendererBackend + 'a>,
        Box<dyn std::error::Error>,
    > = create_renderer_from_arc;
}
