# 0012 — Engine capabilities land as modules first; the crate map for M1–M2

**Status:** Proposed

## Context

M1 and M2 add six engine capabilities: the shared loop ([ENG-F11](../engine/ENG-F11-shared-app-loop/_feature.md)), the UI
shell ([ENG-F18](../engine/ENG-F18-shared-ui-shell/_feature.md)), scene import ([ENG-F14](../engine/ENG-F14-scene-import/_feature.md)), navigation ([ENG-F17](../engine/ENG-F17-navigation/_feature.md)), content
roots ([ENG-F19](../engine/ENG-F19-data-and-mod-content/_feature.md)) and the save envelope ([ADR-0006](0006-save-format-contract.md)). [ADR-0005](0005-crate-lines-and-dependency-direction.md)'s table
assigns today's crates to lines but has no home for any of them. Without a map,
each spec places its own crate, and the likely outcomes are a dumping-ground
`moho_core` or one crate per feature.

Two existing gaps make that worse:

- `moho_core` has no stated end state. Once voxels leave ([ADR-0010](0010-world-geometry-is-a-mesh-contract.md),
  [ENG-F10](../engine/ENG-F10-world-geometry-from-any-source/_feature.md)), it holds the event bus, engine-wide events, prefs and input filters.
- Input is split three ways: mouse-delta filtering in `moho_core::input`, the
  key-to-binding-code map in `moho_input`, and state handling in the binary.
  `moho_types` holds only the strategy game's `GameState` and its transition
  coordinator.

[ENG-F5](../engine/ENG-F5-physical-repo-split/_feature.md)'s gate asks that FPS work needs "no engine API change", which has no
list of APIs to check against.

## Decision

**Modules first.** A capability lands as a module of an existing engine crate.
It gets its own crate only for a deployable, reuse or compile-time boundary
([ADR-0005](0005-crate-lines-and-dependency-direction.md)'s rule), named in its spec.

The engine row of [ADR-0005](0005-crate-lines-and-dependency-direction.md)'s table is extended to this target:

| Crate | Owns | Platform-free | Arrives with |
|---|---|---|---|
| `moho_app` (new) | window, event loop, renderer/physics/audio setup, the [ADR-0009](0009-simulation-time-is-one-fixed-tick.md) accumulator, `GameClock`, the game plug-in interface | no | ENG-F11 |
| `moho_core` | event bus, engine-wide events, prefs; modules for content roots and the save envelope | yes (domain set) | ENG-F10 leaves this; ENG-F19, ADR-0006 add modules |
| `moho_input` | action map, gamepad, bindings as data, mouse-delta filtering (absorbs `moho_core::input`) | no | ENG-F12 |
| `moho_ui_shell` (new, working name) | egui integration, console, settings widgets, modal stack | no | ENG-F18 |
| `moho_render_api`, `moho_renderer`, `moho_audio`, `moho_physics` | as today | no | |
| `moho_types` | deleted; `GameState` moves to the strategy line | | ENG-F12 |

Placement of the rest:

- **Scene import** (ENG-F14) and **navigation** (ENG-F17) are modules of an
  existing engine crate, unless [ENG-F13](../engine/ENG-F13-dependency-audit/_feature.md)'s verdict on `gltf` or the navmesh
  crate makes a compile-time boundary worth a crate. Each spec records which.
- **Character controller and camera** (proposed ENG-F21, generic controller and
  camera): its spec picks the home, within the engine row.
- **Mouse-delta filtering** goes to `moho_input`, not `moho_app`: it turns
  device deltas into the look delta that [ADR-0011](0011-simulation-stays-network-ready.md)'s per-tick command
  carries, which is the action map's job. `moho_app` only forwards device
  events.
- **The UI shell** is its own crate because egui cannot enter the platform-free
  `moho_core`. `moho_ui` keeps the strategy HUD and menus on the strategy line.

**Seam list.** These are the engine's public seams, and the APIs ENG-F5's "no
engine API change" criterion refers to:

1. the game plug-in interface (`moho_app`)
2. the world-geometry contract ([ADR-0010](0010-world-geometry-is-a-mesh-contract.md))
3. the action map (`moho_input`)
4. physics queries ([ENG-F15](../engine/ENG-F15-physics-queries-and-bodies/_feature.md))
5. spatial audio ([ENG-F16](../engine/ENG-F16-positional-audio/_feature.md))
6. content roots (ENG-F19)
7. the save envelope ([ADR-0006](0006-save-format-contract.md))
8. scene import (ENG-F14)

## Consequences

- `scripts/layering.txt` gains each new crate's row in that crate's PR
  (`moho_app`, the UI shell), and loses `moho_types` when it is deleted.
- A module is promoted to its own crate only by a new ADR naming the boundary.
- `moho_core`'s end state is a small, platform-free base: bus, engine-wide
  events, prefs, content roots, save envelope. Anything else proposed for it
  needs a reason it cannot live in a more specific crate.
- Adding or changing a seam is an ADR-level change, so ENG-F5's gate can be
  checked against a fixed list.
- `moho_physics` declares an unused `moho_core` dependency; it is dropped when
  ENG-F15 next touches the crate.
- The target crate graph goes into `wiki/architecture/overview.md` once Justin
  approves the new wiki content.
- Cost: `moho_input` stops being a small leaf and becomes the engine's input
  layer, and the UI shell crate name is settled in the ENG-F18 spec.
