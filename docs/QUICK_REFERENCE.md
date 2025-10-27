# Custom Mesh Rendering - Quick Reference

**Quick lookup for implementation details while coding**

---

## 🎯 Goal
Enable VoxelChunk rendering by adding custom mesh support to renderer alongside existing instance-based rendering.

---

## 📊 Current vs. Target Architecture

### Current (Instance-Based Only)
```
Sphere/Cube → Renderable → InstanceGpu → Renderer (shared mesh) → GPU ✅
VoxelChunk  → Renderable → InstanceGpu → Renderer (no mesh!)  → GPU ❌
```

### Target (Hybrid)
```
Sphere/Cube → Renderable  → InstanceGpu     → Renderer (shared)  → GPU ✅
VoxelChunk  → CustomMesh  → CustomMeshData  → Renderer (custom)  → GPU ✅
```

---

## 🔑 Key Code Locations

### Files to Create/Modify
| File | Action | Purpose |
|------|--------|---------|
| `moho_core/src/actors.rs` | Add trait | Define `CustomMesh` trait |
| `moho_core/src/actors.rs` | Add function | `collect_custom_meshes()` |
| `moho_core/src/voxel.rs` | Implement | `CustomMesh` for `VoxelChunk` |
| `moho_renderer/src/scene.rs` | Add struct | `CustomMeshGpu` buffer manager |
| `moho_renderer/src/lib.rs` | Modify | Add custom mesh rendering path |
| Main render loop | Modify | Call `collect_custom_meshes()` |

### Files to Study First
| File | Why |
|------|-----|
| `moho_renderer/src/lib.rs` | Understand render loop |
| `moho_renderer/src/scene.rs` | See buffer management |
| `moho_renderer/src/gpu_types.rs` | Understand GPU structures |
| `shaders/vertex.wgsl` | Verify input compatibility |

---

## 💻 Code Snippets (Ready to Use)

### 1. CustomMesh Trait
```rust
// In moho_core/src/actors.rs
pub trait CustomMesh {
    fn vertices(&self) -> &[[f32; 3]];
    fn normals(&self) -> &[[f32; 3]];
    fn indices(&self) -> &[u32];
    fn transform(&self) -> glam::Mat4;
    fn material_index(&self) -> u32;
}
```

### 2. Implement for VoxelChunk
```rust
// In moho_core/src/voxel.rs
impl crate::actors::CustomMesh for VoxelChunk {
    fn vertices(&self) -> &[[f32; 3]] { &self.vertices }
    fn normals(&self) -> &[[f32; 3]] { &self.normals }
    fn indices(&self) -> &[u32] { &self.indices }
    fn transform(&self) -> glam::Mat4 { glam::Mat4::IDENTITY }
    fn material_index(&self) -> u32 { 0 }
}
```

### 3. Collection Function
```rust
// In moho_core/src/actors.rs
pub struct CustomMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub transform: glam::Mat4,
    pub material_index: u32,
}

pub fn collect_custom_meshes(world: &World) -> Vec<CustomMeshData> {
    let mut meshes = Vec::new();
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
```

### 4. GPU Buffer Creation
```rust
// In moho_renderer/src/scene.rs or new file
use wgpu::util::DeviceExt;

pub struct CustomMeshGpu {
    pub vertex_buffer: wgpu::Buffer,
    pub normal_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub material_index: u32,
}

impl CustomMeshGpu {
    pub fn from_mesh_data(device: &wgpu::Device, mesh: &CustomMeshData) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let normal_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Normals"),
            contents: bytemuck::cast_slice(&mesh.normals),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Custom Mesh Indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        
        CustomMeshGpu {
            vertex_buffer,
            normal_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
            material_index: mesh.material_index,
        }
    }
}
```

### 5. Draw Custom Mesh
```rust
// In render pass loop
fn render_custom_meshes(&self, render_pass: &mut wgpu::RenderPass) {
    for mesh in self.custom_meshes.iter() {
        render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, mesh.normal_buffer.slice(..));
        render_pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        
        // May need to update material uniform here
        
        render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}
```

---

## 🧪 Testing Checklist

### Phase 1: Trait Definition
- [ ] CustomMesh trait compiles
- [ ] VoxelChunk implements CustomMesh without errors
- [ ] collect_custom_meshes() returns correct count
- [ ] Mesh data not empty (vertices.len() > 0)

### Phase 2: Renderer Integration
- [ ] GPU buffers created successfully
- [ ] Buffer sizes match expected (vertices × 12 bytes, etc.)
- [ ] No wgpu validation errors
- [ ] Device/queue operations succeed

### Phase 3: Rendering
- [ ] Render pass doesn't crash
- [ ] Draw calls execute (use RenderDoc/PIX to verify)
- [ ] Something appears on screen (even if wrong)
- [ ] No shader errors in console

### Phase 4: Visual Validation
- [ ] Terrain appears at correct location (near origin)
- [ ] Geometry looks blocky (cubes visible)
- [ ] Face culling works (no internal faces)
- [ ] Normals correct (lighting looks reasonable)
- [ ] No seams between chunks

---

## 📏 Expected Metrics

| Metric | Value | How to Verify |
|--------|-------|---------------|
| Chunks rendered | 16 | Check draw call count |
| Vertices per chunk | ~500-1000 | Log in from_mesh_data() |
| Indices per chunk | ~1500-3000 | index_count field |
| Total triangles | ~6,000-8,000 | Sum of all index_count / 3 |
| Draw calls | 16 | RenderDoc or log in render loop |
| GPU memory | ~125 KB | Sum of buffer sizes |

---

## 🐛 Common Issues & Fixes

### Issue: Black Screen Still
**Check**:
- [ ] Are meshes being collected? (log in collect_custom_meshes)
- [ ] Are buffers being created? (log in from_mesh_data)
- [ ] Are draw calls executing? (log in render loop)
- [ ] Is render pass configured correctly?
- [ ] Are shaders bound?

### Issue: Validation Errors
**Check**:
- [ ] Buffer usage flags correct (VERTEX, INDEX)
- [ ] Index format matches (Uint32)
- [ ] Vertex buffer slots match shader (@location 0, 1)
- [ ] Buffer not empty before upload

### Issue: Wrong Position/Scale
**Check**:
- [ ] Transform is identity (chunks already in world space)
- [ ] Camera position correct (40, 25, 40)
- [ ] Camera looking at (0, 8, 0)
- [ ] Projection matrix correct

### Issue: Inside-Out Geometry
**Check**:
- [ ] Index winding order (should be counter-clockwise)
- [ ] Normals point outward (check in voxel.rs mesh generation)
- [ ] Face culling mode (back-face culling expected)

---

## 🎯 Minimum Viable Implementation

**To see ANYTHING on screen**:
1. CustomMesh trait defined ✅
2. VoxelChunk implements it ✅
3. collect_custom_meshes() works ✅
4. Single chunk uploads to GPU ✅
5. Single draw call executes ✅

**That's it!** Don't worry about optimization yet.

---

## 🚦 Progress Tracking

### Phase 1: Foundation (estimate: 1 SP)
- [ ] CustomMesh trait defined and documented
- [ ] VoxelChunk implements CustomMesh
- [ ] collect_custom_meshes() implemented
- [ ] Unit tests pass

### Phase 2: Renderer (estimate: 2 SP)
- [ ] CustomMeshGpu struct created
- [ ] Buffer upload working
- [ ] CustomMeshManager added
- [ ] Integration with render loop

### Phase 3: Rendering (estimate: 1 SP)
- [ ] Draw calls implemented
- [ ] Shader compatibility verified
- [ ] First visual output achieved

### Phase 4: Testing (estimate: 1 SP)
- [ ] All 16 chunks render
- [ ] Face culling verified
- [ ] Performance acceptable
- [ ] No visual artifacts

### Phase 5: Polish (optional)
- [ ] Mesh caching
- [ ] Material batching
- [ ] Frustum culling

---

## 📞 When Stuck

1. **Check logs** - Did functions execute?
2. **Use RenderDoc** - Capture frame, inspect buffers
3. **Simplify** - Single chunk, single triangle
4. **Compare** - Look at Sphere/Cube rendering code
5. **Validate** - Check wgpu validation layer output

---

## ✅ Success Definition

**You're done when**:
- Terrain appears on screen
- 16 chunks visible
- Face culling working (no hidden faces)
- No crashes or errors
- Performance >30 FPS

**Everything else is optimization!**

---

**Keep this tab open while coding!** 🚀

