# Split @group(0) by update frequency

**Feature:** [ENG-F3](_feature.md)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

`@group(0)` currently mixes camera, materials, lighting, SSAO views, and
dynamic lights. `set_material_table` rebuilds the whole group and silently
no-ops when SSAO isn't initialized.

## Deliverables

- Groups split by update frequency: camera+lighting (per-frame write, no
  rebuild), shadow textures (on resize/quality change), materials (on table
  change), SSAO (on resize/quality change), dynamic lights (per-frame write).
- Material updates no longer touch SSAO state and vice versa.
