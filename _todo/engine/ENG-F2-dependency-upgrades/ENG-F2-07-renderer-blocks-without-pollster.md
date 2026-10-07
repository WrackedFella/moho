# Renderer device setup blocks on async without pollster

**Feature:** [ENG-F2](_feature.md)
**Issue:** #100

## Summary

ENG-F13 call: replace `pollster`'s two `block_on` calls in renderer device
setup with an in-house `block_on` of about 25 lines
([audit](../ENG-F13-dependency-audit/audit.md)).

## Deliverables

- No workspace crate declares `pollster`.
- An engine-owned `block_on` drives the adapter and device requests.

## Acceptance criteria

- [ ] A future that is already ready completes on the first poll.
- [ ] A future that is pending until woken from another thread completes after the wake.
- [ ] The renderer initialises as before (existing renderer tests pass; app starts).
