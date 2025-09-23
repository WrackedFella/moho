// GPU renderer abstraction. Provides a `Renderer` type backed by either a
// placeholder (non-vulkan) implementation for fast iteration, or a Vulkano
// implementation behind the `vulkan` Cargo feature.

#[cfg(not(feature = "vulkan"))]
pub mod placeholder_renderer {
    use legion::World;
    use legion::query::IntoQuery;
    use engine_core::actors::InstanceGpu;

    /// Minimal placeholder renderer used when the `vulkan` feature is disabled.
    /// It implements the same `new` and `render` surface so `main` can be kept
    /// feature-agnostic.
    pub struct Renderer {}

    impl Renderer {
        pub fn new() -> Self {
            println!("Using placeholder renderer (no Vulkan).");
            Renderer {}
        }

        pub fn render(&mut self, _world: &mut World, _camera: (glam::Mat4, glam::Mat4)) {
            // For now do nothing; we'll log instance count for visibility.
            let mut instances: Vec<InstanceGpu> = Vec::new();
            let mut q = <&engine_core::actors::Sphere>::query();
            for s in q.iter(_world) {
                instances.push(s.to_instance());
            }
            println!("Placeholder render called ({} instances).", instances.len());
        }
    }

    // Re-export is done at the crate root; no local re-export needed here.
}

// When the `vulkan` feature is enabled, provide a Vulkano-backed renderer.
// Keep the implementation as a separate module to avoid bringing heavy Vulkan
// dependencies in the non-vulkan build.
#[cfg(feature = "vulkan")]
pub mod vulkan_renderer {
    use std::sync::Arc;
    use legion::World;
    use legion::query::IntoQuery;
    use engine_core::actors::InstanceGpu;

    // NOTE: keep this small and syntactically-correct. The full Vulkano
    // implementation can be iterated on. This skeleton ensures the module
    // compiles and provides the expected API.

    pub struct Renderer {
        // fields will hold Vulkan objects in the full implementation
        _private: (),
    }

    impl Renderer {
        /// Create a new Vulkan renderer. `event_loop` and `window` are taken so
        /// the renderer can create surface(s) and GPU resources. The signatures
        /// are intentionally simple; expand as needed when implementing the
        /// full renderer.
        pub fn new(event_loop: &winit::event_loop::EventLoop<()>, window: winit::window::Window) -> Self {
            println!("(vulkan) Initializing renderer (skeleton)");
            // For now, drop the provided values to avoid unused warnings in the skeleton.
            let _ = event_loop;
            let _ = window;
            Renderer { _private: () }
        }

        pub fn render(&mut self, _world: &mut World, _camera: (glam::Mat4, glam::Mat4)) {
            // Gather instances and log count; actual GPU submission will be
            // implemented later.
            let mut instances: Vec<InstanceGpu> = Vec::new();
            let mut q = <&engine_core::actors::Sphere>::query();
            for s in q.iter(_world) {
                instances.push(s.to_instance());
            }
            println!("(vulkan-skel) render called ({} instances).", instances.len());
        }
    }

    // Re-export is done at the crate root; no local re-export needed here.
}

// Export the appropriate Renderer type at the crate root so callers can use
// `gpu::Renderer` regardless of the active feature.
#[cfg(not(feature = "vulkan"))]
pub use placeholder_renderer::Renderer as Renderer;

#[cfg(feature = "vulkan")]
pub use vulkan_renderer::Renderer as Renderer;
