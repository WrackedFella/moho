# Hotbar displays the player's mined resources

**Feature:** [SG-F1](_feature.md)

## Summary

The 8-slot hotbar shows real inventory counts instead of empty placeholders.

## Deliverables

- `HudData.hotbar: Vec<(u32, u32)>` (denormalized — `moho_ui` has no
  `moho_game` dependency) — done.
- `render_hotbar` renders populated slots as `R<id>` + count — done.

## Notes

`determine_resource_id` excludes the top ~4 blocks of any column from ore
eligibility — surface scraping alone never yields a resource. Worth a tuning
pass if new players find this confusing.
