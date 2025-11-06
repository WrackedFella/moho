# moho_renderer

Backend-agnostic 3D renderer for the Moho engine. Provides instance-based mesh rendering, cascaded shadow mapping, dynamic lighting, and skybox rendering using wgpu.

## Features

- **Instance Rendering** - Efficient batched rendering of multiple instances per mesh
- **Custom Mesh Support** - Register arbitrary vertex/index buffers
- **Cascaded Shadow Maps (CSM)** - 4 cascades covering 0-800 world units
- **Dynamic Lighting** - Sun and moon directional lights with time-based ambient
- **Skybox Rendering** - Atmospheric rendering with dynamic sky colors
- **Material System** - PBR-style materials with transparency support
- **Scene Serialization** - Save/load scene data to disk

## Architecture

The renderer is built with a clean separation between scene data and GPU backend:

- **`Scene`** - High-level scene representation (materials, meshes, instances)
- **`Renderer`** - GPU backend implementation (wgpu)
- **`MaterialTable`** - Material management with deduplication
- **`ShadowSystem`** - Cascaded shadow map generation and management

## Quick Start

### Creating a Renderer

```rust
use moho_renderer::create_renderer_from_arc;
use std::sync::Arc;

let window = Arc::new(winit::window::WindowBuilder::new().build(&event_loop)?);
let mut renderer = create_renderer_from_arc(&window)?;
```

**Lifetime Constraint:** The `wgpu::Surface` borrows the window, so the renderer lifetime is tied to the window. Keep the window `Arc` alive for the renderer's lifetime.

### Registering Meshes

```rust
// Simple triangle list
let vertices = vec![
    [-0.5, -0.5, 0.0],
    [ 0.5, -0.5, 0.0],
    [ 0.0,  0.5, 0.0],
];
let mesh_id = renderer.register_mesh(&vertices);

// Indexed mesh (more efficient)
let indices = vec![0, 1, 2, 2, 3, 0];
let mesh_id = renderer.register_indexed_mesh(&vertices, &indices);
```

### Setting Materials

```rust
use moho_renderer::MaterialGpu;

let materials = vec![
    MaterialGpu {
        albedo: [1.0, 0.0, 0.0, 1.0], // Red
        params: [0.5, 1.5, 0.0, 0.0], // fuzz, ref_idx, transparent, unused
    },
    MaterialGpu {
        albedo: [0.0, 1.0, 0.0, 1.0], // Green
        params: [0.0, 1.0, 0.0, 0.0],
    },
];

renderer.set_material_table(&materials);
```

### Rendering Instances

```rust
// Render 100 cubes at different positions
renderer.render_mesh(
    mesh_id,
    &instances, // Vec<[f32; 16]> - 4×4 transform matrices
    &materials, // Vec<u32> - material indices per instance
);
```

### Updating Lighting

```rust
use moho_renderer::LightingGpu;

let lighting = LightingGpu {
    sun_direction: [0.0, -1.0, 0.0, 1.0],     // Down, intensity 1.0
    sun_color: [1.0, 0.95, 0.8, 0.0],         // Warm sunlight
    moon_direction: [0.0, 1.0, 0.0, 0.3],     // Up, intensity 0.3
    moon_color: [0.7, 0.8, 0.9, 0.0],         // Cool moonlight
    ambient: [0.1, 0.1, 0.15, 0.15],          // Ambient color + intensity
    time_of_day: [12.0, 0.0, 0.0, 0.0],       // Noon
};

renderer.update_lighting(lighting);
```

### Frame Rendering

```rust
// Called each frame by the rendering system
renderer.begin_frame(&camera)?;
renderer.render_scene(&scene)?;
renderer.present_frame()?;
```

## Cascaded Shadow Maps

The renderer implements 4-cascade CSM for high-quality shadows across large view distances.

**Configuration:**
- **Cascades**: 4 (covering near to far terrain)
- **Resolution**: 4096×4096 per cascade
- **Split Distances**: [50, 150, 400, 800] world units
- **Format**: Depth32Float
- **PCF Filtering**: 2×2 sample pattern for soft shadows

**Shadow Coverage:**
- Cascade 0: 0-50 units (high detail near camera)
- Cascade 1: 50-150 units (medium detail)
- Cascade 2: 150-400 units (distant terrain)
- Cascade 3: 400-800 units (far horizon)

Only the sun casts shadows. Moon does not cast shadows for performance reasons.

## GPU Data Structures

See `../docs/gpu_abi.md` for detailed structure layouts. Key types:

### MaterialGpu (32 bytes)
```rust
pub struct MaterialGpu {
    pub albedo: [f32; 4],    // RGBA color
    pub params: [f32; 4],    // fuzz, ref_idx, transparent, unused
}
```

### LightingGpu (96 bytes)
```rust
pub struct LightingGpu {
    pub sun_direction: [f32; 4],   // xyz + intensity
    pub sun_color: [f32; 4],       // RGB + unused
    pub moon_direction: [f32; 4],  // xyz + intensity
    pub moon_color: [f32; 4],      // RGB + unused
    pub ambient: [f32; 4],         // RGB + intensity
    pub time_of_day: [f32; 4],     // hours (0-24) + unused
}
```

### CameraGpu (80 bytes)
```rust
pub struct CameraGpu {
    pub view_proj: [[f32; 4]; 4],  // 4×4 view-projection matrix
    pub camera_pos: [f32; 4],      // xyz + unused
}
```

## Skybox System

Dynamic skybox with time-of-day color transitions.

**Sky Color Periods:**
- **Night** (0-5h, 21-24h): Dark blue atmosphere
- **Dawn** (5-7h): Orange/pink sunrise gradient
- **Day** (7-17h): Bright blue sky
- **Dusk** (17-21h): Red/orange sunset gradient

Transitions use smoothstep for natural blending between periods.

## Scene Serialization

Save and load complete scene state:

```rust
use moho_renderer::Scene;

// Save
let scene = Scene::new();
// ... populate scene ...
scene.save_to_file("scene.bin")?;

// Load
let scene = Scene::load_from_file("scene.bin")?;
```

Scene data includes materials, mesh references, and instance transforms.

## Performance Characteristics

- **Instance Rendering**: 10,000+ instances at 60 FPS
- **Shadow Map Generation**: 4 cascades in ~2-3ms
- **Material Switches**: Minimal overhead (single buffer)
- **Mesh Registration**: One-time CPU→GPU transfer

## Backend Support

Currently supports:
- **wgpu** (default) - Vulkan, Metal, DirectX 12, WebGPU

Enable with cargo feature: `backend-wgpu` (enabled by default)

## Logging

The crate uses the `log` facade. Initialize a logger to see renderer output:

```rust
env_logger::init(); // Or any log implementation
```

Set log level:
```bash
RUST_LOG=moho_renderer=debug cargo run
```

## Testing

```bash
# Run all tests (including shader validation)
cargo test --package moho_renderer

# Test scene serialization
cargo test --test save_load

# Validate WGSL shaders
cargo test --test shaders_validation
```

## Shader Files

Located in `shaders/`:
- `common.wgsl` - Shared structures (Camera, Lighting, Material)
- `vertex.wgsl` - Vertex shader with instance transforms
- `fragment.wgsl` - PBR-style fragment shader with CSM shadows
- `skybox.wgsl` - Skybox vertex/fragment shaders
- `shadow.wgsl` - Shadow map generation

## Thread Safety

- **`Renderer`**: Not `Send` or `Sync` (wgpu requires main thread)
- **`Scene`**: `Send + Sync` (can be constructed on worker threads)
- **`MaterialTable`**: `Send + Sync`

Always create and use the renderer on the main thread with the winit event loop.

## Future Plans

- Deferred rendering pipeline
- Point and spot lights
- Post-processing effects (bloom, SSAO)
- Multiple shadow-casting lights
- Instanced shadow rendering
- Level-of-detail (LOD) system

