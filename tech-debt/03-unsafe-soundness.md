# TD-03: Audit and Fix Unsafe Soundness Issues

**Priority:** High — unjustified `unsafe` is a latent soundness hole and violates
the project's own CLAUDE.md rule: "Avoid `unsafe` without documentation."

## Problems

### A. `unsafe impl Send + Sync for EguiAdapter` (moho_ui)

```rust
// moho_ui/src/adapter/mod.rs:95-96
unsafe impl Send for EguiAdapter {}
unsafe impl Sync for EguiAdapter {}
```

`EguiAdapter` contains `egui::Context`, `egui_winit::State`, and
`egui_wgpu::Renderer`. These are generally not safe to share across threads — the
adapter works only because it lives behind `Arc<Mutex<...>>` and is always accessed
on the main thread. The `unsafe impl` asserts thread-safety that isn't analysed or
documented.

### B. `Box::leak` for renderer lifetime (`src/main.rs:226`)

```rust
let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
```

The comment calls this "acceptable" for a main-window lifetime, but it is a real
memory leak with no cleanup path. If the window is ever recreated (e.g. fullscreen
toggle on some platforms), the leak compounds.

### C. `unsafe { std::mem::transmute }` in `moho_ui/src/adapter/gpu_ops.rs:63-65`

Used to satisfy `egui_wgpu` lifetime requirements on the render pass. This may be
legitimate but is undocumented.

## Acceptance Criteria

### For A
- Either: remove `unsafe impl Send/Sync`, make `EguiAdapter` explicitly `!Send`
  (by holding `Rc` or a `PhantomData<*mut ()>`), and keep it on the main thread
  via a `Mutex`-like wrapper that only exposes it from the main thread.
- Or: document *exactly* which contained types are the soundness risk, prove they
  are safe (e.g. `egui::Context` is in fact `Send + Sync` as of recent egui), and
  add a `// SAFETY:` comment per Rust convention covering every field.

### For B
- Replace `Box::leak` by ensuring the `Arc<Window>` is stored on `App` and that
  the renderer holds a `&'a Window` reference tied to the `Arc`'s lifetime.
  `create_renderer_from_arc` already exists in `moho_renderer` for this purpose.
  Wire it correctly so the renderer's lifetime is bounded by `App`.

### For C
- Add a `// SAFETY:` comment explaining why the transmuted lifetime is valid (e.g.
  the render pass does not outlive the encoder/view it borrows from).

## Files

- `moho_ui/src/adapter/mod.rs`
- `moho_ui/src/adapter/gpu_ops.rs`
- `src/main.rs` (~L226)
- `moho_renderer/src/lib.rs` — `create_renderer_from_arc` already exists, use it
