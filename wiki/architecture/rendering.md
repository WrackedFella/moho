# Rendering

**Source:** `moho_render_api/src/` (`world_geometry.rs`), `moho_renderer/src/` (`scene.rs`,
`world_meshes.rs`, `markers.rs`, `renderer.rs`, `render_ops/`, `shadow.rs`, `ssao.rs`, `ui_pass/`), `moho_render_api/src/ui_paint.rs`, `moho_render_api/src/marker.rs`, `flagstone/src/app/world_geometry.rs`, `flagstone/src/app/cut_caps.rs`, `flagstone/src/app/view_level.rs`, `flagstone_voxel/src/mesh/cap.rs`,
`shaders/`, `moho_level/src/`.
**Decisions:** [ADR-0001](../../_todo/adr/0001-render-api-boundary.md),
[ADR-0013](../../_todo/adr/0013-ui-reaches-the-renderer-as-paint-data.md).

## 1 Boundary

The renderer never names a game type. Game types implement `Renderable` and
`RenderMaterial` from `moho_render_api`, and the renderer only sees `InstanceGpu` and
`MaterialGpu`.

```mermaid
flowchart LR
    subgraph game["flagstone_game (flagstone line)"]
        sph["Sphere, Cube"]
    end
    subgraph api["moho_render_api (engine)"]
        R["trait Renderable<br/>to_instance_with_material"]
        M["trait RenderMaterial<br/>to_gpu, dedup_key"]
        IG["InstanceGpu (80 B)"]
        MG["MaterialGpu (32 B)"]
    end
    subgraph rend["moho_renderer (engine)"]
        Sc["Scene::render&lt;S, C&gt;"]
        MT["MaterialTable<br/>dedup by bit pattern"]
        BE["dyn RendererBackend"]
    end
    sph -. implements .-> R & M
    R --> IG
    M --> MG
    Sc --> MT --> MG
    Sc --> BE
    IG --> BE
```

`Scene::render` is generic over the actor types, so `moho_renderer` compiles without
`flagstone_game`.

## 2 World geometry

World geometry reaches the renderer as meshes ([ADR-0010](../../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)),
not as voxel types. `moho_render_api::WorldMesh` is an owned indexed mesh with one
`Vec` per vertex channel (position, normal, ao, light rgb, sky exposure, surface);
`WorldMesh::new` refuses mismatched channel lengths, out-of-range indices and partial
triangles (`WorldMeshError`). A `WorldMeshId` is the game-chosen stable key. An optional
per-vertex `materials` channel (`with_materials`, one material-table index per vertex,
one material per triangle) lets a single mesh draw in several materials; without it
every vertex takes the `upsert` material. The renderer uploads the channel as `Vertex::material`,
and `shaders/vertex.wgsl` uses it unless it is `u32::MAX`, which defers to the instance's
material ([GPU ABI](../reference/gpu-abi.md)).

`moho_renderer::WorldMeshes` (reached via `Scene::world_meshes_mut`) queues
`upsert(id, mesh, material_idx)` and `remove(id)`. `Scene::render` calls `flush`
first: replaced or removed meshes are unregistered from the backend, new ones are
registered, and an empty mesh counts as a remove. Opaque world meshes are drawn one
draw per mesh in id order.

The binary adapts voxel chunks in `flagstone/src/app/world_geometry.rs`: `chunk_mesh_id` packs
the chunk position at 21 bits per signed axis, `chunk_world_mesh` converts a
`VoxelChunk`, and `insert_chunk` / `remove_chunk` keep the `ChunkStore`, the
renderer and the physics colliders in step, along with the chunk's [cut-face cap](#3-cut-face-caps). The same `WorldMesh` and `WorldMeshId` go to
`PhysicsWorld::set_world_mesh` / `remove_world_mesh` (`moho_physics/src/world.rs`), which
holds one static trimesh collider per id; an empty mesh holds none. A refused mesh is
logged and the chunk is neither drawn nor solid. When a chunk carries per-vertex voxel
materials, `TerrainPalette` (same file) maps each voxel material id to its material-table
index and `apply` writes the result into the mesh's `materials` channel.

Each registration carries a `HitTag`, the game's own meaning for the surface (material,
team, body part), which the engine stores and never interprets. `PhysicsWorld::cast_ray`
(`moho_physics/src/world/query.rs`) takes a `RayQuery` and returns the nearest tagged
surface as a `RayHit` (point, normal facing back against the ray, distance, tag). Untagged
colliders, such as movement capsules and props, never stop a ray; `RayQuery::ignore` skips
one character. It reads the world as the last `step` left it. `PhysicsWorld::collider_tag`
turns a collider handle, such as the one a movement `Impact` reports, into its tag. The adapter tags chunks with
`TERRAIN_TAG` in `world_geometry.rs`.

`PhysicsWorld::add_hit_volume` (`moho_physics/src/world/character.rs`) attaches a tagged
`HitVolume` box, offset and sized in the character's body frame, for per-body-part hits.
Volumes are sensors parented to the character's body: they follow its position and yaw
(never pitch) on `step`, are removed with the character, never block movement or props,
and only rays see them. A ray ignoring a character skips its volumes too.

### 2.1 Level files

`moho_level::load(path)` reads a `.gltf` or `.glb` file into a `Level`: one `LevelMesh`
(a `WorldMesh` plus the glTF material name) per triangle primitive reachable from the
default scene, plus a `LevelMarker` per named node without a mesh
(`moho_level/src/lib.rs`, `moho_level/src/marker.rs`). Rules:

- node transforms are baked into positions and normals; a mirroring transform flips
  triangle winding;
- positions and normals are required; ao, light, sky exposure and surface take neutral
  values (1, 0, 1, 0);
- buffers come from the GLB blob or a relative file URI beside the level; absolute
  paths and URL schemes are refused;
- non-triangle primitives are skipped with a warning; a node reached twice is an error;
- markers (spawns, loot points and the like) carry name, world position, world rotation
  (scale dropped) and `properties`, the node's `extras` object flattened to strings
  (non-string values become compact JSON; non-object or invalid extras are ignored with
  a warning). They arrive in scene-walk order; the loader gives them no meaning, game
  code interprets them;
- every failure is a `LevelError` naming the file, with a `LevelErrorKind`; no `gltf`
  type appears in the API.

The loader does not register meshes: the caller assigns `WorldMeshId`s and sends each
mesh to `WorldMeshes` and `PhysicsWorld` as above.

## 3 Cut-face caps

The strategy HUD's view-height slider (`RtsHud`, `ViewHeightHud`) chooses the cut as a block
layer. `flagstone/src/app/view_level.rs` (`ViewLevel`, `LayerExtent`) holds the pick: the
extent is the layer span of the loaded chunks (`floor + 1 ..= top`), the top means no cut, and
the cut sits `CUT_ABOVE_LAYER` over the layer boundary. `Game::frame` takes the pick from
`EguiAdapter::take_hud_intents` and calls `view_level::update`, which sends a changed cut to
the caps (`set_cut`) and the renderer (`RenderRequest::ViewHeight`). In first person the
renderer gets no cut. `update_hud_data` feeds the current level back to the slider.

The view-height cut discards geometry above the plane and leaves the ground hollow.
The binary closes it with flat caps built from the voxels, not by the renderer.
`flagstone/src/app/cut_caps.rs` (`CutCaps`) follows the cut: `view_level::update` and
`SetViewHeight` set it (`set_cut`), chunk insert and upsert mark the chunk dirty, remove drops its cap at once, and
`FrameProcessor::update_cut_caps` runs `refresh` each frame before lighting and render.
`refresh` rebuilds caps only for dirty chunks, plus, when the cut moved, every chunk in
`cap_chunk_layers` and every chunk already capped. A cap is an ordinary world mesh queued
under `cap_mesh_id` (the chunk's id with bit 63 set) with the cut-face material
(`register_cut_face_material`: terrain dirt on both faces) as its default; each cap triangle
carries the face material of the ground it cuts, picked by the same nearest-solid-block
rule as the walls (`nearest_solid_corner`), so cap and wall edges line up. Caps get no collider and are
removed with their chunk or when the cut clears.

`flagstone_voxel::cut_cap_mesh` (`mesh/cap.rs`) builds the `+Y` face at `y = cut` over
the cells the chunk's own mesher drew, at the chunk's LOD, so the cap meets the walls and
neighbouring caps tile. Solidity is sampled at `cut + CAP_PROBE`. A marker whose top sits
on the cut keeps its lid: `shaders/marker.wgsl` discards above the cut plus the
`marker_inflate` pipeline constant, set from `MARKER_INFLATE`.

## 4 Submission

`Scene::render` (called from the redraw handler) does CPU-side collection, then calls
the backend in a fixed sequence:

```mermaid
sequenceDiagram
    participant S as Scene::render
    participant B as RendererBackend
    S->>S: prepare: collect instances, dedup materials,<br/>split opaque / transparent
    S->>B: begin_frame(camera)<br/>upload camera, cull lights, update shadow matrices,<br/>acquire surface
    S->>B: enqueue_draw(cubes, spheres, each world mesh)
    S->>B: enqueue_draw(transparent, sorted back-to-front)
    S->>B: enqueue_markers(fills, outlines, each sorted far to near)
    S->>B: submit_frame()
```

`begin_frame` returns `FrameError` if the surface can't be acquired; the frame is
skipped with a warning.

## 5 Pass order

`submit_frame` flattens all pending draws into one instance buffer, then records one
command encoder:

```mermaid
flowchart LR
    A["flatten instances<br/>upload to instance buffer"] --> B["shadow passes<br/>shadow_ops"]
    B --> C["main pass<br/>skybox, then geometry<br/>main_pass_ops"]
    C --> K["marker pass<br/>marker_pass_ops<br/>only when markers exist"]
    K --> D["SSAO compute<br/>from depth"]
    D --> U["UI pass<br/>draw_ui"]
    U --> E["finish_frame<br/>submit, present"]
```

The egui UI is drawn by the UI pass from paint data (below); `finish_frame` only submits
and presents. Shader sources are `shaders/*.wgsl` (`common`, `vertex`,
`fragment`, `shadow`, `skybox`, `gtao`, `ssao_blur`, `pcss`, `instance`, `ui`, `marker`).

Present mode: the surface starts on `Fifo` and the renderer keeps the present modes the
surface supports. `RendererBackend::set_vsync(on)` picks the mode with
`device::choose_present_mode` and reconfigures the surface, the same path as `resize`.
On gives `Fifo`. Off gives `Immediate`, else `Mailbox`, else `Fifo` with a fallback
reason, which `Renderer::set_vsync` logs at `warn`. `FifoRelaxed` is never chosen. The
trait method is a no-op by default.

### 5.1 Light composition

The last lighting step in `shaders/fragment.wgsl` combines the two per-vertex light
channels:

```wgsl
color = color * max(sky_exposed, MIN_SKY_AMBIENT)
      + albedo * ao * block_light_rgb * BLOCK_LIGHT_GAIN
```

Sky exposure gates everything lit before this step, sun, moon, ambient and point lights
(floor `MIN_SKY_AMBIENT` = 0.1); block light
adds on top, so torches and emissive blocks light caves and night surfaces the sky
leaves dark. A surface with no block light (entity meshes, level files) draws as sky
exposure alone. Block light next to a strong emitter can push a channel past 1;
`BLOCK_LIGHT_GAIN` (1.0) is the tuning knob.

## 6 Markers

Markers are axis-aligned boxes a game places over the world (selection and mining
marks). `moho_render_api::Marker` holds `min`, `max` (world metres), a linear RGBA colour
with straight alpha and a `MarkerStyle`: `Fill` (translucent solid, the default) or
`Outline` (the twelve edges only). `Marker::new` orders the corners per axis and makes a
fill; `with_style` changes the style. A
`MarkerId` is the game-chosen stable key.

`moho_renderer::Markers` (reached via `Scene::markers_mut`) holds the live set in id
order: `upsert`, `remove`, `clear`. Markers persist until removed. `Scene::render` turns
them into `MarkerDraws` (`fills` and `outlines` lists of `MarkerGpu` instances; each box
grown by `MARKER_INFLATE` per side so a marker on a block face does not z-fight it, each
list sorted far to near by box centre) and passes both to `RendererBackend::enqueue_markers`.

`submit_frame` draws them in a separate pass after the main pass: colour and depth are
loaded, not cleared, so markers are depth-tested against the world; the pipeline writes no
depth, which keeps them out of the SSAO pass. One unit cube is instanced per marker and
scaled to its box in `shaders/marker.wgsl`. Fills draw first as a triangle list, then
outlines through a second pipeline over the same shader as a line list on the cube's edge
indices. Layout: [GPU ABI](../reference/gpu-abi.md).
The console drives them with `marker` and `marker_clear`
([console](console.md)).

## 7 UI paint data

UI reaches the renderer as plain data ([ADR-0013](../../_todo/adr/0013-ui-reaches-the-renderer-as-paint-data.md)),
not as wgpu or egui types. `moho_render_api::UiFrame` holds `pixels_per_point`, texture
deltas (`UiTextureSet` before drawing, `UiTextureId` frees after) and `UiMesh`es
(clipped, textured triangle lists of `UiVertex`, in points). A game implements
`UiFrameSource` (`flagstone_ui`'s `EguiAdapter` and `moho_ui_shell`'s `UiShell` map egui tessellation through `moho_ui_shell/src/paint_map.rs`)
and registers a `UiSource` with `RendererBackend::set_ui_source`; the
renderer pulls `Option<UiFrame>` once per frame with the surface size. Several sources reach
the one slot through `moho_ui_shell::UiLayers`, which merges them in order, later on top. The adapter
consumes egui's `TexturesDelta`; `textures_set` keeps the order of partial updates
to one texture, so patches apply after the full upload they follow.

`moho_renderer::ui_pass` draws it over the finished scene, before `finish_frame`. The pure
logic (`scissor`, `build_draws`, texture bookkeeping) is separate from the wgpu half
(`ui_pass/gpu.rs`). Meshes with an empty scissor, no indices or an unknown texture are
skipped; a non-finite or non-positive `pixels_per_point` skips the whole frame. The
painter and `shaders/ui.wgsl` are a port of `egui-wgpu` 0.34 (unchanged through 0.36): premultiplied-alpha
blending, sRGB vertex colours, no custom paint callbacks.

## 8 Tunables

| Setting | Where | Notes |
|---|---|---|
| Shadow quality 0–4 | `graphics.shadow_quality`, `r_shadow_quality` | Off/Low/Med/High/Ultra |
| SSAO quality 0–4 | `graphics.ssao_quality`, `r_ssao_quality` | Samples 0/4/8/16/32 |
| Debug view | `r_debug_view <mode>` | `Lighting.params.y` |
| View-height cut | `r_view_height <metres\|off>` | `Renderer::set_view_height`; sticky; cuts world, markers and shadows; the binary caps the cut ([below](#3-cut-face-caps)) |
| Shadow map size | `SHADOW_MAP_SIZE` = 4096 | `shadow.rs` |
| Shadow distance | `SHADOW_DISTANCE` = 1000 | `shadow.rs` |

CPU↔GPU struct layouts: [GPU ABI](../reference/gpu-abi.md).
