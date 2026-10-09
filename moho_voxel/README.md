# moho_voxel

Chunk-based voxel terrain for the strategy line: grid, meshing, LOD, lighting,
chunk store and streaming. The engine never depends on it; terrain reaches the
renderer and physics as meshes ([ADR-0010](../_todo/adr/0010-world-geometry-is-a-mesh-contract.md)).

## Architecture

- `VoxelGrid` - Stores block data in 3D grid
- `VoxelChunk` - Fixed-size 16×16×16 chunk
- `BlockData` - Snapshot of a stored block's position, material and resource

## Usage

```rust
use moho_voxel::{BlockPos, VoxelGrid};

// Create a grid (chunk size must be 16)
let mut grid = VoxelGrid::new(16);

// Place a block through the mutator: position, material id, optional resource id
grid.mutator().place(BlockPos::new(32, 10, 32), 0, None);
```

## Testing

```bash
just test -E 'package(moho_voxel)'
```

## Thread safety

- **VoxelGrid**: Not thread-safe (chunk generation is parallelizable externally)
