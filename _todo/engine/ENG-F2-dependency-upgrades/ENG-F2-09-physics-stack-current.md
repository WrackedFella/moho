# The physics stack is on current rapier3d

**Feature:** [ENG-F2](_feature.md)
**Issue:** #102

## Summary

ENG-F13 found `rapier3d` four minors behind (0.32 vs 0.36). Its `glamx`
dependency is what pins `glam` at 0.30, and its math stack is what brings in
`paste` ([ENG-F2-04](ENG-F2-04-paste-advisory-cleared.md)). This is the
M1-close upgrade under the upgrade policy.

## Deliverables

- `rapier3d` on the latest release; `glam` on the newest version the stack
  then allows.
- The `glam` watchlist entry and ENG-F2-04's blocker note are updated with
  what the upgrade showed.

## Acceptance criteria

- [ ] `just check` and `just deny` pass.
- [ ] Physics tests pass (character controller, colliders).
- [ ] The workspace builds a single `glam` version.

## Verification

- The player walks, jumps and collides with terrain as before.
