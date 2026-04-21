# TD-09: Clean Up RendererBackend Trait

**Priority:** Low-Medium — architectural noise that doesn't block work but
accumulates confusion.

## Problem

`moho_renderer::RendererBackend` has three no-op methods on its only implementation:

```rust
fn request_redraw(&self) { /* no-op */ }
fn set_cursor_visible(&self, _visible: bool) { /* no-op */ }
fn set_cursor_grab(&self, _locked: bool) -> Result<(), ...> { Ok(()) }
```

These are no-ops because the renderer no longer owns the window. The application
controls cursor/redraw directly via `Arc<Window>`. These methods should not be on the
trait — they create a false impression that renderers manage cursor state.

Additionally, the public surface is duplicated: virtually every method on
`RendererBackend` is a 1:1 forward to an inherent method on
`gfx::wgpu_impl::Renderer`. This exists to support a "backend-swap" story (comments
mention "when the backend-wgpu feature is enabled") but there is only one backend,
no feature gate is used at the call sites, and the `gfx::wgpu_impl` nesting serves
no apparent purpose. The `pub mod prelude` re-exports only `Renderer` — it is noise.

## Acceptance Criteria

1. `request_redraw`, `set_cursor_visible`, `set_cursor_grab` are removed from
   `RendererBackend`. All call sites in `main.rs`/`event_loop/` that use these
   through the trait are updated to call `Arc<Window>` directly (as most already do).

2. Either:
   - Retain `RendererBackend` as the primary API and justify the trait (e.g. for
     future headless/test backend), OR
   - Collapse the trait and use `Renderer` directly in the main binary if a second
     backend is not planned.
   
   If retained: document the trait's purpose and what a conforming backend must provide.

3. The `gfx::wgpu_impl` nesting is removed or documented. The concrete `Renderer`
   type is accessible without navigating two module levels.

4. `pub mod prelude` is removed if it only re-exports one type with no other value.

## Files

- `moho_renderer/src/lib.rs` — `RendererBackend` trait definition, `gfx::wgpu_impl`,
  `RendererBackend for gfx::wgpu_impl::Renderer`, `prelude`
- `src/main.rs` — any calls through trait that should go to `Arc<Window>` directly
