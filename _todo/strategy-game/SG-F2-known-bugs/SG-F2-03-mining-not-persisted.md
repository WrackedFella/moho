# Mining edits aren't reliably persisted

**Status:** not started
**Feature:** [SG-F2](_feature.md)

## Summary

Blocks removed by mining can revert on save/reload. Two persistence paths
exist (whole-scene autosave from live grid state; per-chunk save-on-eviction)
and mining likely falls into a gap between them.

## Deliverables

- Confirm whether eviction-save relies on a per-chunk "modified" flag that
  `Pawn::mine` never sets; if so, set it there too.
- If no such flag exists, trace an actual mine → walk away → walk back repro
  before committing to a fix.

## Notes

Files: `src/app/chunk_streamer.rs` (`evict_distant`), `src/app/autosave.rs`.
