# TD-12: Fix Docs Drift

**Priority:** Low — docs that contradict code erode trust in all docs.

## Problem

Several documentation inconsistencies have accumulated:

### A. Status contradictions in docs/README.md

- Describes the project as "Production Ready" in the header.
- Under Key Metrics, lists "❌ No rendering" and "After Custom Mesh Rendering
  (Expected)" projections alongside the "Production Ready" label.
- These cannot both be true.

### B. Broken doc links in README.md

The following links are listed in `README.md` under "Architecture Documentation" but
the target files do not exist in `docs/`:

- `docs/GAME_STATE_ARCHITECTURE.md` — not found
- `docs/engine_core/EVENT_BUS_TESTING_NOTES.md` — referenced in `docs/README.md`
  but not found in the repo

### C. docs/README.md "Last Updated: November 17, 2025"

Physics integration, terrain work, and several phases have completed since then. The
"Next Steps" section lists "Multiplayer Foundation" but the roadmap is on Phase 5
(terrain overhaul). Minor, but makes the docs feel unmaintained.

### D. README.md feature flag mismatch

The Quick Start and Build sections reference `--features "backend-wgpu,ui-egui"`:
```sh
cargo run --features "backend-wgpu,ui-egui"
```
But those features don't exist in `Cargo.toml`. The correct invocation is just
`cargo run` (or `cargo run --features ui-egui-debug` for debug panels). Any new
contributor following the README will immediately hit a compile error.

### E. `moho_ui/src/screens/menu.rs:147` stale TODO

```rust
/// TODO: Remove this alias in a future refactor once all code uses Screen directly.
```
This should either be done or scheduled in a work item rather than sitting
inline indefinitely.

## Acceptance Criteria

1. `docs/README.md` status header reflects the actual current state (engine in active
   development, rendering works, physics integrated, terrain overhaul in progress).
2. Broken links are removed or replaced with correct paths.
3. `README.md` Quick Start corrects the `cargo run` invocation to remove
   non-existent feature flags.
4. "Last Updated" in docs is either automated or removed (stale timestamps are worse
   than no timestamp).
5. The `moho_ui/src/screens/menu.rs` alias TODO is either resolved or tracked as
   a work item (TD-05 Phase B is the natural cleanup point when UI screens are
   standardized).

## Files

- `README.md`
- `docs/README.md`
- `moho_ui/src/screens/menu.rs`
