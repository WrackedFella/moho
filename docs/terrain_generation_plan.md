# Terrain Generation Implementation Plan

## Overview
Transform the current random scene generation into a terrain system with deformed cube tops simulating geographic features using Perlin noise.

---

## Current State Analysis

### Existing Components
- **Location**: `engine_core/src/scene_builders.rs`
- **Function**: `random_scene(world: &mut World)`
- **Behavior**:
  - Generates 1 large ground sphere (radius 1000)
  - Creates ~484 small random objects (spheres/cubes) in a 22×22 grid
  - 3 featured objects (1 cube, 2 glass spheres)
  - Random material assignment (80% Lambertian, 15% Metal, 5% Dielectric)

### Existing Mesh System
- **Cube mesh**: `actors::Cube::unit_cube_indexed()`
  - Returns `(Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>)` (vertices, normals, indices)
  - 24 vertices (4 per face), 36 indices
  - Centered at origin, spans [-0.5, 0.5] on each axis
  - Proper per-face normals for lighting

### Current Architecture
- **ECS**: Legion-based entity-component system
- **Renderables**: Sphere and Cube implement `Renderable` trait
- **Material System**: Supports Lambertian, Metal, Dielectric
- **Dependencies**: glam (math), rand (RNG), legion (ECS)

---

## Proposed Solution

### Goal
Create a terrain system using cubes with deformed top faces, where deformation is driven by Perlin noise to simulate hills, valleys, and geographic features.

---

## Implementation Plan

### Phase 1: Foundation Setup

#### Task 1.1: Add Noise Generation Library
**File**: `engine_core/Cargo.toml`

**Action**: Add dependency
```toml
noise = "0.9"
```

**Rationale**: 
- `noise` crate is the standard Rust library for procedural noise
- Lightweight (~50KB), well-maintained
- Provides Perlin, Simplex, and other noise algorithms
- Compatible with our existing dependencies

**Estimated Effort**: 5 minutes

---

#### Task 1.2: Create Dedicated Terrain Module
**File**: `engine_core/src/terrain.rs` (new file)

**Action**: Create module structure
```rust
use glam::Vec3;
use crate::materials::MaterialType;
use noise::{NoiseFn, Perlin};

pub struct TerrainTile {
    pub center: Vec3,
    pub size: f32,
    pub height_scale: f32,
    pub mat_ptr: MaterialType,
}

pub struct TerrainConfig {
    pub frequency: f64,
    pub octaves: u32,
    pub amplitude: f32,
    pub seed: u32,
}
```

**Also Update**: `engine_core/src/lib.rs`
```rust
pub mod terrain;
```

**Rationale**:
- Separates terrain logic from scene_builders
- Allows for future expansion (biomes, LOD, etc.)
- Keeps scene_builders focused on composition

**Estimated Effort**: 15 minutes

---

### Phase 2: Mesh Generation

#### Task 2.1: Implement Deformed Cube Mesh Generator
**File**: `engine_core/src/terrain.rs`

**Action**: Create `terrain_tile_mesh()` function
```rust
pub fn terrain_tile_mesh(
    center: Vec3,
    size: f32,
    config: &TerrainConfig,
    noise: &Perlin,
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    // Start with standard cube vertices
    // Sample Perlin noise for top face (vertices 8-11)
    // Deform Y coordinate based on noise
    // Recalculate normals for top face and adjacent faces
    // Return modified (vertices, normals, indices)
}
```

**Algorithm**:
1. **Base Cube**: Start with `Cube::unit_cube_indexed()` data
2. **Top Face Deformation** (vertices 8-11):
   ```rust
   let x_world = center.x + vertex.x * size;
   let z_world = center.z + vertex.z * size;
   let noise_value = noise.get([
       x_world as f64 * config.frequency,
       z_world as f64 * config.frequency
   ]);
   vertex.y += noise_value as f32 * config.amplitude;
   ```
3. **Normal Recalculation**:
   - Top face: Calculate cross product of triangle edges
   - Side faces touching top: Blend normals for smooth transition
   - Bottom and non-touching sides: Keep original normals

**Perlin Noise Parameters**:
- **Frequency**: 0.1 - 0.5 (lower = larger features)
- **Octaves**: 1-4 (multi-octave for detail)
- **Amplitude**: 0.2 - 1.0 (height variation)
- **Seed**: User-configurable for reproducible terrain

**Rationale**:
- Perlin noise provides smooth, natural-looking height variation
- Simple algorithm, efficient to compute
- Adjustable parameters for different terrain styles

**Estimated Effort**: 2-3 hours

---

#### Task 2.2: Implement TerrainTile Component
**File**: `engine_core/src/terrain.rs`

**Action**: Complete TerrainTile implementation
```rust
impl TerrainTile {
    pub fn new(
        center: Vec3,
        size: f32,
        height_scale: f32,
        mat: MaterialType,
    ) -> Self {
        TerrainTile {
            center,
            size,
            height_scale,
            mat_ptr: mat,
        }
    }
    
    pub fn generate_mesh(&self, config: &TerrainConfig) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        let noise = Perlin::new(config.seed);
        terrain_tile_mesh(self.center, self.size, config, &noise)
    }
}
```

**Rationale**:
- Encapsulates terrain tile data
- Mesh generation on-demand
- Consistent with existing actor patterns (Sphere, Cube)

**Estimated Effort**: 30 minutes

---

### Phase 3: Renderer Integration

#### Task 3.1: Add Renderable Trait for TerrainTile
**File**: `engine_core/src/actors.rs`

**Action**: Update Renderable infrastructure
```rust
impl Renderable for terrain::TerrainTile {
    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        // Similar to Cube, but may need custom mesh handle
        // For now, use cube mesh handle with appropriate transform
    }
}
```

**Also Update**: `collect_renderable_instances()`
```rust
pub fn collect_renderable_instances(world: &mut World) -> Vec<InstanceGpu> {
    // ... existing code for Sphere, Cube ...
    
    let mut qt = <&terrain::TerrainTile>::query();
    for t in qt.iter(world) {
        out.push(t.to_instance_with_material(0));
    }
    out
}
```

**Challenge**: Current renderer uses shared mesh handles (sphere_mesh, cube_mesh). TerrainTiles have custom geometry.

**Solutions** (pick one):
1. **Option A**: Register each unique terrain mesh with renderer (simple, but memory-intensive)
2. **Option B**: Use instanced rendering with vertex displacement in shader (efficient, but more complex)
3. **Option C**: Generate terrain mesh once, store in separate mesh handle (balanced approach)

**Recommended**: **Option C** - Register terrain mesh as a separate mesh handle during scene generation.

**Estimated Effort**: 1-2 hours

---

#### Task 3.2: Update Renderer Mesh Registration
**File**: `src/main.rs` (or wherever renderer is initialized)

**Action**: Register terrain mesh
```rust
// During setup
let terrain_config = terrain::TerrainConfig::default();
let (terrain_verts, terrain_normals, terrain_indices) = 
    terrain::generate_terrain_chunk_mesh(&terrain_config);
let terrain_mesh_handle = renderer.register_indexed_mesh(
    &terrain_verts,
    &terrain_normals,
    &terrain_indices
);
```

**File**: Scene rendering
```rust
// Use terrain_mesh_handle when rendering TerrainTile instances
```

**Challenge**: Current system assumes one mesh per object type. Terrain has variable geometry.

**Solution**: 
- Generate a representative terrain mesh at initialization
- OR: Switch to per-instance mesh handles (requires renderer refactor)

**Estimated Effort**: 1-2 hours

---

### Phase 4: Scene Generation

#### Task 4.1: Create Terrain Scene Builder
**File**: `engine_core/src/scene_builders.rs`

**Action**: Add `terrain_scene()` function
```rust
pub fn terrain_scene(world: &mut World) {
    let config = terrain::TerrainConfig {
        frequency: 0.2,
        octaves: 2,
        amplitude: 0.5,
        seed: 42,
    };
    
    // Generate grid of terrain tiles
    let grid_size = 20;
    let tile_size = 2.0;
    
    for x in -grid_size..grid_size {
        for z in -grid_size..grid_size {
            let center = Vec3::new(
                x as f32 * tile_size,
                0.0,
                z as f32 * tile_size,
            );
            
            let tile = terrain::TerrainTile::new(
                center,
                tile_size,
                1.0,
                MaterialType::Lambertian {
                    albedo: Vec3::new(0.3, 0.5, 0.3), // Green terrain
                },
            );
            
            world.push((tile,));
        }
    }
    
    log::info!("Terrain Generated");
}
```

**Features**:
- Configurable grid size
- Tile size controls detail level
- Single material for simplicity (can add biomes later)
- Perlin noise ensures continuity between tiles

**Rationale**:
- Simple grid-based approach for initial implementation
- Easy to extend with LOD, culling, etc.
- Maintains compatibility with existing scene system

**Estimated Effort**: 1 hour

---

### Phase 5: Integration & Testing

#### Task 5.1: Update Main Application
**File**: `src/main.rs`

**Action**: Add terrain generation option
```rust
// In generate_new_world() or similar
fn generate_new_world(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Generating new world...");
    self.world.clear();
    
    // Choose scene type
    // engine_core::scene_builders::random_scene(&mut self.world);
    engine_core::scene_builders::terrain_scene(&mut self.world);
    
    // ... rest of save logic ...
}
```

**Alternative**: Add command-line flag or settings option
```rust
match scene_type {
    SceneType::Random => engine_core::scene_builders::random_scene(&mut self.world),
    SceneType::Terrain => engine_core::scene_builders::terrain_scene(&mut self.world),
}
```

**Estimated Effort**: 30 minutes

---

#### Task 5.2: Parameter Tuning & Testing
**Action**: Experiment with noise parameters

**Test Matrix**:
| Frequency | Octaves | Amplitude | Expected Result |
|-----------|---------|-----------|-----------------|
| 0.05 | 1 | 0.3 | Gentle rolling hills |
| 0.1 | 2 | 0.5 | Medium terrain variation |
| 0.2 | 3 | 0.8 | Rocky, varied terrain |
| 0.5 | 4 | 1.0 | Sharp, dramatic features |

**Performance Testing**:
- Grid sizes: 10×10, 20×20, 40×40
- Measure frame rate, memory usage
- Optimize mesh generation if needed

**Visual Testing**:
- Verify normal calculations (lighting should look correct)
- Check tile continuity (no seams between tiles)
- Ensure materials apply correctly

**Estimated Effort**: 2-3 hours

---

## Alternative Approaches Considered

### Approach 1: Heightmap-based Terrain
**Description**: Generate a 2D heightmap, sample for each tile
**Pros**: More efficient for large terrains, easier LOD
**Cons**: More complex initial setup, requires texture sampling
**Decision**: **Not chosen** - Perlin noise is simpler for initial implementation

### Approach 2: Shader-based Displacement
**Description**: Deform vertices in vertex shader using noise texture
**Pros**: GPU-accelerated, very efficient
**Cons**: Requires shader modifications, more complex
**Decision**: **Not chosen** - Keep initial implementation CPU-side for simplicity

### Approach 3: Marching Cubes / Voxel Terrain
**Description**: Use voxel-based terrain with smooth surfaces
**Pros**: Allows caves, overhangs, destructible terrain
**Cons**: Much more complex, different rendering approach
**Decision**: **Not chosen** - Out of scope for current requirements

---

## Technical Challenges & Solutions

### Challenge 1: Mesh Handle System
**Problem**: Current renderer uses shared meshes (one mesh for all cubes). Terrain needs custom geometry.

**Solution**: 
- **Short-term**: Generate representative terrain mesh, use for all terrain tiles (accepts some visual repetition)
- **Long-term**: Implement per-instance mesh system or vertex displacement in shader

---

### Challenge 2: Tile Continuity
**Problem**: Adjacent terrain tiles must have matching edge heights to avoid gaps.

**Solution**: 
- Perlin noise is deterministic based on world position
- Sample noise at exact world coordinates, not local tile coordinates
- Edge vertices between tiles will sample same noise values → automatic continuity

```rust
// Correct: Use world coordinates
let noise_value = noise.get([
    (center.x + local_x) as f64 * frequency,
    (center.z + local_z) as f64 * frequency,
]);

// Wrong: Use local coordinates (creates discontinuities)
let noise_value = noise.get([
    local_x as f64 * frequency,
    local_z as f64 * frequency,
]);
```

---

### Challenge 3: Normal Recalculation
**Problem**: Deforming top face breaks pre-calculated normals, causing incorrect lighting.

**Solution**: Recalculate normals for affected faces
```rust
fn calculate_face_normal(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    edge1.cross(edge2).normalize()
}

// For top face
let top_normal = calculate_face_normal(
    verts[8], verts[9], verts[10]
);

// Update all 4 top vertices with same normal
normals[8] = normals[9] = normals[10] = normals[11] = top_normal;

// For side faces: blend with new top edge normals for smooth transition
```

---

### Challenge 4: Performance
**Problem**: Generating many custom meshes may be slow.

**Solutions**:
1. **Caching**: Store generated meshes, reuse for similar tiles
2. **Async Generation**: Generate terrain in background thread
3. **LOD**: Use simpler geometry for distant tiles
4. **Culling**: Only generate/render visible tiles

**Initial Implementation**: No optimization (keep simple), add if needed

---

## Performance Considerations

### Memory Estimates
- **Single Terrain Tile**: 
  - Vertices: 24 × 12 bytes = 288 bytes
  - Normals: 24 × 12 bytes = 288 bytes
  - Indices: 36 × 4 bytes = 144 bytes
  - **Total per tile**: ~720 bytes

- **20×20 Grid**: 400 tiles × 720 bytes = ~280 KB (negligible)
- **40×40 Grid**: 1,600 tiles × 720 bytes = ~1.1 MB (still fine)

### CPU Impact
- Noise generation: O(n) per vertex, very fast (~10 µs per tile)
- Normal calculation: O(1) per face, trivial
- **Total generation time**: ~1-5 ms for 400 tiles (imperceptible)

### GPU Impact
- Each tile is a separate instance (same as current cubes)
- No additional shader complexity
- **Rendering cost**: Same as current random scene

---

## Testing Strategy

### Unit Tests
**File**: `engine_core/src/terrain.rs`
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_terrain_tile_creation() {
        // Verify TerrainTile initializes correctly
    }
    
    #[test]
    fn test_mesh_generation() {
        // Verify mesh has correct vertex/index counts
    }
    
    #[test]
    fn test_noise_determinism() {
        // Same input should produce same output
    }
    
    #[test]
    fn test_tile_continuity() {
        // Adjacent tiles should have matching edge heights
    }
}
```

### Integration Tests
1. **Visual Inspection**: Load terrain scene, verify appearance
2. **Performance**: Measure frame rate with different grid sizes
3. **Material Application**: Verify materials render correctly
4. **Lighting**: Check normal calculations via lighting behavior

---

## Future Enhancements (Out of Scope)

1. **Biomes**: Vary materials based on height/position
2. **LOD System**: Simplify distant terrain
3. **Vegetation**: Add trees, grass using instancing
4. **Water**: Flat plane with transparency
5. **Caves**: Use 3D noise, negative space
6. **Erosion Simulation**: Post-process noise for realistic features
7. **Texture Mapping**: Multi-texture splatting based on slope/height
8. **Dynamic Terrain**: User-modifiable landscape

---

## Estimated Total Effort

| Phase | Time |
|-------|------|
| Phase 1: Foundation | 20 min |
| Phase 2: Mesh Generation | 3-4 hours |
| Phase 3: Renderer Integration | 2-3 hours |
| Phase 4: Scene Generation | 1 hour |
| Phase 5: Testing & Tuning | 2-3 hours |
| **Total** | **8-11 hours** |

---

## Decision Points for Review

### 1. Noise Library Choice
**Question**: Use `noise` crate or implement custom Perlin noise?
**Recommendation**: Use `noise` crate (mature, tested, minimal dependency cost)

### 2. Mesh Handling Strategy
**Question**: How to handle custom terrain geometry?
**Options**:
- A) Register single representative mesh (simple, some repetition)
- B) Per-instance mesh handles (flexible, more complex)
- C) Shader-based displacement (efficient, requires shader work)

**Recommendation**: Start with **Option A**, migrate to C if needed

### 3. Scene Generation Approach
**Question**: Replace random_scene() or add new terrain_scene()?
**Recommendation**: Add `terrain_scene()` as separate function, keep both available

### 4. Tile Grid Size
**Question**: How large should default terrain be?
**Recommendation**: 20×20 grid (400 tiles), configurable via parameter

### 5. Noise Parameters
**Question**: What default values for frequency/amplitude?
**Recommendation**: 
- Frequency: 0.2
- Octaves: 2
- Amplitude: 0.5
- (Tunable during testing phase)

---

## Risks & Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Mesh rendering issues | High | Medium | Test with simple cube first, verify rendering pipeline |
| Performance degradation | Medium | Low | Profile early, optimize if needed |
| Visual artifacts (seams) | Medium | Medium | Test tile continuity, adjust noise sampling |
| Normal calculation errors | High | Low | Unit test normal generation, visual verification |
| Renderer architecture mismatch | High | Medium | Review renderer code before implementation |

---

## Success Criteria

✅ **MVP (Minimum Viable Product)**:
1. Terrain generates without errors
2. Top face vertices are deformed by Perlin noise
3. Lighting appears correct (normals calculated properly)
4. No visual seams between tiles
5. Frame rate remains acceptable (>30 FPS for 400 tiles)

✅ **Stretch Goals**:
1. Configurable noise parameters via UI or config file
2. Multiple terrain presets (hills, mountains, plains)
3. Colored materials based on height (snow on peaks, grass in valleys)

---

## Next Steps

**After Plan Approval**:
1. Create feature branch: `feature/terrain-generation`
2. Implement Phase 1 (dependency + module setup)
3. Implement Phase 2 (mesh generation + testing)
4. Implement Phase 3 (renderer integration)
5. Implement Phase 4 (scene builder)
6. Phase 5 (testing, tuning, documentation)
7. Create pull request for review

---

## Open Questions for User

1. **Visual Style**: Do you want realistic terrain, stylized blocks, or something else?
2. **Scale**: Should terrain be flat (like Minecraft) or have dramatic height variation?
3. **Grid Layout**: Square grid OK, or prefer hexagonal/irregular layout?
4. **Camera**: Should initial camera position/angle change for terrain viewing?
5. **Materials**: Single green material, or vary by height/biome?

Please review this plan and provide feedback before implementation begins! 🏔️
