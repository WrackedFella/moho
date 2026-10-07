# ENG-F18 — Both games share the UI shell


## Summary

The FPS needs console, settings and menus without depending on the strategy UI. Consumer: FPS GDD v0.2 §10 (pending on `dev`); strategy is a likely second consumer.

## Exit criteria

- egui integration, console, settings widgets and the modal stack sit on the engine line.
- Strategy HUD and menus stay on the strategy line.
- The layering check passes; strategy UI is unchanged (manual check).

## Scope

- Out: the FPS HUD.

## Items

| Item |
|---|
