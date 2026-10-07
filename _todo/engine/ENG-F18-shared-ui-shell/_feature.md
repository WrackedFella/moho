# ENG-F18 — Both games share the UI shell


## Summary

The FPS needs console, settings and menus without depending on the strategy UI. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- egui integration, console, settings widgets and the modal stack sit on the engine line.
- Strategy HUD and menus stay on the strategy line.
- The layering check passes; strategy UI is unchanged (manual check).

## Scope

- Out: the FPS HUD.

## Notes

- Lands after [ENG-F20](../ENG-F20-graphics-upgrade-touches-one-crate/_feature.md), which
  contains wgpu in the renderer first; add no new wgpu use outside it.

## Items

| Item |
|---|
