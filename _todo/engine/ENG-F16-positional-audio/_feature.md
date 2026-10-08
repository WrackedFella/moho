# ENG-F16 — Sounds play from positions

**Status:** Draft
**Labels:** feature, line:engine

## Summary

The player can tell where a sound came from. Consumer: [FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §10; strategy is a likely second consumer.

## Exit criteria

- A listener transform plus positioned emitters, with distance attenuation and stereo panning (tests on gain values).
- Existing non-positional sounds are unchanged.

## Scope

- In: an engine-owned gain and pan layer over `rodio` (ENG-F13 call: `rodio`
  stays; its built-in spatial model is too crude), testable as plain functions.
- Out: occlusion; reverb. Needing either reopens the audio choice (`kira` is the
  audited fallback).

## Items

| Item |
|---|
