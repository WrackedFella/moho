# SG-F2 — Known Bugs

**Note:** Low priority — none block development
**Status:** Draft
**Labels:** feature, line:strategy

## Summary

Runtime defects with known symptoms. Resolve when the surrounding system is
next touched, or on a dedicated bug-fix pass.

## Items

| Item |
|---|
| [SG-F2-01 spawn-inside-terrain](SG-F2-01-spawn-inside-terrain.md) |
| [SG-F2-02 mining-mesh-gaps](SG-F2-02-mining-mesh-gaps.md) |
| [SG-F2-03 mining-not-persisted](SG-F2-03-mining-not-persisted.md) |

## Notes

A 2026-08 fix (smooth-terrain mesh offset, see [SG-F1](../SG-F1-core-interaction-loop/_feature.md)) was checked against
spawn-inside-terrain and mining-mesh-gaps as a possible shared cause — it
wasn't the fix for either; both still reproduce. Kept as separate bugs, not
merged, since their root causes are now confirmed distinct.
