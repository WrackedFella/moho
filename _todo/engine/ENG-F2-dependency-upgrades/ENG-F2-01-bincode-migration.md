# bincode 2 → 3

**Status:** not started — blocked on a decision
**Feature:** ENG-F2

## Summary

bincode 3 changed its default encoding; existing `saves/scene.bin` and chunk
files won't deserialize without a plan.

## Deliverables

- Decision: break old saves (small PR, fine if no long-lived saves exist) vs.
  migration shim (detect format on read, rewrite on next save) vs. defer.
- Implementation matching the chosen option.
- Save round-trip test under the new format.

## Notes

Recommendation: break old saves, if acceptable — smallest change.
