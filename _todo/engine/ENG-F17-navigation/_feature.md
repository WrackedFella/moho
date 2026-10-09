# ENG-F17 — Agents path over any static level

**Issue:** [#229](https://github.com/WrackedFella/moho/issues/229)
**Status:** Backlog
**Labels:** feature, line:engine

## Summary

AI moves through non-voxel levels. Until this lands, FPS Phase 1 AI can use waypoints. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- A navmesh is built from ENG-F10 meshes.
- Test: on a fixture level with an obstacle, a path query between two points returns a path that avoids it.

## Scope

- Out: cover-point selection; crowds; dynamic obstacles.

## Items

| Item |
|---|
