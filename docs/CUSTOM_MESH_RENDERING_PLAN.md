# Custom Mesh Rendering Implementation Plan

**Goal**: Extend the rendering system to support entities with custom mesh geometry alongside instanced rendering.  
**Branch**: `custom-mesh-rendering` (to be created from `world-generation`)  
**Estimated Effort**: 4-6 hours  
**Status**: Not Started

---

## Problem Statement

The current renderer uses **instance-based rendering** where all instances share a single mesh (unit cube or unit sphere). VoxelChunks require **unique geometry per chunk** due to face culling optimization, which the current system cannot handle.

### Current Limitations
- ❌ VoxelChunk mesh data (vertices, normals, indices) generated but never uploaded to GPU
- ❌ Only `InstanceGpu` transform data reaches the renderer
- ❌ No path for custom geometry in rendering pipeline
- ❌ Results in black screen despite correct terrain generation

---

## Architecture Overview

### Current System (Instance-Based)

```
┌─────────────────────────────────────────────────────────────┐
│ ECS World (Legion)                                          │
│ ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│ │  Sphere  │  │  Cube    │  │ VoxelChunk│ (has mesh data)│
│ │  entity  │  │  entity  │  │  entity   │                 │
│ └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
         │              │              │
         └──────────────┴──────────────┘
                        │
                        ▼
         ┌──────────────────────────────┐
         │ collect_renderable_instances │
         │ (extracts InstanceGpu only)  │
         └──────────────────────────────┘
                        │
                        ▼
         ┌──────────────────────────────┐
         │    Renderer (engine_renderer)│
         │  • Upload unit cube once     │
         │  • Upload unit sphere once   │
         │  • Upload instance transforms│
         │  • Draw instanced            │
         └──────────────────────────────┘
                        │
                        ▼
                   GPU Rendering
         • Spheres render ✅
         • Cubes render ✅
         • VoxelChunks don't render ❌ (no mesh uploaded)
```

### Proposed System (Hybrid: Instanced + Custom Meshes)

```
┌─────────────────────────────────────────────────────────────┐
│ ECS World (Legion)                                          │
│ ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│ │  Sphere  │  │  Cube    │  │ VoxelChunk│                 │
│ │ (shared) │  │ (shared) │  │ (custom)  │                 │
│ └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
         │              │              │
         ├──────────────┤              │
         │              │              │
         ▼              ▼              ▼
    Renderable     Renderable    CustomMesh (NEW)
    (instances)    (instances)   (unique geometry)
         │              │              │
         └──────────────┴──────────────┘
                        │
         ┌──────────────┴───────────────┐
         │                              │
         ▼                              ▼
  collect_renderable_instances   collect_custom_meshes (NEW)
         │                              │
         ▼                              ▼
┌────────────────────────────────────────────────────────────┐
│ Renderer (engine_renderer)                                 │
│                                                            │
│ Instanced Path:           Custom Mesh Path: (NEW)        │
│ • Upload unit cube once    • Upload per-chunk geometry   │
│ • Upload unit sphere once  • Create GPU buffers per mesh │
│ • Upload instances         • Upload vertices/normals/idx  │
│ • Draw instanced           • Draw indexed (per chunk)    │
└────────────────────────────────────────────────────────────┘
         │                              │
         └──────────────┬───────────────┘
                        ▼
                   GPU Rendering
         • Spheres render ✅
         • Cubes render ✅
         • VoxelChunks render ✅ (custom mesh uploaded)
```

---

## Implementation Plan

### Phase 1: Define CustomMesh Trait (1-2 hours)

#### Task 1.1: Create CustomMesh Trait
**File**: `engine_core/src/actors.rs` or new `engine_core/src/custom_mesh.rs`

```rust
/// Trait for entities that provide custom mesh geometry
pub trait CustomMesh {
    /// Get vertex positions [x, y, z]
    fn vertices(&self) -> &[[f32; 3]];
    
    /// Get vertex normals [x, y, z]
    fn normals(&self) -> &[[f32; 3]];
    
    /// Get triangle indices (3 per triangle)
    fn indices(&self) -> &[u32];
    
    /// Get world-space transform matrix
    fn transform(&self) -> glam::Mat4;
    
    /// Get material index for this mesh
    fn material_index(&self) -> u32;
    
    /// Optional: Unique identifier for mesh caching
    fn mesh_id(&self) -> Option<u64> {
        None
    }
}
```

**Considerations**:
- Should return references to avoid copying large buffers
- Transform needed for positioning (even if identity)
- Material index integrates with existing material system
- Optional mesh_id for future caching optimization

#### Task 1.2: Implement CustomMesh for VoxelChunk
**File**: `engine_core/src/voxel.rs`

```rust
impl crate::actors::CustomMesh for VoxelChunk {
    fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }
    
    fn normals(&self) -> &[[f32; 3]] {
        &self.normals
    }
    
    fn indices(&self) -> &[u32] {
        &self.indices
    }
    
    fn transform(&self) -> glam::Mat4 {
        glam::Mat4::IDENTITY  // Chunk mesh already in world space
    }
    
    fn material_index(&self) -> u32 {
        0  // Lambertian material
    }
    
    fn mesh_id(&self) -> Option<u64> {
        // Future: hash of chunk position for caching
        None
    }
}
```

#### Task 1.3: Collection Function
**File**: `engine_core/src/actors.rs`

```rust
/// Collect all entities with custom mesh geometry
pub fn collect_custom_meshes(world: &World) -> Vec<CustomMeshData> {
    let mut meshes = Vec::new();
    
    // Query VoxelChunk entities
    let mut query = <&crate::voxel::VoxelChunk>::query();
    for chunk in query.iter(world) {
        if !chunk.is_empty() {
            meshes.push(CustomMeshData {
                vertices: chunk.vertices().to_vec(),
                normals: chunk.normals().to_vec(),
                indices: chunk.indices().to_vec(),
                transform: chunk.transform(),
                material_index: chunk.material_index(),
            });
        }
    }
    
    meshes
}

/// Serializable mesh data for renderer
#[derive(Clone)]
pub struct CustomMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub transform: glam::Mat4,
    pub material_index: u32,
}
```

**Note**: Copying data here to avoid lifetime issues between ECS world and renderer. Consider optimization later.

---

### Phase 2: Extend Renderer Architecture (2-3 hours)

#### Task 2.1: Study Current Renderer
**Files to examine**:
- `engine_renderer/src/lib.rs` - Main render loop
- `engine_renderer/src/scene.rs` - Scene/geometry management
- `engine_renderer/src/gpu_types.rs` - GPU buffer structures

**Questions to answer**:
1. Where are vertex/index buffers created?
2. How are they uploaded to GPU?
3. Where do draw calls happen?
4. How is the instance buffer structured?
5. What's the shader input format?

#### Task 2.2: Add Custom Mesh Buffer Management
**File**: `engine_renderer/src/scene.rs` (or new file)

```rust
/// Manages GPU buffers for a single custom mesh
pub struct CustomMeshGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub normal_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub material_index: u32,
    pub transform: glam::Mat4,
}

impl CustomMeshGpu {
    /// Create GPU buffers from mesh data
    pub fn from_mesh_data(
        device: &wgpu::Device,
        mesh: &CustomMeshData,
    ) -> Self {
        use wgpu::util::DeviceExt;
        
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let normal_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Normal Buffer"),
            contents: bytemuck::cast_slice(&mesh.normals),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Index Buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        
        CustomMeshGpu {
            vertex_buffer,
            normal_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            material_index: mesh.material_index,
            transform: mesh.transform,
        }
    }
}

/// Manages all custom meshes in the scene
pub struct CustomMeshManager {
    meshes: Vec<CustomMeshGpu>,
}

impl CustomMeshManager {
    pub fn new() -> Self {
        CustomMeshManager {
            meshes: Vec::new(),
        }
    }
    
    /// Upload new meshes to GPU
    pub fn upload_meshes(
        &mut self,
        device: &wgpu::Device,
        mesh_data: Vec<CustomMeshData>,
    ) {
        // Clear old meshes (simple approach - optimize later)
        self.meshes.clear();
        
        // Upload new meshes
        for data in mesh_data {
            self.meshes.push(CustomMeshGpu::from_mesh_data(device, &data));
        }
    }
    
    pub fn meshes(&self) -> &[CustomMeshGpu] {
        &self.meshes
    }
}
```

#### Task 2.3: Integrate with Render Loop
**File**: `engine_renderer/src/lib.rs` (or wherever render happens)

**Current structure** (approximate):
```rust
fn render(&mut self) {
    // Upload shared geometry
    // Upload instances
    // Draw instanced geometry
}
```

**Enhanced structure**:
```rust
fn render(&mut self, custom_meshes: Vec<CustomMeshData>) {
    // 1. Upload shared geometry (unchanged)
    // ...
    
    // 2. Upload instances (unchanged)
    // ...
    
    // 3. Upload custom meshes (NEW)
    self.custom_mesh_manager.upload_meshes(&self.device, custom_meshes);
    
    // 4. Draw instanced geometry (unchanged)
    // ...
    
    // 5. Draw custom meshes (NEW)
    self.render_custom_meshes(render_pass);
}

fn render_custom_meshes(&self, render_pass: &mut wgpu::RenderPass) {
    for mesh in self.custom_mesh_manager.meshes() {
        // Set vertex buffers
        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, mesh.normal_buffer.slice(..));
        
        // Set index buffer
        render_pass.set_index_buffer(
            mesh.index_buffer.slice(..),
            wgpu::IndexFormat::Uint32
        );
        
        // Set material/transform (may need uniform buffer update)
        // self.update_mesh_uniforms(mesh.transform, mesh.material_index);
        
        // Draw indexed
        render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}
```

---

### Phase 3: Shader Compatibility (1 hour)

#### Task 3.1: Verify Shader Input Format
**Files**: `shaders/vertex.wgsl`, `shaders/common.wgsl`

Check current vertex input:
```wgsl
struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}
```

**Compatibility check**:
- ✅ Position and normal match our mesh data format `[f32; 3]`
- ✅ Existing shaders should work with custom meshes
- ⚠️ May need to handle transform differently (not from instance buffer)

#### Task 3.2: Handle Non-Instanced Uniforms
Current instance-based rendering uses per-instance transforms from instance buffer. Custom meshes need transform in a uniform buffer.

**Options**:
1. **Use instance buffer with 1 instance per mesh** (simpler, reuses existing code)
2. **Create separate uniform buffer** (cleaner separation)
3. **Bake transform into vertices** (VoxelChunk already does this with identity transform)

**Recommendation**: Option 3 is already implemented - VoxelChunk mesh is in world space with identity transform.

---

### Phase 4: Integration & Testing (1 hour)

#### Task 4.1: Connect Engine Core to Renderer
**File**: Main application render loop (likely `src/main.rs` or engine integration point)

```rust
// In render/update loop
fn render_frame(&mut self) {
    // Collect instances (existing)
    let instances = engine_core::actors::collect_renderable_instances(&mut self.world);
    
    // Collect custom meshes (NEW)
    let custom_meshes = engine_core::actors::collect_custom_meshes(&self.world);
    
    // Pass to renderer
    self.renderer.render(instances, custom_meshes);
}
```

#### Task 4.2: Initial Testing
1. **Single VoxelChunk Test**
   - Modify terrain generation to create only 1 chunk
   - Verify mesh uploads without errors
   - Check vertex/index counts in debugger
   - Confirm draw call executes

2. **Visual Verification**
   - Should see terrain blocks appear
   - Verify face culling (no hidden faces)
   - Check lighting/normals look correct
   - Confirm camera view shows terrain

3. **Full Terrain Test**
   - Re-enable all 16 chunks
   - Verify performance (should be 16 draw calls)
   - Check memory usage
   - Ensure no visual artifacts between chunks

#### Task 4.3: Performance Validation
- Measure frame time with custom mesh rendering
- Compare triangle count with/without face culling
- Verify GPU memory usage is reasonable
- Check for memory leaks (upload on every frame vs. caching)

---

### Phase 5: Optimization & Polish (Optional - 1 hour)

#### Task 5.1: Mesh Caching
Instead of uploading every frame, cache meshes and only update when terrain changes:

```rust
pub struct CustomMeshManager {
    meshes: HashMap<u64, CustomMeshGpu>,  // mesh_id -> GPU buffers
    dirty: bool,
}

impl CustomMeshManager {
    pub fn upload_if_needed(
        &mut self,
        device: &wgpu::Device,
        mesh_data: Vec<CustomMeshData>,
    ) {
        // Only upload if terrain changed
        if !self.dirty {
            return;
        }
        
        // ... upload logic
        self.dirty = false;
    }
    
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}
```

#### Task 5.2: Material Batching
Group custom meshes by material to reduce state changes:

```rust
// Sort meshes by material_index before drawing
let mut meshes_by_material: HashMap<u32, Vec<&CustomMeshGpu>> = HashMap::new();
for mesh in self.custom_mesh_manager.meshes() {
    meshes_by_material.entry(mesh.material_index)
        .or_insert_with(Vec::new)
        .push(mesh);
}

// Draw grouped by material
for (material_idx, meshes) in meshes_by_material {
    self.set_material(material_idx);
    for mesh in meshes {
        self.draw_mesh(mesh);
    }
}
```

#### Task 5.3: Frustum Culling
Don't draw chunks outside camera view:

```rust
fn is_chunk_visible(chunk_pos: IVec3, camera: &Camera) -> bool {
    // Simple distance check or proper frustum culling
    let chunk_center = chunk_pos.as_vec3() * CHUNK_SIZE;
    camera.frustum.contains_sphere(chunk_center, CHUNK_SIZE * 0.866)  // Diagonal
}
```

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_custom_mesh_trait() {
    let chunk = VoxelChunk::new(IVec3::ZERO);
    // Add some blocks
    // Verify vertices() returns correct count
    // Verify indices() are valid (all < vertex count)
}

#[test]
fn test_collect_custom_meshes() {
    let mut world = World::default();
    // Add VoxelChunk entities
    let meshes = collect_custom_meshes(&world);
    assert_eq!(meshes.len(), expected_count);
}
```

### Integration Tests
1. **Render Single Chunk**: Verify one chunk renders without errors
2. **Render Multiple Chunks**: Verify all 16 chunks render
3. **Face Culling Verification**: Compare triangle counts
4. **Material Assignment**: Verify Lambertian shading appears correct
5. **Camera Movement**: Verify chunks stay positioned correctly

### Visual Tests
- [ ] Terrain appears at expected location (centered at origin)
- [ ] Face culling works (no interior faces visible)
- [ ] Lighting looks correct (normals point outward)
- [ ] No seams between chunks
- [ ] Smooth ramps render correctly
- [ ] Material variation visible (grass/dirt/stone)

---

## Potential Issues & Solutions

### Issue 1: Lifetime Problems with References
**Problem**: `CustomMesh` trait returns references, but renderer needs owned data.

**Solution**: Copy data in `collect_custom_meshes()` as shown in implementation. Optimize later with arena allocation or unsafe tricks if needed.

### Issue 2: Buffer Upload Every Frame
**Problem**: Currently re-uploading all meshes every frame (wasteful).

**Solution**: Implement caching in Phase 5. For now, profile to see if it's actually a problem (16 chunks is small).

### Issue 3: Shader Compatibility
**Problem**: Shaders expect instance buffer but custom meshes don't use it.

**Solution**: Use identity transform and fake instance data, or modify shaders to support both paths.

### Issue 4: Material System Integration
**Problem**: Custom meshes need to set material uniforms per-mesh, not per-instance.

**Solution**: Update material uniform buffer before each custom mesh draw call, or use push constants if supported.

### Issue 5: Index Format Mismatch
**Problem**: Shader might expect u16 indices but we use u32.

**Solution**: Verify shader uses `u32` or convert indices during upload.

---

## Success Criteria

### Minimum Viable (Must Have)
- [x] CustomMesh trait defined
- [x] VoxelChunk implements CustomMesh
- [x] collect_custom_meshes() works
- [ ] Renderer uploads custom mesh buffers
- [ ] Custom meshes render on screen
- [ ] No crashes or GPU errors
- [ ] Face culling visible (interior faces not rendered)

### Full Success (Should Have)
- [ ] All 16 chunks render correctly
- [ ] Lighting/normals correct
- [ ] No visual artifacts or seams
- [ ] Performance acceptable (>30 FPS)
- [ ] Memory usage reasonable (<1 GB)

### Stretch Goals (Nice to Have)
- [ ] Mesh caching implemented
- [ ] Frustum culling working
- [ ] Material batching optimization
- [ ] Debug visualization (wireframe mode)
- [ ] Chunk boundary visualization

---

## Rollback Plan

If custom mesh rendering proves too complex or breaks existing rendering:

1. **Fallback to instanced cubes**: Render each block as a separate cube instance (no face culling)
2. **Temporary single-chunk**: Limit to 1 VoxelChunk to simplify debugging
3. **Alternative renderer**: Explore switching to a different rendering architecture

---

## Timeline

| Phase | Tasks | Estimated Time | Dependencies |
|-------|-------|----------------|--------------|
| Phase 1 | Define trait, implement for VoxelChunk | 1-2 hours | None |
| Phase 2 | Renderer architecture changes | 2-3 hours | Phase 1 |
| Phase 3 | Shader compatibility | 1 hour | Phase 2 |
| Phase 4 | Integration & testing | 1 hour | Phase 3 |
| Phase 5 | Optimization (optional) | 1 hour | Phase 4 |
| **Total** | | **6-8 hours** | |

---

## Next Steps

1. **Commit current work** to `world-generation` branch
2. **Create new branch** `custom-mesh-rendering` from `world-generation`
3. **Study renderer code** (`engine_renderer/src/lib.rs`) to understand current architecture
4. **Start Phase 1** with trait definition and implementation
5. **Iterate** through phases with frequent testing

---

## References

- Current voxel system: `engine_core/src/voxel.rs`
- Rendering system: `engine_renderer/src/`
- Instance-based rendering: `engine_core/src/actors.rs::collect_renderable_instances()`
- Shaders: `shaders/*.wgsl`
- Main integration: `src/main.rs`
