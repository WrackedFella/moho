# 0015 — FPS rules are a platform-free crate; every shot resolves through one path

**Status:** Accepted; amended by [0031](0031-rounds-fly-as-stepped-segments.md)

## Context

The FPS line starts with its first crates ([ADR-0005](0005-crate-lines-and-dependency-direction.md)).
Phases 2-7 of the game design document build on the weapon and damage rules, and the
GDD separates rules from modes (waves, score, restart). A rules crate that names physics,
renderer or input types could not be tested headless or reused by a later mode, and a shot
path written for the player alone would be copied for enemies.

## Decision

1. **`moho_fps_rules` is in the fps and domain rows.** It holds weapon and body rules as plain,
   serialisable data and depends on no engine, physics, renderer, audio or input crate.
2. **`moho_fps` adapts.** It turns engine results (ray hits, input, level markers) into rule
   calls and presents the outcomes. It is the only FPS crate that reaches the engine.
3. **One shot path.** A shot resolves as: ray, surface tag, part, rule call. The shooter only
   chooses what the ray ignores; the rules never see who fired. Player and enemy shots share it.
4. **Modes sit above the rules.** Waves, score and restart live in `moho_fps` and read rule
   outcomes; `moho_fps_rules` names none of them.

## Consequences

- Rules are mutation-tested without a GPU or a window.
- A second mode, a headless simulation or a dedicated server reuses the rules unchanged.
- Cost: two crates for a small amount of code, and conversions between engine and rule types.
- The ADR-0005 table gains an fps row; the layering check keeps the rules crate off winit, wgpu and egui.
