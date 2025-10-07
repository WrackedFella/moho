# engine_renderer

Small utility crate that exposes a backend-agnostic renderer API for moho.

Ownership and lifetimes
- The wgpu backend currently requires a borrow of a `winit::window::Window` for the lifetime of the renderer because `wgpu::Surface` borrows the window. The application should keep an `Arc<winit::window::Window>` and pass a borrow when creating the renderer.

Helpers
- `create_renderer(window: Option<&Window>)` - existing factory used by the app.
- `create_renderer_from_arc(window: &Arc<Window>)` - convenience helper that accepts an `Arc<Window>` and forwards a borrow into `create_renderer`. This centralizes creation logic and makes call sites cleaner while keeping the renderer implementation safe.

Future changes
- If the renderer is changed to own the `Arc<Window>` (for `'static` ergonomics), the helper can be updated to perform the ownership transfer without changing application call sites.

Tests
- The crate includes several unit and integration-style tests that are deliberately
	platform-independent. Where possible we use small mock implementations of
	`RendererBackend` so tests exercise renderer-facing logic (material dedup,
	scene submission, save/load) without initializing GPU adapters or windows.

Examples
- Use `create_renderer_from_arc(&window_arc)` to create a renderer from an
	`Arc<winit::window::Window>` while the application retains ownership of the
	Window. This helper is intentionally lightweight and forwards a borrow to the
	underlying factory.

Logging
-------

The crate uses the `log` facade. Applications should initialize a logger to
see the renderer's log output. A simple option is `env_logger`:

```rust
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
// or set RUST_LOG in the environment: RUST_LOG=debug cargo run -- ...
```

Set the `RUST_LOG` environment variable to control verbosity (for example,
`RUST_LOG=debug` to enable trace/debug output from the renderer).
