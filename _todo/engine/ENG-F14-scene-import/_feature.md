# ENG-F14 — A game loads a static scene from a file


## Summary

A level authored in a tool loads as meshes, colliders and named markers; the game decides what a marker means. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- A scene file (glTF unless ENG-F13 decides otherwise) yields meshes registered through the ENG-F10 contract, colliders, and named nodes with transforms and metadata.
- Test: a fixture with two meshes and one named marker loads both meshes and the marker with its transform.
- Rigid characters render as moving meshes with transforms.

## Scope

- Out: map semantics (FPS line); streaming; skinning.

## Items

| Item |
|---|
