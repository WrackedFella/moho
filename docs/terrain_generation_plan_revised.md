# Voxel-Based Terrain Generation - Revised Plan

## Overview
Build a **voxel/block-based terrain system** with 3D grid tracking, supporting stylized blocks, ramps/slopes, future mining mechanics, and resource management.

---

## Architecture Change Summary

### Original Plan (❌ Deprecated)
- Deformed cube meshes with Perlin noise on top face
- Individual TerrainTile entities
- Static geometry, no editing support

### Revised Plan (✅ Current)
- **3D voxel grid system** with discrete block coordinates
- **Block types**: solid cubes, air, ramps/slopes
- **Editable terrain**: add/remove blocks at runtime
- **Resource tracking**: each block can contain material/resource data
- **Chunk-based rendering**: optimize for large worlds

---

## System Architecture

### Core Components

#### 1. VoxelGrid (World Container)
```rust
pub struct VoxelGrid {
    // 3D storage: chunks or flat HashMap
    blocks: HashMap<IVec3, VoxelBlock>,
    chunk_size: i32,
    // Future: chunk-based storage for LOD
}
```

**Key Features**:
- Sparse storage (only store non-air blocks)
- Fast coordinate lookups O(1)
- Chunk subdivision for rendering optimization
- Supports infinite or bounded worlds

---

#### 2. VoxelBlock (Individual Block)
```rust
pub struct VoxelBlock {
    pub position: IVec3,        // Grid coordinates
    pub mesh_data: VoxelMesh,   // Custom mesh for this block
    pub material_id: u32,       // Index into material array
    pub resource_id: Option<u32>, // Index into resource array
}

pub struct VoxelMesh {
    // Stores deformed vertex positions for this block
    // Could be a full cube, or smoothed/ramped based on neighbors
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}
```

**Key Features**:
- All blocks are "voxels" to the engine (no explicit types)
- Mesh shape determined by smoothing algorithm (automatic ramps)
- Material/resource data stored separately via IDs
- Simple, uniform data structure

---

#### 3. BlockMeshGenerator & Smoothing
```rust
pub struct TerrainSmoother {
    // Analyzes neighbor heights to determine if smoothing is needed
}

impl TerrainSmoother {
    // After initial cube placement, smooth transitions between heights
    pub fn smooth_terrain(grid: &mut VoxelGrid) {
        for block in grid.iter_blocks_mut() {
            let neighbors = grid.get_neighbor_heights(block.position);
            if needs_smoothing(&neighbors) {
                block.mesh_data = generate_smoothed_mesh(block.position, &neighbors);
            } else {
                block.mesh_data = generate_cube_mesh();
            }
        }
    }
}
```

**Mesh Generation Strategy**:
1. **Initial Pass**: Place full cube blocks based on noise height
2. **Smoothing Pass**: Analyze height differences between neighbors
3. **Ramp Generation**: Automatically convert cubes to ramps where height changes by 1
4. **Normal Recalculation**: Compute smooth normals for deformed meshes
5. **Per-Chunk Merging**: Combine blocks into single chunk mesh

---

### Rendering Pipeline

```
VoxelGrid → Chunk Subdivision → Mesh Generation → Render Instances
     ↓              ↓                    ↓                ↓
[Block Data]  [16×16×16 chunks]  [Optimized mesh]  [GPU buffers]
```

**Optimizations**:
1. **Face Culling**: Don't render faces between solid blocks
2. **Greedy Meshing**: Merge adjacent same-material quads
3. **Chunk-based**: Only regenerate mesh for modified chunks
4. **Frustum Culling**: Only render visible chunks

---

## Implementation Plan (Revised)

### Phase 1: Foundation (Voxel System)

#### Task 1.1: Add Noise Dependency
**File**: `engine_core/Cargo.toml`
```toml
noise = "0.9"
```
**Same as before** - Perlin noise still used for terrain height generation

**Estimated Effort**: 5 minutes

---

#### Task 1.2: Create Voxel Module
**File**: `engine_core/src/voxel.rs` (new)

```rust
use glam::{IVec3, Vec3};
use std::collections::HashMap;

/// Integer vector for grid coordinates
pub type BlockPos = IVec3;

/// Mesh data for a single voxel block
#[derive(Debug, Clone)]
pub struct VoxelMesh {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Individual voxel block in the world
/// All blocks are treated uniformly - material/resource data stored separately
#[derive(Debug, Clone)]
pub struct VoxelBlock {
    pub position: BlockPos,
    pub mesh_data: VoxelMesh,
    pub material_id: u32,       // Index into MaterialRegistry
    pub resource_id: Option<u32>, // Index into ResourceRegistry
}

/// 3D grid storing voxel blocks
pub struct VoxelGrid {
    blocks: HashMap<BlockPos, VoxelBlock>,
    chunk_size: i32,
    material_registry: MaterialRegistry,
    resource_registry: ResourceRegistry,
}

/// Material registry - maps material IDs to actual material data
pub struct MaterialRegistry {
    materials: Vec<MaterialType>,
}

/// Resource registry - maps resource IDs to resource data
pub struct ResourceRegistry {
    resources: Vec<ResourceData>,
}

#[derive(Debug, Clone)]
pub struct ResourceData {
    pub resource_type: String,  // "stone", "ore", "dirt", etc.
    pub quantity: u32,
}

impl VoxelGrid {
    pub fn new(chunk_size: i32) -> Self {
        VoxelGrid {
            blocks: HashMap::new(),
            chunk_size,
            material_registry: MaterialRegistry::new(),
            resource_registry: ResourceRegistry::new(),
        }
    }
    
    pub fn set_block(&mut self, pos: BlockPos, block: VoxelBlock) {
        self.blocks.insert(pos, block);
    }
    
    pub fn get_block(&self, pos: &BlockPos) -> Option<&VoxelBlock> {
        self.blocks.get(pos)
    }
    
    pub fn get_block_mut(&mut self, pos: &BlockPos) -> Option<&mut VoxelBlock> {
        self.blocks.get_mut(pos)
    }
    
    pub fn remove_block(&mut self, pos: &BlockPos) -> Option<VoxelBlock> {
        self.blocks.remove(pos)
    }
    
    pub fn iter_blocks(&self) -> impl Iterator<Item = &VoxelBlock> {
        self.blocks.values()
    }
    
    pub fn iter_blocks_mut(&mut self) -> impl Iterator<Item = &mut VoxelBlock> {
        self.blocks.values_mut()
    }
    
    /// Get chunk coordinate from block position
    pub fn get_chunk_pos(&self, pos: BlockPos) -> IVec3 {
        IVec3::new(
            pos.x.div_euclid(self.chunk_size),
            pos.y.div_euclid(self.chunk_size),
            pos.z.div_euclid(self.chunk_size),
        )
    }
    
    /// Get all blocks in a chunk
    pub fn get_chunk_blocks(&self, chunk_pos: IVec3) -> Vec<&VoxelBlock> {
        let min = chunk_pos * self.chunk_size;
        let max = min + IVec3::splat(self.chunk_size);
        
        self.blocks.values()
            .filter(|b| {
                b.position.x >= min.x && b.position.x < max.x &&
                b.position.y >= min.y && b.position.y < max.y &&
                b.position.z >= min.z && b.position.z < max.z
            })
            .collect()
    }
    
    /// Get height at a position (highest Y with a block)
    pub fn get_height(&self, x: i32, z: i32) -> Option<i32> {
        let blocks_at_xz: Vec<i32> = self.blocks.keys()
            .filter(|pos| pos.x == x && pos.z == z)
            .map(|pos| pos.y)
            .collect();
        blocks_at_xz.into_iter().max()
    }
    
    /// Get neighbor heights for smoothing algorithm
    pub fn get_neighbor_heights(&self, pos: BlockPos) -> [Option<i32>; 4] {
        [
            self.get_height(pos.x, pos.z + 1),     // North
            self.get_height(pos.x, pos.z - 1),     // South
            self.get_height(pos.x + 1, pos.z),     // East
            self.get_height(pos.x - 1, pos.z),     // West
        ]
    }
}

impl VoxelBlock {
    pub fn new(position: BlockPos, material_id: u32) -> Self {
        VoxelBlock {
            position,
            mesh_data: VoxelMesh::empty(),
            material_id,
            resource_id: None,
        }
    }
    
    /// Convert grid position to world position (center of block)
    pub fn world_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x as f32,
            self.position.y as f32,
            self.position.z as f32,
        )
    }
}

impl VoxelMesh {
    pub fn empty() -> Self {
        VoxelMesh {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        }
    }
}
```

**Update**: `engine_core/src/lib.rs`
```rust
pub mod voxel;
```

**Estimated Effort**: 1 hour

---

### Phase 2: Mesh Generation

#### Task 2.1: Basic Mesh Generation
**File**: `engine_core/src/voxel.rs` (continued)

```rust
pub struct MeshGenerator;

impl MeshGenerator {
    /// Generate standard cube mesh (baseline)
    pub fn cube_mesh() -> VoxelMesh {
        let (verts, normals, indices) = crate::actors::Cube::unit_cube_indexed();
        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }
    
    /// Generate mesh with top vertices deformed based on neighbor heights
    /// This creates smooth transitions/ramps automatically
    pub fn smoothed_mesh(
        position: BlockPos,
        neighbor_heights: [Option<i32>; 4], // [N, S, E, W]
    ) -> VoxelMesh {
        let current_height = position.y;
        let (mut verts, mut normals, indices) = crate::actors::Cube::unit_cube_indexed();
        
        // Identify top face vertices (indices vary, but typically last 4)
        // For standard cube: vertices 8-11 are top face
        // Adjust Y coordinates based on neighbor heights
        
        let [north_h, south_h, east_h, west_h] = neighbor_heights;
        
        // If neighbor is lower by 1, deform that edge down to create ramp
        if north_h == Some(current_height - 1) {
            // Deform north edge (z = -0.5)
            deform_vertices_on_edge(&mut verts, Edge::North, -0.5);
        }
        if south_h == Some(current_height - 1) {
            deform_vertices_on_edge(&mut verts, Edge::South, -0.5);
        }
        if east_h == Some(current_height - 1) {
            deform_vertices_on_edge(&mut verts, Edge::East, -0.5);
        }
        if west_h == Some(current_height - 1) {
            deform_vertices_on_edge(&mut verts, Edge::West, -0.5);
        }
        
        // Recalculate normals for deformed faces
        recalculate_normals(&verts, &indices, &mut normals);
        
        VoxelMesh {
            vertices: verts,
            normals,
            indices,
        }
    }
}

enum Edge { North, South, East, West }

fn deform_vertices_on_edge(verts: &mut Vec<[f32; 3]>, edge: Edge, offset: f32) {
    // Find vertices on the specified edge and adjust their Y coordinate
    for vert in verts.iter_mut() {
        let matches_edge = match edge {
            Edge::North => vert[2] < -0.49 && vert[1] > 0.49,  // z=-0.5, y=0.5 (top north)
            Edge::South => vert[2] > 0.49 && vert[1] > 0.49,   // z=0.5, y=0.5 (top south)
            Edge::East => vert[0] > 0.49 && vert[1] > 0.49,    // x=0.5, y=0.5 (top east)
            Edge::West => vert[0] < -0.49 && vert[1] > 0.49,   // x=-0.5, y=0.5 (top west)
        };
        
        if matches_edge {
            vert[1] += offset; // Move down by offset
        }
    }
}

fn recalculate_normals(
    verts: &[[f32; 3]],
    indices: &[u32],
    normals: &mut Vec<[f32; 3]>,
) {
    // For each triangle, calculate face normal
    // Average normals at shared vertices for smooth shading
    // (Simplified - full implementation would handle vertex sharing properly)
    for i in (0..indices.len()).step_by(3) {
        let i0 = indices[i] as usize;
        let i1 = indices[i + 1] as usize;
        let i2 = indices[i + 2] as usize;
        
        let v0 = Vec3::from(verts[i0]);
        let v1 = Vec3::from(verts[i1]);
        let v2 = Vec3::from(verts[i2]);
        
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let normal = edge1.cross(edge2).normalize();
        
        // Assign to all three vertices (averaging would be better)
        normals[i0] = normal.to_array();
        normals[i1] = normal.to_array();
        normals[i2] = normal.to_array();
    }
}
```

**Estimated Effort**: 2-3 hours

---

#### Task 2.2: Terrain Smoothing Algorithm
**File**: `engine_core/src/voxel.rs` (continued)

```rust
pub struct TerrainSmoother;

impl TerrainSmoother {
    /// Apply smoothing pass to entire terrain
    /// Converts cubes to ramps where there are single-block height differences
    pub fn smooth_terrain(grid: &mut VoxelGrid) {
        // Collect positions first to avoid borrow issues
        let positions: Vec<BlockPos> = grid.iter_blocks()
            .map(|b| b.position)
            .collect();
        
        for pos in positions {
            let neighbor_heights = grid.get_neighbor_heights(pos);
            
            if Self::needs_smoothing(&neighbor_heights, pos.y) {
                // Generate smoothed mesh for this block
                let smoothed = MeshGenerator::smoothed_mesh(pos, neighbor_heights);
                
                if let Some(block) = grid.get_block_mut(&pos) {
                    block.mesh_data = smoothed;
                }
            } else {
                // Keep as standard cube
                if let Some(block) = grid.get_block_mut(&pos) {
                    block.mesh_data = MeshGenerator::cube_mesh();
                }
            }
        }
    }
    
    /// Determine if a block needs smoothing based on neighbor heights
    fn needs_smoothing(neighbor_heights: &[Option<i32>; 4], current_height: i32) -> bool {
        // If any neighbor is exactly 1 block lower, we need smoothing
        neighbor_heights.iter().any(|&h| h == Some(current_height - 1))
    }
}
```

**Estimated Effort**: 1-2 hours

---

### Phase 3: Terrain Generation

#### Task 3.1: Terrain Generation Algorithm
**File**: `engine_core/src/scene_builders.rs`

```rust
use crate::voxel::{VoxelGrid, VoxelBlock, BlockType, BlockPos};
use noise::{NoiseFn, Perlin};

pub struct TerrainConfig {
    pub frequency: f64,
    pub amplitude: f32,
    pub octaves: u32,
    pub seed: u32,
    pub terrain_type: TerrainType,
}

pub enum TerrainType {
    GentleHills,      // Smooth, rolling terrain
    Mountains,        // Dramatic height variation
    Plains,           // Mostly flat with small bumps
    Cliffs,           // Stepped terrain with vertical faces
    Canyon,           // Deep valleys
}

impl Default for TerrainConfig {
    fn default() -> Self {
        TerrainConfig {
            frequency: 0.05,
            amplitude: 8.0,
            octaves: 3,
            seed: 42,
            terrain_type: TerrainType::GentleHills,
        }
    }
}

pub fn voxel_terrain_scene(world: &mut World) {
    let config = TerrainConfig::default();
    let mut grid = VoxelGrid::new(16); // 16×16×16 chunks
    
    generate_terrain(&mut grid, &config);
    
    // Store VoxelGrid as a resource in the ECS world
    // (or convert blocks to entities)
    world.push((grid,));
    
    log::info!("Voxel terrain generated");
}

fn generate_terrain(grid: &mut VoxelGrid, config: &TerrainConfig) {
    let noise = Perlin::new(config.seed);
    let size = 64; // 64×64 XZ plane
    
    // Pass 1: Generate vertical columns of blocks based on noise
    for x in -size..size {
        for z in -size..size {
            // Sample noise for height
            let height = sample_height(&noise, x, z, config);
            
            // Generate vertical column of blocks
            for y in 0..=height {
                let pos = BlockPos::new(x, y, z);
                let material_id = determine_material_id(grid, height, y);
                let resource_id = determine_resource_id(height, y);
                
                let mut block = VoxelBlock::new(pos, material_id);
                block.resource_id = resource_id;
                
                grid.set_block(pos, block);
            }
        }
    }
    
    // Pass 2: Apply smoothing to create ramps automatically
    TerrainSmoother::smooth_terrain(grid);
}

fn sample_height(noise: &Perlin, x: i32, z: i32, config: &TerrainConfig) -> i32 {
    let mut value = 0.0;
    let mut amplitude = config.amplitude;
    let mut frequency = config.frequency;
    
    // Multi-octave Perlin noise
    for _ in 0..config.octaves {
        value += noise.get([
            x as f64 * frequency,
            z as f64 * frequency,
        ]) * amplitude as f64;
        
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    
    // Terrain type modifiers
    value = match config.terrain_type {
        TerrainType::GentleHills => value,
        TerrainType::Mountains => value * 2.0,
        TerrainType::Plains => value * 0.3,
        TerrainType::Cliffs => (value * 4.0).floor() / 4.0, // Stepped
        TerrainType::Canyon => {
            // Negative in valleys, positive on ridges
            if value < 0.0 { value * 2.0 } else { value * 0.5 }
        }
    };
    
    (value.max(0.0) as i32).clamp(0, 32) // Height range [0, 32]
}

fn determine_material_id(grid: &VoxelGrid, column_height: i32, y: i32) -> u32 {
    // Register materials if not already present and return ID
    // For now, use simple height-based logic
    if y == column_height {
        0 // Grass material (top layer)
    } else if y > column_height - 3 {
        1 // Dirt material (sub-surface)
    } else {
        2 // Stone material (deep)
    }
}

fn determine_resource_id(column_height: i32, y: i32) -> Option<u32> {
    // Distribute resources based on depth
    // 10% chance of iron ore in mid-levels
    if y > 5 && y < column_height - 3 {
        if rand::random::<f32>() < 0.1 {
            Some(1) // Iron ore resource ID
        } else {
            None
        }
    } else {
        None
    }
}
```

**Estimated Effort**: 2-3 hours

---

### Phase 4: ECS Integration

#### Task 4.1: VoxelGrid as ECS Component
**File**: `engine_core/src/actors.rs`

**Option A: Grid as Resource**
```rust
// Store entire VoxelGrid as a single entity/resource
world.push((voxel_grid,));
```

**Option B: Blocks as Entities**
```rust
// Create entity for each visible block (memory-intensive)
for block in grid.iter_blocks() {
    world.push((block.clone(),));
}
```

**Option C: Chunks as Entities** (Recommended)
```rust
pub struct VoxelChunk {
    pub chunk_pos: IVec3,
    pub mesh: ChunkMesh,
    pub material_indices: Vec<u32>,
}

// Create entity per chunk
for chunk_pos in grid.get_active_chunks() {
    let chunk = VoxelChunk::new(chunk_pos, &grid);
    world.push((chunk,));
}
```

**Recommended**: **Option C** - Chunks as entities, enables efficient culling and LOD

---

#### Task 4.2: Renderable for VoxelChunk
**File**: `engine_core/src/actors.rs`

```rust
impl Renderable for VoxelChunk {
    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        // Chunk mesh is already in world space
        let model = glam::Mat4::IDENTITY;
        let cols = model.to_cols_array();
        let mut mat = [[0f32; 4]; 4];
        mat[0] = [cols[0], cols[1], cols[2], cols[3]];
        mat[1] = [cols[4], cols[5], cols[6], cols[7]];
        mat[2] = [cols[8], cols[9], cols[10], cols[11]];
        mat[3] = [cols[12], cols[13], cols[14], cols[15]];
        
        InstanceGpu {
            model: mat,
            material: material_index,
            object_type: 3u32, // New type for voxel chunks
            padding: [0u32; 2],
        }
    }
}
```

**Update**: `collect_renderable_instances()`
```rust
pub fn collect_renderable_instances(world: &mut World) -> Vec<InstanceGpu> {
    let mut out: Vec<InstanceGpu> = Vec::new();
    
    // Existing sphere/cube collection...
    
    let mut qv = <&VoxelChunk>::query();
    for chunk in qv.iter(world) {
        out.push(chunk.to_instance_with_material(0));
    }
    
    out
}
```

**Estimated Effort**: 2 hours

---

### Phase 5: Block Editing (Foundation)

#### Task 5.1: Block Modification API
**File**: `engine_core/src/voxel.rs`

```rust
impl VoxelGrid {
    /// Add a block to the grid
    pub fn add_block(&mut self, block: VoxelBlock) -> Option<IVec3> {
        let pos = block.position;
        self.set_block(pos, block);
        Some(self.get_chunk_pos(pos))
    }
    
    /// Remove (mine) a block from the grid
    pub fn mine_block(&mut self, pos: BlockPos) -> Option<(VoxelBlock, IVec3)> {
        if let Some(block) = self.blocks.remove(&pos) {
            let chunk_pos = self.get_chunk_pos(pos);
            Some((block, chunk_pos))
        } else {
            None
        }
    }
    
    /// Place a block at a position
    pub fn place_block(
        &mut self,
        pos: BlockPos,
        block_type: BlockType,
        material: MaterialType,
    ) -> Option<IVec3> {
        let block = VoxelBlock::new(pos, block_type, material);
        self.add_block(block)
    }
}
```

**Future**: Wire to input system
```rust
// In main.rs or game logic
if player_action == Action::Mine {
    if let Some((block, chunk_pos)) = voxel_grid.mine_block(target_pos) {
        // Add resources to inventory
        if let Some(resource) = block.resource_data {
            inventory.add(resource.resource_type, resource.quantity);
        }
        
        // Mark chunk for mesh regeneration
        dirty_chunks.insert(chunk_pos);
    }
}
```

**Estimated Effort**: 1 hour

---

### Phase 6: Renderer Updates

#### Task 6.1: Register Voxel Mesh System
**File**: `src/main.rs`

```rust
// During renderer setup
let voxel_mesh_handle = {
    // Generate a representative chunk mesh or use placeholder
    let (verts, normals, indices) = generate_initial_voxel_mesh();
    renderer.register_indexed_mesh(&verts, &normals, &indices)
};
```

**Alternative**: Dynamic mesh registration per chunk
```rust
// When rendering VoxelChunks
for chunk in chunks {
    let mesh = chunk.generate_mesh();
    let handle = renderer.register_indexed_mesh(&mesh.verts, &mesh.normals, &mesh.indices);
    chunk.mesh_handle = handle;
}
```

**Estimated Effort**: 1-2 hours

---

## Comparison: Old vs New Plan

| Aspect | Original Plan | Revised Plan |
|--------|--------------|--------------|
| **Architecture** | Deformed mesh cubes | 3D voxel grid |
| **Editing** | Not supported | Built-in |
| **Block Types** | Single (deformed cube) | Multiple (cube, ramps, etc.) |
| **Storage** | Individual entities | Grid + chunks |
| **Optimization** | Per-tile transforms | Greedy meshing, face culling |
| **Resource Tracking** | Not supported | Per-block resource data |
| **Scalability** | Limited (entity count) | High (chunk-based) |
| **Future Features** | Limited | Mining, building, caves, etc. |

---

## Technical Considerations

### Coordinate Systems
- **Grid Coordinates**: `IVec3` (integer x, y, z)
- **World Coordinates**: `Vec3` (float x, y, z)
- **Conversion**: `world_pos = grid_pos.as_vec3()`

### Block Size
- Default: 1 unit = 1 block
- Consistent with existing Cube scaling

### Chunk Size
- Recommended: 16×16×16 blocks per chunk
- Balance between:
  - Too small: Overhead from many chunks
  - Too large: Slow mesh regeneration

### Memory Estimates
- **Single Block**: ~96 bytes (position, type, material, resource)
- **64×64 terrain, avg height 16**: ~65,536 blocks × 96 bytes = **6.3 MB**
- **Chunk mesh**: ~2-10 KB per chunk (depends on optimization)
- **Total for 64×64×32 world**: ~10-20 MB (very manageable)

---

## Rendering Optimization Strategies

### 1. Face Culling
Don't render faces between solid blocks
```rust
if neighbor_is_solid(grid, pos + face.normal()) {
    skip_face();
}
```
**Savings**: 50-80% fewer triangles

### 2. Greedy Meshing
Combine adjacent same-material quads
```rust
// Instead of: 100 blocks = 600 faces = 1200 triangles
// Generate: 100 blocks = 150 merged faces = 300 triangles
```
**Savings**: 60-90% fewer triangles

### 3. Chunk Frustum Culling
Only render chunks in camera view
```rust
if !chunk.bounds.intersects_frustum(camera) {
    skip_chunk();
}
```
**Savings**: ~75% of chunks not rendered

### 4. LOD (Future)
Use simpler meshes for distant chunks
- Close: Full detail
- Medium: Merge 2×2 blocks
- Far: Merge 4×4 blocks

---

## Terrain Type Examples

### 1. Gentle Hills (Default)
```rust
TerrainConfig {
    frequency: 0.05,
    amplitude: 8.0,
    octaves: 3,
    terrain_type: TerrainType::GentleHills,
}
```
**Result**: Smooth rolling landscape, natural ramps between heights

### 2. Mountains
```rust
TerrainConfig {
    frequency: 0.03,
    amplitude: 24.0,
    octaves: 5,
    terrain_type: TerrainType::Mountains,
}
```
**Result**: Dramatic peaks and valleys, steep cliffs

### 3. Plains
```rust
TerrainConfig {
    frequency: 0.1,
    amplitude: 2.0,
    octaves: 2,
    terrain_type: TerrainType::Plains,
}
```
**Result**: Mostly flat with gentle bumps

### 4. Cliffs
```rust
TerrainConfig {
    frequency: 0.05,
    amplitude: 12.0,
    octaves: 2,
    terrain_type: TerrainType::Cliffs,
}
```
**Result**: Stepped terrain with vertical faces (no ramps)

---

## Resource System Design

### Resource Types
```rust
pub enum ResourceType {
    Stone,
    Iron,
    Gold,
    Coal,
    Dirt,
    // Add more as needed
}
```

### Resource Distribution
```rust
fn add_resources(grid: &mut VoxelGrid) {
    for block in grid.iter_blocks_mut() {
        let resource = match block.position.y {
            0..=5 => Some(("stone", 1)),
            6..=10 => {
                if random() < 0.1 {
                    Some(("iron", 2))
                } else {
                    None
                }
            },
            _ => None,
        };
        
        if let Some((resource_type, quantity)) = resource {
            block.resource_data = Some(ResourceData {
                resource_type: resource_type.to_string(),
                quantity,
            });
        }
    }
}
```

---

## Testing Plan

### Unit Tests
```rust
#[test]
fn test_voxel_grid_operations() {
    let mut grid = VoxelGrid::new(16);
    let pos = BlockPos::new(0, 0, 0);
    let block = VoxelBlock::new(pos, BlockType::Solid, test_material());
    
    grid.set_block(pos, block);
    assert!(grid.get_block(&pos).is_some());
    
    grid.remove_block(&pos);
    assert!(grid.get_block(&pos).is_none());
}

#[test]
fn test_chunk_coordinate_conversion() {
    let grid = VoxelGrid::new(16);
    let pos = BlockPos::new(17, 5, -18);
    let chunk_pos = grid.get_chunk_pos(pos);
    
    assert_eq!(chunk_pos, IVec3::new(1, 0, -2));
}

#[test]
fn test_terrain_height_determinism() {
    let config = TerrainConfig::default();
    let noise = Perlin::new(42);
    
    let h1 = sample_height(&noise, 10, 10, &config);
    let h2 = sample_height(&noise, 10, 10, &config);
    
    assert_eq!(h1, h2); // Same input = same output
}
```

### Integration Tests
1. Generate terrain, verify no crashes
2. Add/remove blocks, verify grid updates
3. Render chunks, verify mesh generation
4. Performance: measure frame rate with different grid sizes

---

## Migration Path from Original Plan

### What Stays the Same
- ✅ Perlin noise for height generation
- ✅ Material system (Lambertian, Metal, Dielectric)
- ✅ ECS architecture (Legion)
- ✅ Renderer integration points

### What Changes
- ❌ No deformed cube meshes → Block-based meshes
- ❌ No individual tile entities → Chunk entities
- ❌ No static geometry → Editable terrain

### New Requirements
- ➕ 3D grid data structure
- ➕ Chunk subdivision system
- ➕ Block editing API
- ➕ Resource tracking system
- ➕ Ramp/slope mesh generation

---

## Future Roadmap

### Phase 7: Advanced Features (Not Immediate)
1. **Caves**: 3D noise for underground voids
2. **Water**: Transparent blocks with flow simulation
3. **Biomes**: Material/resource variation by region
4. **Trees/Vegetation**: Procedural structure generation
5. **LOD System**: Distance-based detail reduction
6. **Multiplayer**: Network sync for block changes
7. **Persistence**: Save/load voxel world state
8. **Advanced Building**: Stairs, slabs, walls, etc.

---

## Updated Estimated Effort

| Phase | Tasks | Time |
|-------|-------|------|
| 1. Foundation | Module setup, voxel grid, registries | 1-2 hours |
| 2. Mesh Generation | Cube mesh, smoothing algorithm, deformation | 3-4 hours |
| 3. Terrain Generation | Noise-based height, material/resource assignment | 2-3 hours |
| 4. ECS Integration | Chunks as entities, renderable trait | 2 hours |
| 5. Block Editing | Add/remove API foundation | 1 hour |
| 6. Renderer Updates | Mesh registration, rendering | 1-2 hours |
| **Total** | | **10-14 hours** |

---

## Open Questions Answered

1. **Visual Style**: ✅ Stylized blocks (confirmed)
2. **Terrain Type**: ✅ Gentle hills + configurable types
3. **Grid System**: ✅ 3D grid for tracking and editing
4. **Ramps**: ✅ Slope blocks between height transitions
5. **Resources**: ✅ Per-block resource data for mining
6. **Future Mining**: ✅ Architecture supports it

---

## Next Steps

**After Plan Approval**:
1. Review and approve this revised voxel-based plan
2. Create feature branch: `feature/voxel-terrain`
3. Implement Phase 1 (voxel module + grid)
4. Implement Phase 2 (mesh generation for cubes + ramps)
5. Implement Phase 3 (terrain generation with noise)
6. Implement Phase 4 (ECS integration)
7. Implement Phase 5 (block editing foundation)
8. Implement Phase 6 (renderer updates)
9. Testing and parameter tuning
10. Create pull request for review

---

## Summary of Changes (Revised)

**Major Architectural Changes**:
- ✅ **Voxel grid** with uniform block representation
- ✅ **No explicit block types** - all blocks are voxels
- ✅ **Automatic ramp generation** via smoothing algorithm
- ✅ **Separate material/resource registries** (data extracted from blocks)
- ✅ **Two-pass terrain generation** (placement → smoothing)
- ✅ **Chunk-based rendering** for optimization

**Simplified Design**:
- 🧊 **Single block type** - engine sees all as "voxels"
- 🎨 **Material IDs** instead of embedded material data
- 📦 **Resource IDs** for mining/gathering
- 🔄 **Smoothing algorithm** creates ramps automatically
- ⚡ **Less complexity** - easier to maintain

**Benefits**:
- 🎮 **Future-proof** for mining/building mechanics
- 🧩 **Simpler data model** - uniform voxel handling
- 🔧 **Editable** terrain at runtime
- 📦 **Resource system** via separate registry
- 🏔️ **Varied terrain** types supported
- ⛰️ **Smooth transitions** between heights

**Ready to proceed?** 🚀
