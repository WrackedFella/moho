# GPU ABI (Rust ↔ WGSL)

**Source of truth:** the Rust structs. WGSL mirrors them.

| Rust | WGSL |
|---|---|
| `moho_renderer/src/gpu_types.rs` | `shaders/common.wgsl` |
| `moho_render_api/src/material.rs` (`MaterialGpu`) | `Material` in `common.wgsl` |
| `moho_render_api/src/instance.rs` (`InstanceGpu`) | `InstanceIn` vertex inputs |
| `moho_render_api/src/ui_paint.rs` (`UiVertex`) | vertex inputs in `shaders/ui.wgsl` |
| `moho_render_api/src/marker.rs` (`MarkerGpu`) | vertex inputs in `shaders/marker.wgsl` |
| `moho_renderer/src/skinning.rs` (`SkinVertexGpu`, `SkinParamsGpu`) | `SkinVertex`, `SkinParams` in `shaders/skin.wgsl` |

All structs are `#[repr(C)]`, `bytemuck::{Pod, Zeroable}`, and built from `vec4`-sized
fields so std140/std430 padding never differs between the two sides. Size matches
`size_of::<T>()`. `UiVertex` and `Vertex` are the exceptions: they are only read as
vertex buffers, so they are packed with no padding (`UiVertex`: `Float32x2`, `Float32x2`,
`Uint32`, 20 B; `Vertex`: see the layout below, 64 B).

## Sizes

| Rust type | Size | Bound as |
|---|---|---|
| `CameraGpu` | 80 B (5 × vec4) | uniform, group 0 binding 0 |
| `MaterialGpu` | 32 B (2 × vec4) | storage array, group 0 binding 1 |
| `LightingGpu` | 144 B (9 × vec4) | uniform, group 0 binding 2 |
| `DynamicLightsGpu` | 2064 B (16 + 64 × 32) | storage, group 0 binding 5 |
| `ShadowGpu` | 496 B (6 × mat4 + 7 × vec4) | uniform, group 1 binding 0 (`Shadow` in `common.wgsl`) |
| `PointLightGpu` | 32 B (2 × vec4) | element of `DynamicLightsGpu` |
| `ToneMapGpu` | 16 B (f32 exposure, u32 enabled, f32 bloom_intensity, u32 pad) | uniform, tone-map pass group 0 binding 1 (`ToneMap` in `shaders/tonemap.wgsl`); binding 0 is the HDR texture, 2 the bloom texture (mip 0, or 1×1 black when Off), 3 its linear clamp sampler |
| `InstanceGpu` | 80 B (mat4 + 4 × u32) | per-instance vertex buffer |
| `MarkerGpu` | 48 B (3 × vec4) | per-instance vertex buffer, marker pass |
| `Vertex` (`moho_renderer/src/types.rs`) | 64 B (packed, 16 × 4 B) | per-vertex buffer, main pass |
| `UiVertex` | 20 B (f32×2 pos, f32×2 uv, u8×4 colour) | UI pass vertex buffer |
| `SkinVertexGpu` | 32 B (4 × u32 joints, 4 × f32 weights) | storage, skinning compute group 0 binding 1 |
| `SkinParamsGpu` | 16 B (4 × u32) | uniform, skinning compute group 1 binding 2 |

Group 0 also binds the SSAO texture and sampler (bindings 3, 4). Group 1 binds the
shadow map array (`texture_depth_2d_array`, binding 1), a comparison sampler (2), and a
nearest sampler for the PCSS blocker search (3). Group 2 (main pass only) binds the draw's
base-colour texture (`texture_2d<f32>`, `Rgba8UnormSrgb`, binding 0) and a filtering sampler (1);
untextured draws bind a 1×1 white texture (handle 0).

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
| `ShadowGpu` | `matrices` | six light view-projections, one per map layer: sun cascades 0–3, moon cascades 4–5 |
| | `split_far` | far end of each cascade by view depth: sun in `[0]`, moon in `[1].xy` |
| | `texel_world` | world size of one texel per cascade, packed like `split_far` |
| | `camera_forward` | xyz camera forward, w cascade blend fraction |
| | `light_intensities` | x sun, y moon |
| | `metadata` | x effective shadow distance, y light size (PCSS), z PCSS quality 0–3, w view-height cut |
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
| | `uv` | `Float32x2` at location 14, offset 56: base-colour coordinates; `[0, 0]` when the mesh has none |
| `SkinVertexGpu` | `joints`, `weights` | four joint indices into the placement's palette, and their weights |
| `SkinParamsGpu` | `vertex_count`, `first_vertex`, `first_joint`, `pad` | vertices to skin; arena index of the placement's first vertex; palette index of its joint 0 |
| `InstanceGpu` | `model` | column-major mat4 |
| | `material`, `object_type` | `u32` material index, object kind |
| | `padding` | `[u32; 2]` |

Notes:

- The skinning palette is a storage array of `vec4` (group 1 binding 0), three rows per
  joint `[m_r0, m_r1, m_r2, t_r]`; the arena (binding 1) and the mesh's vertex buffer (group 0
  binding 0) are read as `u32` words, `Vertex` size / 4 words per vertex. Position is words
  0-2 and normal words 3-5; reordering them in `Vertex` needs a `skin.wgsl` edit.
- `MaterialGpu.params[2] > 0` marks a material transparent (`is_transparent()`). Terrain
  materials reuse `params.xyz` for the side-face colour; `params.w > 0` bypasses lighting.
- `LightingGpu.params.y` is the debug view mode (0–8, 8 = cascade tint) set by `r_debug_view`
  ([console commands](console-commands.md)).
- `Renderer::update_lighting` overwrites the three sky palette fields from `params.x`
  (`moho_renderer/src/sky.rs`); a game's values there are ignored. The skybox and the
  main fragment shader read the same fields.
- View-height cut (`r_view_height`): `view_height_uniform` maps `None` and non-finite
  heights to `f32::MAX` (no cut, not infinity: WGSL may assume none). The main and
  marker fragment shaders discard world y above `CameraGpu.cam_pos.w` (the marker shader adds the `marker_inflate` override constant, `MARKER_INFLATE`, so a box grown onto the cut keeps its top); the shadow pass
  has a fragment stage that discards on `metadata.w`, so shadows follow drawn geometry.
- Shadow constants (`shadow.rs`): `CASCADE_MAP_SIZE` 4096 per layer, `SHADOW_DISTANCE` 1000,
  `SUN_CASCADES` 4, `MOON_CASCADES` 2, `CASCADE_LAYERS` 6. `SUN_CASCADES` and
  `MOON_CASCADES` are also declared in the shaders; a test keeps them equal.
  `MAX_DYNAMIC_LIGHTS` = 64. Point lights do not cast shadows.

## Changing a struct

1. Change the Rust type first and keep every field `vec4`-sized.
2. Update the matching WGSL struct in `shaders/*.wgsl` in the same change.
3. Update the byte size here and in any `size_of` assertion.
4. Run `just test -E 'package(moho_renderer)'`.
5. Upload with `bytemuck::cast_slice`; never hand-roll byte copies.
