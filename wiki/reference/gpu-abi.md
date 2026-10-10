# GPU ABI (Rust ↔ WGSL)

**Source of truth:** the Rust structs. WGSL mirrors them.

| Rust | WGSL |
|---|---|
| `moho_renderer/src/gpu_types.rs` | `shaders/common.wgsl` |
| `moho_render_api/src/material.rs` (`MaterialGpu`) | `Material` in `common.wgsl` |
| `moho_render_api/src/instance.rs` (`InstanceGpu`) | `InstanceIn` vertex inputs |
| `moho_render_api/src/ui_paint.rs` (`UiVertex`) | vertex inputs in `shaders/ui.wgsl` |

All structs are `#[repr(C)]`, `bytemuck::{Pod, Zeroable}`, and built from `vec4`-sized
fields so std140/std430 padding never differs between the two sides. Size matches
`size_of::<T>()`.

## Sizes

| Rust type | Size | Bound as |
|---|---|---|
| `CameraGpu` | 80 B (5 × vec4) | uniform, group 0 binding 0 |
| `MaterialGpu` | 32 B (2 × vec4) | storage array, group 0 binding 1 |
| `LightingGpu` | 96 B (6 × vec4) | uniform, group 0 binding 2 |
| `DynamicLightsGpu` | 2064 B (16 + 64 × 32) | storage, group 0 binding 5 |
| `MultiLightShadowGpu` | 288 B (4 × mat4 + 2 × vec4) | uniform, group 1 binding 0 |
| `PointLightGpu` | 32 B (2 × vec4) | element of `DynamicLightsGpu` |
| `ShadowMatrixGpu` | 64 B | legacy single shadow map |
| `InstanceGpu` | 80 B (mat4 + 4 × u32) | per-instance vertex buffer |
| `UiVertex` | 20 B (f32×2 pos, f32×2 uv, u8×4 colour) | UI pass vertex buffer |

Group 0 also binds the SSAO texture and sampler (bindings 3, 4). Group 1 binds the
shadow map array (`texture_depth_2d_array`, binding 1), a comparison sampler (2), and a
nearest sampler for the PCSS blocker search (3).

## Field layouts

| Struct | Field | Contents |
|---|---|---|
| `CameraGpu` | `vp0`–`vp3` | view-projection matrix columns |
| | `cam_pos` | xyz eye position |
| `LightingGpu` | `sun_direction` | xyz normalized direction, w intensity |
| | `sun_color` | xyz colour |
| | `moon_direction` | xyz normalized direction, w intensity |
| | `moon_color` | xyz colour |
| | `ambient` | xyz colour, w intensity |
| | `params` | x time of day 0–24, y debug mode |
| `MaterialGpu` | `albedo` | xyz albedo (terrain: top-face colour) |
| | `params` | xyz `[fuzz, ref_idx, transparent]` (terrain: side-face colour), w emissive |
| `MultiLightShadowGpu` | `light0`–`light3` | one mat4 each: Sun, Moon, Dynamic 1, Dynamic 2 |
| | `light_intensities` | x sun, y moon, z dyn 1, w dyn 2 |
| | `metadata` | x shadow distance, y light size (PCSS), z PCSS quality 0–3 |
| `PointLightGpu` | `position_range` | xyz position, w range |
| | `color_intensity` | xyz colour, w intensity |
| `DynamicLightsGpu` | `light_count` | x active count |
| | `lights` | `[PointLightGpu; 64]` |
| `InstanceGpu` | `model` | column-major mat4 |
| | `material`, `object_type` | `u32` material index, object kind |
| | `padding` | `[u32; 2]` |

Notes:

- `MaterialGpu.params[2] > 0` marks a material transparent (`is_transparent()`). Terrain
  materials reuse `params.xyz` for the side-face colour; `params.w > 0` bypasses lighting.
- `LightingGpu.params.y` is the debug view mode set by `r_debug_view`
  ([console commands](console-commands.md)).
- Shadow constants (`shadow.rs`): map size 4096, shadow distance 1000.
  `MAX_SHADOW_LIGHTS` = 4, `MAX_DYNAMIC_LIGHTS` = 64.

## Changing a struct

1. Change the Rust type first and keep every field `vec4`-sized.
2. Update the matching WGSL struct in `shaders/*.wgsl` in the same change.
3. Update the byte size here and in any `size_of` assertion.
4. Run `just test -E 'package(moho_renderer)'`.
5. Upload with `bytemuck::cast_slice`; never hand-roll byte copies.
