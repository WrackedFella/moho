# Uncalled UI and voxel helpers are deleted; HUD data reaching overlays is pinned

**Feature:** ENG-F22
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

Five public helpers have no production caller (some only a test), and the one path
that feeds HUD data to overlays has no test. Delete the helpers with their tests, per
ENG-F22's scope ("deleting small production items whose only caller is a test"), and
pin the HUD path by what overlays receive, not by a setter-getter round trip.

## Deliverables

- Deleted, with any test that exists only for them: the console's public clear (the
  `clear` command keeps working through its own path), the settings menu's active-tab
  getter, the overlay manager's HUD-data getter and mutable getter, and the light
  budget's conservative and aggressive presets (the default stays).
- A test that overlays render with the HUD data most recently supplied.

## Acceptance criteria

Checklist:
- [ ] Each helper above is gone and the workspace builds with no new dead-code allow.
- [ ] The console `clear` command still empties the output (existing test stays green).

```gherkin
Scenario: Overlays render with the latest HUD data
  Given an overlay manager with a registered overlay
  And HUD data A was supplied, then HUD data B
  When the overlays render
  Then the overlay receives HUD data B
```

## Notes

- `VoxelChunk::is_uploaded`, listed with these in the #210 follow-ups, is already gone
  on `dev`.
