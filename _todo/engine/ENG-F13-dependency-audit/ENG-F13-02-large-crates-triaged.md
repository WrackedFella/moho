# Large crates are triaged against the green light

**Feature:** [ENG-F13](_feature.md)
**Issue:** #86

## Summary

Large, professional-grade crates are kept on both verdicts unless a red flag
applies ([rubric](_feature.md#verdict-rubric)). This pass confirms that in one
line per crate, and escalates any with a red flag to a full two-verdict row.

## Deliverables

- A verdict row in [`audit.md`](audit.md) for each of: `wgpu`, `naga`, `egui`,
  `egui-winit`, `egui-wgpu`, `rapier3d`, `rodio`, `glam`, `serde`, `bytemuck`,
  `rand`, `noise`.

## Acceptance criteria

- [ ] Each crate either gets the green light (keep / keep, with the evidence
      that it is large and maintained) or names the red flag that blocks it.
- [ ] A red-flagged crate gets both verdicts and the lean-cost estimate the
      feature's exit criteria require.
- [ ] Known flags are addressed: the `glam` pin through `rapier3d`, `paste`
      via `rapier3d` (ENG-F2-04), `noise`'s duplicate `rand`, `wgpu`/`naga`
      blocked on `egui-wgpu`, and `rodio`'s MPL-2.0 codecs.
- [ ] Every verdict cites a fact from ENG-F13-01's table.
