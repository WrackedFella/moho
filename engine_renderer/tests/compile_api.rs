// Ensure the public API for the renderer factory and trait type-checks.
// This test intentionally does not call into GPU initialization to avoid platform flakiness.

#[test]
fn api_compiles() {
    // Use the symbols so the compiler verifies their existence and signatures.
    use engine_renderer::{create_renderer, RendererBackend};

    // We can't construct a real renderer without a window/surface, but we can assert the
    // factory function exists and has the expected return type via a type annotation.
    let _factory: for<'a> fn(&'a winit::event_loop::EventLoop<()>, winit::window::Window) -> Box<dyn RendererBackend> = create_renderer;
}
