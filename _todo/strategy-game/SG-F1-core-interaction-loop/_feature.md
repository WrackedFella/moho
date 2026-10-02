# SG-F1 — Core Interaction & Resource Loop

**Status:** in progress (3/4 items done)

## Summary

Aim → mine a voxel → resource lands in inventory → hotbar shows it → mining
requires an equipped tool. The minimum playable resource loop.

## Items

| Item | Status |
|---|---|
| [SG-F1-01 mine-voxel](SG-F1-01-mine-voxel.md) | done |
| [SG-F1-02 hotbar-display](SG-F1-02-hotbar-display.md) | done |
| [SG-F1-03 tool-gating](SG-F1-03-tool-gating.md) | done |
| [SG-F1-04 pickup-feedback](SG-F1-04-pickup-feedback.md) | not started |

## Notes

Long-term: mining moves to sub-grid voxel deformation (swap a full block for
deformable sub-blocks), not whole-block removal. The seam is `Pawn::mine`'s
internal block-destruction call — its signature and the inventory-deposit
contract (`ResourceYield { resource_id, amount }`) must not assume whole-block
granularity. Don't design future items against that assumption.
