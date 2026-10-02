# Break up oversized renderer constructors

**Status:** not started
**Feature:** ENG-F1

## Summary

`moho_renderer/src/shadow.rs::new` (290 lines) and `ssao.rs::new` (218 lines)
are the longest functions in the codebase — real init work, not just wiring.

## Deliverables

- Both split into named builder stages, mirroring `RendererBuilder`'s
  `init_device → create_pipelines → allocate_resources → build` pattern.
