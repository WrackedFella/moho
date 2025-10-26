# engine_renderer

Small utility crate that exposes a backend-agnostic renderer API for moho.

Ownership and lifetimes
- The wgpu backend currently requires a borrow of a `winit::window::Window` for the lifetime of the renderer because `wgpu::Surface` borrows the window. The application should keep an `Arc<winit::window::Window>` and pass a borrow when creating the renderer.

# engine_renderer

Renderer utilities for Moho. Detailed design notes and lifetime constraints have been moved to `docs/moho_renderer/CONCEPTS.md`.

Quick start
-----------
- Use `create_renderer_from_arc(&window_arc)` to construct a renderer when you hold an `Arc<winit::window::Window>`.

Logging
-------
- The crate uses the `log` facade; initialize a logger (e.g., `env_logger`) or set `RUST_LOG` to control verbosity.

