# ENG-F16 — Sounds play from positions


## Summary

The player can tell where a sound came from. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- A listener transform plus positioned emitters, with distance attenuation and stereo panning (tests on gain values).
- Existing non-positional sounds are unchanged.

## Scope

- Out: occlusion; reverb.

## Items

| Item |
|---|
