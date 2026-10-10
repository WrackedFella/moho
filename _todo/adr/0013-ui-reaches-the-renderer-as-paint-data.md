# 0013 — UI reaches the renderer as plain paint data; the renderer owns UI drawing

**Status:** Proposed (2026-10-10)

## Context

[ENG-F20](https://github.com/WrackedFella/moho/issues/104) wants a wgpu upgrade
and an egui upgrade to each touch one crate. Today `moho_ui` draws egui through
`egui-wgpu`, so it depends on wgpu and receives `Device`, `Queue`, `TextureView`
and `CommandEncoder` through the renderer's frame callback. `egui-wgpu` pins one
wgpu version per egui release, so neither library can move without the other,
and the UI crate holds GPU code.

## Decision

- **UI is paint data.** `moho_render_api` defines a UI frame as clipped,
  textured triangle meshes plus texture set/free deltas and the pixels-per-point
  scale. It names no wgpu and no egui type.
- **The renderer owns UI drawing.** `moho_renderer` converts that data to GPU
  buffers and textures and draws it over the finished scene, with its own
  pipeline and shader, targeting its own surface format. The painter is a port
  of `egui-wgpu` 0.34's (premultiplied-alpha blending, sRGB vertex colours);
  custom paint callbacks are not supported.
- **The game supplies the frame through a wgpu-free source.** The renderer pulls
  `Option<UiFrame>` from a game-registered source each frame, passing the
  surface size.
- **egui stays in the UI crate.** Mapping egui's output to paint data is the UI
  crate's job; no engine crate depends on `egui` or `egui-wgpu`.

## Consequences

- An egui upgrade touches only `moho_ui` (its mapping); a wgpu upgrade touches
  only `moho_renderer`. #156 (ENG-F20-02) moves `moho_ui` onto the paint data
  and removes the wgpu-typed frame callback; #157 (ENG-F20-03) enforces it.
- The engine owns a few hundred lines of painter code and must follow egui's
  paint-output semantics through the mapping, not the renderer.
- A UI that needs custom GPU drawing inside a panel needs a new seam; none is
  built until a consumer asks.
