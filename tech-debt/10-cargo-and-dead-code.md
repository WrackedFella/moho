# TD-10: Cargo Hygiene and Dead Code Cleanup

**Priority:** Low — cosmetic, but each item is a 5-minute fix and the accumulation
degrades signal-to-noise for future contributors.

## Problem

### A. Duplicate bytemuck version spec

`Cargo.toml` workspace declares `bytemuck = "1.24"` but the binary's
`[dependencies.bytemuck]` also specifies `version = "1.9.1"`. The workspace dep
rule exists precisely to avoid this. One should be authoritative.

```toml
# Cargo.toml workspace.dependencies
bytemuck = "1.24"

# Also in Cargo.toml [dependencies.bytemuck] — this should just be:
# [dependencies.bytemuck]
# workspace = true
# features = ["derive"]
```

### B. Dead fields with `#[allow(dead_code)]`

| Field | Location | Action |
|---|---|---|
| `mouse_sensitivity` | `App` struct, `src/main.rs:121` | Remove or wire to prefs |
| `vertex_buffer` | `gfx::wgpu_impl::Renderer`, `moho_renderer/src/lib.rs:74` | Remove (prefixed `_`) |
| `vertex_count` | Same | Remove |
| `window` (renderer) | Same, `moho_renderer/src/lib.rs:67` | Already prefixed `_window`; confirm unused and remove |
| `voxel_grid: Option<VoxelGrid>` | `App` struct `src/main.rs:77` | Remove — `None` always, grid is in `LightSystem` |

### C. Public physics fields

`moho_physics::PhysicsWorld` exposes `rigid_body_set`, `collider_set`,
`query_pipeline`, `character_body`, `character_collider`, `vertical_velocity`,
`is_grounded`, `noclip` as `pub`. Most of these are accessed directly in
`src/app/event_loop/event_processor.rs`. Leaking rapier3d internals makes
`moho_physics` impossible to use as a stable API crate.

Minimum fix: add accessor methods for the fields actually used externally, then make
internal fields private.

### D. `InputRouter` re-exports stale `GameState`

`moho_ui/src/adapter/mod.rs` has this in a comment block:

```rust
/// Re-export from moho_types — single source of truth for game states.
pub use moho_types::GameState;
```

But it's in a `///` doc comment on a `pub struct ProgressState`, not actually a
re-export. This is a stale comment fragment from a previous refactor.

## Acceptance Criteria

1. `bytemuck` in the binary's `[dependencies]` uses `workspace = true` with
   `features = ["derive"]` added. Version spec removed from that section.
2. All `#[allow(dead_code)]` fields in the table above are removed (or wired up
   if they represent genuinely missing functionality that should exist).
3. `PhysicsWorld` internal rapier fields are private; public accessor methods
   cover the usage in `event_processor.rs`.
4. Stale comment in `moho_ui/src/adapter/mod.rs:85` removed.

## Files

- `Cargo.toml` — bytemuck dep
- `src/main.rs` — `App::mouse_sensitivity`, `App::voxel_grid`
- `moho_renderer/src/lib.rs` — `Renderer::vertex_buffer`, `vertex_count`, `_window`
- `moho_physics/src/world.rs` — pub fields
- `moho_ui/src/adapter/mod.rs` — stale comment
