# GPU ABI (Rust ↔ WGSL)

**Source of truth:** the Rust structs. WGSL mirrors them.

| Rust | WGSL |
|---|---|
| `moho_renderer/src/gpu_types.rs` | `shaders/common.wgsl` |
| `moho_render_api/src/material.rs` (`MaterialGpu`) | `Material` in `common.wgsl` |
| `moho_render_api/src/instance.rs` (`InstanceGpu`) | `InstanceIn` vertex inputs |
| `moho_render_api/src/ui_paint.rs` (`UiVertex`) | vertex inputs in `shaders/ui.wgsl` |
| `moho_render_api/src/marker.rs` (`MarkerGpu`) | vertex inputs in `shaders/marker.wgsl` |

All structs are `#[repr(C)]`, `bytemuck::{Pod, Zeroable}`, and built from `vec4`-sized
fields so std140/std430 padding never differs between the two sides. Size matches
`size_of::<T>()`. `UiVertex` and `Vertex` are the exceptions: they are only read as
vertex buffers, so they are packed with no padding (`UiVertex`: `Float32x2`, `Float32x2`,
`Uint32`, 20 B; `Vertex`: see the layout below, 56 B).

## Sizes

| Rust type | Size | Bound as |
|---|---|---|
| `CameraGpu` | 80 B (5 × vec4) | uniform, group 0 binding 0 |
| `MaterialGpu` | 32 B (2 × vec4) | storage array, group 0 binding 1 |
| `LightingGpu` | 144 B (9 × vec4) | uniform, group 0 binding 2 |
| `DynamicLightsGpu` | 2064 B (16 + 64 × 32) | storage, group 0 binding 5 |
| `MultiLightShadowGpu` | 288 B (4 × mat4 + 2 × vec4) | uniform, group 1 binding 0 |
| `PointLightGpu` | 32 B (2 × vec4) | element of `DynamicLightsGpu` |
| `ShadowMatrixGpu` | 64 B | legacy single shadow map |
| `ToneMapGpu` | 16 B (f32 exposure, u32 enabled, 2 × u32 pad) | uniform, tone-map pass group 0 binding 1 (`ToneMap` in `shaders/tonemap.wgsl`); binding 0 is the HDR texture |
| `InstanceGpu` | 80 B (mat4 + 4 × u32) | per-instance vertex buffer |
| `MarkerGpu` | 48 B (3 × vec4) | per-instance vertex buffer, marker pass |
| `Vertex` (`moho_renderer/src/types.rs`) | 56 B (packed, 14 × 4 B) | per-vertex buffer, main pass |
| `UiVertex` | 20 B (f32×2 pos, f32×2 uv, u8×4 colour) | UI pass vertex buffer |

Group 0 also binds the SSAO texture and sampler (bindings 3, 4). Group 1 binds the
shadow map array (`texture_depth_2d_array`, binding 1), a comparison sampler (2), and a
nearest sampler for the PCSS blocker search (3).

## Field layouts

| Struct | Field | Contents |
|---|---|---|
| `CameraGpu` | `vp0`–`vp3` | view-projection matrix columns |
| | `cam_pos` | xyz eye position, w view-height cut |
| `LightingGpu` | `sun_direction` | xyz normalized direction, w intensity |
| | `sun_color` | xyz colour |
| | `moon_direction` | xyz normalized direction, w intensity |
| | `moon_color` | xyz colour |
| | `ambient` | w sky-ambient strength; xyz unused |
| | `params` | x time of day 0–24, y debug mode |
| | `sky_zenith`, `sky_horizon`, `sky_ground` | xyz sky palette colour; filled by the renderer from `params.x` |
| `MaterialGpu` | `albedo` | xyz albedo (terrain: top-face colour); w gloss 0..1 for world meshes (`object_type` 0), 0 = matte, unused otherwise |
| | `params` | xyz `[fuzz, ref_idx, transparent]` (terrain: side-face colour), w emissive |
| `MultiLightShadowGpu` | `light0`–`light3` | one mat4 each: Sun, Moon, Dynamic 1, Dynamic 2 |
| | `light_intensities` | x sun, y moon, z dyn 1, w dyn 2 |
| | `metadata` | x shadow distance, y light size (PCSS), z PCSS quality 0–3, w view-height cut |
| `PointLightGpu` | `position_range` | xyz position, w range |
| | `color_intensity` | xyz colour, w intensity |
| `DynamicLightsGpu` | `light_count` | x active count |
| | `lights` | `[PointLightGpu; 64]` |
| `MarkerGpu` | `min`, `max` | xyz box corners in world metres, w unused |
| | `color` | linear RGBA, straight alpha |
| `Vertex` | `position`, `normal` | `Float32x3` at locations 0, 1 |
| | `ao`, `geometry_type` | `Float32`, `Uint32` at locations 2, 3 |
| | `light_level`, `block_light_rgb`, `sky_exposed` | `Float32`, `Float32x3`, `Float32` at locations 10, 11, 12 |
| | `material` | `Uint32` at location 13: material-table index; `u32::MAX` means draw in the instance's material |
| `InstanceGpu` | `model` | column-major mat4 |
| | `material`, `object_type` | `u32` material index, object kind |
| | `padding` | `[u32; 2]` |

Notes:

- `MaterialGpu.params[2] > 0` marks a material transparent (`is_transparent()`). Terrain
  materials reuse `params.xyz` for the side-face colour; `params.w > 0` bypasses lighting.
- `LightingGpu.params.y` is the debug view mode set by `r_debug_view`
  ([console commands](console-commands.md)).
- `Renderer::update_lighting` overwrites the three sky palette fields from `params.x`
  (`moho_renderer/src/sky.rs`); a game's values there are ignored. The skybox and the
  main fragment shader read the same fields.
- View-height cut (`r_view_height`): `view_height_uniform` maps `None` and non-finite
  heights to `f32::MAX` (no cut, not infinity: WGSL may assume none). The main and
  marker fragment shaders discard world y above `CameraGpu.cam_pos.w` (the marker shader adds the `marker_inflate` override constant, `MARKER_INFLATE`, so a box grown onto the cut keeps its top); the shadow pass
  has a fragment stage that discards on `metadata.w`, so shadows follow drawn geometry.
- Shadow constants (`shadow.rs`): map size 4096, shadow distance 1000.
  `MAX_SHADOW_LIGHTS` = 4, `MAX_DYNAMIC_LIGHTS` = 64.

## Changing a struct

1. Change the Rust type first and keep every field `vec4`-sized.
2. Update the matching WGSL struct in `shaders/*.wgsl` in the same change.
3. Update the byte size here and in any `size_of` assertion.
4. Run `just test -E 'package(moho_renderer)'`.
5. Upload with `bytemuck::cast_slice`; never hand-roll byte copies.
