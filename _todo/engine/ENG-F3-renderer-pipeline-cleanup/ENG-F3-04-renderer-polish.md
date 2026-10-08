# Renderer polish (independently mergeable)

**Feature:** [ENG-F3](_feature.md)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Small, unrelated renderer cleanups; land any of these independently, any time.
Split from ENG-F3-03, which kept only the unsound frame-callback deletion.

## Deliverables

- Hoist `InterleavedVertex` (currently redefined inline with hand-tuned
  padding) to a single top-level type.
- Gate per-vertex sample logging in `register_indexed_mesh`; it runs every
  chunk registration even when filtered.
- Replace `MaterialDesc::from_material`/`into_material_type` boilerplate with
  a `serde` derive on `MaterialType`.
- `MAX_SHADOW_LIGHTS` and initial instance-capacity configurable via
  `RendererBuilder`.
- Wrap `pending_frame`/`pending_frame_view`/`pending_draws` in one `Frame`
  struct so they can't desync.
- `PipelineSetup` holds `_shadow_pass_bgl`/`_csm_pass_bgl` as dead
  `#[allow(dead_code)]` fields while shadow setup builds its own layouts.
  Expose them via accessors and have shadow construction use them (a stale
  agent worktree prototyped this; it was dropped rather than rebased).
