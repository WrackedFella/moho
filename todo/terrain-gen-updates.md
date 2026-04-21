# Terrain Generation Overhaul

**Branch:** `exp/terrain`
**Goal:** Replace the current 2D heightmap + flat HashMap storage with a scalable, idempotent,
3D-density-field terrain engine that supports large bounded worlds, distance LOD, smooth
marching-cubes surfaces, biomes, and a clean extension path for caves, ores, and player scanning.

---

## Context & Motivation

The current system (`moho_core/src/scene_builders.rs`, `moho_core/src/voxel/grid.rs`) has
several architectural limits that must be addressed before the engine can support the target
mining game:

- **Storage:** `VoxelGrid` uses `HashMap<BlockPos, VoxelBlock>` — per-entry overhead (~80 bytes
  in Rust's HashMap) makes large worlds untenable. A 2048×2048 surface world at 10 blocks deep
  is ~400M blocks × 80 bytes = ~32 GB.
- **Generation:** 2D heightmap only — no caves possible, no underground variation.
- **Idempotency bug:** `determine_resource_id` calls `rng().random()` (unseeded) — same seed
  produces different ore layouts every run. Must be fixed.
- **No LOD:** Every chunk is rendered at full 16³ resolution regardless of distance.
- **Biome system:** `TerrainType` is a single global enum applied uniformly — no regional
  variation, no blending.
- **Ore placement:** Block-aligned, seeded via noise (after fixing idempotency bug), with a
  toggle between `BlockAligned` (all sub-voxels inherit parent material) and `Geological`
  (sub-voxel noise sampling at MicroChunk conversion time). Default: `Geological`.

---

## Architecture Overview

```
TerrainFunction (pure, seed-deterministic)
    ├── BiomeMap: (x,z,seed) → [(BiomeType, weight)]   ← 2D Voronoi noise
    ├── DensityField: (x,y,z,seed,biome_params) → f32  ← surface + cave carver
    ├── MaterialLayer: density + depth → MaterialId
    └── OreField: (x,y,z,seed,biome) → Option<OreType> ← per-vein 3D noise blob

PalettedChunk (replaces flat HashMap entries per chunk)
    ├── palette: Vec<u8> (up to 16 material IDs, expandable to 256)
    └── indices: [u8; 4096]  (16×16×16, 4-bit or 8-bit index per block)

ChunkCache (loaded radius only)
    ├── loaded: HashMap<ChunkPos, PalettedChunk>
    └── dirty: HashSet<ChunkPos>

LOD Tiers
    ├── LOD 0 (< 4 chunks): 16³ paletted, hybrid mesh (marching cubes surface)
    ├── LOD 1 (4–16 chunks): 32³ coarse (1 sample per 2-block cell), blocky mesh
    └── LOD 2 (> 16 chunks): heightmap impostor only — deferred
```

**Invariant:** The terrain function is pure — it reads only `(x, y, z, seed, config)` and
never the grid. Idempotency is structurally guaranteed. Ores can be queried without loading
chunks (enables future scanning mechanic).

---

## Implementation Phases

### Phase 1 — Fix Idempotency Bug (prerequisite, do first)

**File:** `moho_core/src/scene_builders.rs:299–310`

`determine_resource_id` calls `rng().random::<f32>()` — this is non-deterministic regardless
of seed. Replace with a seeded hash or noise sample.

- Replace `rng().random()` with a positional hash: `hash3(x, y, z, seed) % 100 < threshold`
  using a fast integer hash (e.g., xxHash or a simple pcg-style mix).
- Apply the same fix to any other site that calls `rng()` during terrain generation.
- Add a regression test: generate the same world twice with the same seed, assert block-for-block
  equality of the resulting `VoxelGrid`.

---

### Phase 2 — Biome System

**Files:** `moho_core/src/scene_builders.rs`, new `moho_core/src/voxel/biome.rs`

Migrate `TerrainType` → `BiomeType` with per-biome terrain parameters.

**New types:**

```rust
// moho_core/src/voxel/biome.rs
pub enum BiomeType {
    GentleHills,
    Mountains,
    Plains,
    Cliffs,
    Canyon,
    // Future: Forest, River, Desert, ...
}

pub struct BiomeParams {
    pub surface_amplitude: f32,   // height variation scale
    pub surface_frequency: f64,   // noise frequency for surface
    pub octaves: u32,
    pub cave_density: f32,        // 0.0 = no caves (current default)
    pub surface_material: u32,    // material ID for top layer
    pub subsurface_material: u32, // material ID for sub-surface layer
    pub base_material: u32,       // material ID for deep rock
}

impl BiomeType {
    pub fn params(&self) -> BiomeParams { ... }
}
```

**Changes to `TerrainConfig`:**

```rust
pub struct TerrainConfig {
    pub seed: u32,
    pub world_size: u32,
    pub enabled_biomes: Vec<BiomeType>,  // replaces terrain_type: TerrainType
    pub ore_layout: OreLayout,           // BlockAligned | Geological
    // cave_scale: f32 — deferred, always 0.0 until caves are enabled
}

pub enum OreLayout {
    BlockAligned,  // All sub-voxels inherit parent material (fast)
    Geological,    // Sub-voxels sampled individually at MicroChunk conversion (default)
}
```

**Biome map:** A low-frequency 2D Simplex noise field maps XZ positions to the nearest enabled
`BiomeType`. For now, a single biome per world position (no blending). Blending between biome
params at boundaries is a future enhancement.

---

### Phase 3 — 3D Density Field Generator

**File:** `moho_core/src/scene_builders.rs` (replace `generate_terrain` and `sample_height`)

Replace the 2D heightmap loop with a 3D density field evaluated per block.

**Density function:**

```rust
fn density(x: i32, y: i32, z: i32, seed: u32, params: &BiomeParams) -> f32 {
    let surface_h = sample_surface_height(x, z, seed, params); // existing FBM logic
    let base_density = (surface_h as f32) - (y as f32);        // >0 underground, <0 above
    // Cave carver (always 0.0 scale until Phase 6):
    // base_density + cave_noise(x, y, z, seed) * params.cave_density
    base_density
}

// Block is solid if density > 0.0
fn is_solid(x: i32, y: i32, z: i32, seed: u32, params: &BiomeParams) -> bool {
    density(x, y, z, seed, params) > 0.0
}
```

**Ore field (stub):**

```rust
fn ore_at(x: i32, y: i32, z: i32, seed: u32, _biome: &BiomeType) -> Option<OreType> {
    None  // Populated in a future phase
}
```

This function already has the correct signature for the scan mechanic. No behavior yet.

**Material determination:** Driven by density gradient depth (same logic as current
`determine_material_id`, but using biome's material IDs from `BiomeParams`).

---

### Phase 4 — Paletted Chunk Storage

**File:** `moho_core/src/voxel/grid.rs` (significant rewrite of storage layer)

Replace `HashMap<BlockPos, VoxelBlock>` with `HashMap<ChunkPos, PalettedChunk>`.

**`PalettedChunk`:**

```rust
pub struct PalettedChunk {
    palette: Vec<u8>,        // material IDs used in this chunk (max 16 initially)
    indices: [u8; 4096],     // 16×16×16 block indices into palette (0 = air/empty)
    pub sky_light: [u8; 4096],
    pub block_light: [u8; 4096],
    dirty: bool,
}
```

- Index 0 in palette is always air (no data stored for all-air chunks — `None` in the outer map).
- Encoding `(x, y, z)` into flat index: `x + y*16 + z*256` (chunk-local coords 0..15).
- `PalettedChunk::get(lx, ly, lz) -> u8` returns material ID.
- `PalettedChunk::set(lx, ly, lz, material_id)` updates palette and index.
- All-air chunks are stored as `None` in the outer `HashMap<ChunkPos, Option<PalettedChunk>>`.

**`VoxelGrid` API** stays the same at the call sites:
- `grid.set_block(pos, block)` — converts to chunk-local coords, writes into `PalettedChunk`
- `grid.get_block(pos)` — looks up chunk, reads palette entry, returns lightweight view

The `VoxelBlock` struct (with full `VoxelMesh` inline) is replaced by a lightweight query result.
Light data moves into the `PalettedChunk` arrays.

**Migration:** `LightSystem`, `LightPropagator`, `MeshGenerator`, and `scene_builders.rs` all
call `grid.get_block()` / `grid.set_block()` / `grid.iter_blocks()`. These call sites do not
change — the new storage is behind the same API.

---

### Phase 5 — Chunk Streaming

**Files:** `moho_core/src/voxel/grid.rs`, `src/app/event_loop/frame_processor.rs`,
`src/main.rs`

Generate chunks on demand as the player moves; evict chunks beyond the unload radius.

**Parameters (in `TerrainConfig` or a new `StreamingConfig`):**

```rust
pub struct StreamingConfig {
    pub load_radius_chunks: u32,    // default 8  (128 blocks)
    pub unload_radius_chunks: u32,  // default 12 (192 blocks)
}
```

**Per-frame logic (in `frame_processor.rs`):**
1. Compute player chunk position from `simulation.position()`.
2. For each chunk within `load_radius` not in `VoxelGrid.loaded`: call `generate_chunk(pos)`
   using the terrain function (Phase 3), insert into cache, mark dirty for meshing.
3. For each loaded chunk beyond `unload_radius`: serialize to disk (or drop if unmodified),
   remove from cache.

**Idempotency guarantee:** An unmodified chunk evicted and re-generated produces identical
block data because generation is a pure function.

**Modified chunks:** Track `modified: bool` on `PalettedChunk`. On eviction, if `modified`,
write the delta (full chunk bytes) to `saves/<world>/chunks/<cx>_<cy>_<cz>.bin`. On load,
check for a saved file first; fall back to generation if absent.

---

### Phase 6 — LOD System

**Files:** `moho_core/src/voxel/chunk.rs`, `moho_core/src/voxel/jobs.rs`,
`moho_renderer/src/scene.rs`

Two active LOD tiers (LOD 2 heightmap impostor deferred):

| Tier  | Distance        | Chunk res | Mesh strategy         |
|-------|----------------|-----------|----------------------|
| LOD 0 | < 4 chunks     | 16³       | Hybrid (marching cubes surface + blocky interior) |
| LOD 1 | 4–16 chunks    | Coarse (1 sample per 2-block cell → 8³ effective) | Blocky only |

**Implementation:**
- `VoxelChunk` gains an `lod: u8` field.
- `MeshGenerator::generate(chunk, lod)` dispatches to hybrid or blocky path.
- On distance change, dirty the affected chunks to trigger re-mesh at new LOD.
- Seam cracks between LOD tiers are covered by **skirts** — a few extra triangles extending
  downward from the chunk boundary edge. No neighbor data needed; skirts use the chunk's own
  edge column extruded downward by 1 block.

---

## What Is Explicitly Deferred

These items are architecturally accommodated by the above but not implemented:

| Feature | Hook in place | Phase to implement |
|---|---|---|
| Cave carver | `cave_density: f32 = 0.0` in `BiomeParams`, commented-out term in density fn | Future |
| Ore veins | `ore_at()` stub returns `None`; `OreLayout` enum ready | Future |
| Biome blending | Biome map returns single biome; `BiomeParams` lerp path exists | Future |
| River carver | Separate "carver" layer slot in density fn, disabled | Future |
| Micro-Grid deletion | Depends on Phase 4 (paletted chunks) landing first | Future |
| Player scan mechanic | Trivial once `ore_at()` is populated — no chunk load needed | Future |
| LOD 2 heightmap impostor | `lod: u8` field in `VoxelChunk` can hold value 2 | Future |

---

## Files to Modify

| File | Change |
|---|---|
| `moho_core/src/scene_builders.rs` | Replace `generate_terrain` / `sample_height` / `determine_resource_id`; add density field, ore stub, biome dispatch |
| `moho_core/src/voxel/grid.rs` | Replace `HashMap<BlockPos, VoxelBlock>` with `HashMap<ChunkPos, Option<PalettedChunk>>`; maintain existing public API |
| `moho_core/src/voxel/chunk.rs` | Add `lod: u8` field; update `MeshGenerator` dispatch |
| `moho_core/src/voxel/jobs.rs` | Pass LOD tier into mesh job; update job priority by distance |
| `moho_core/src/voxel/biome.rs` | **New file** — `BiomeType`, `BiomeParams`, `OreLayout`, biome map function |
| `moho_core/src/voxel/mod.rs` | Re-export new `biome` module |
| `src/app/event_loop/frame_processor.rs` | Add per-frame chunk load/unload streaming logic |
| `src/main.rs` | Pass `StreamingConfig` into terrain setup; remove upfront full-world generation loop |
| `moho_ui/src/screens/new_world.rs` | Replace `terrain_type` dropdown with `enabled_biomes` checkbox list |

---

## Verification

1. **Idempotency:** `cargo test --package moho_core same_seed_produces_identical_grid`
   — generate world twice with seed 42, assert every block position and material matches.
2. **Biome dispatch:** Generate with `enabled_biomes: [Mountains]` only — verify no plains
   terrain appears anywhere.
3. **Paletted storage memory:** Generate a 512×512 world, measure RSS before and after — should
   be under 100 MB active (vs. multi-GB with old HashMap at that scale).
4. **Streaming:** Walk the player from one edge of the world to the other; verify chunks load
   ahead and distant chunks are evicted from `VoxelGrid.loaded`.
5. **LOD seams:** Stand at LOD boundary distance; verify no visible cracks in terrain mesh.
6. **Saves compat:** Existing `saves/scene.bin` files — load should still work (scene format
   is unaffected; chunk streaming saves are a separate `saves/<world>/chunks/` tree).
