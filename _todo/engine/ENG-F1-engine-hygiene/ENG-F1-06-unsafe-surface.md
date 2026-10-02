# Unsafe code is minimal and every remaining block is sound

**Status:** not started
**Feature:** ENG-F1

## Summary

The workspace denies `unsafe_code`; five sites carry scoped allows. Two can be
removed outright and one impl is likely unnecessary.

## Deliverables

- `moho_ui` render pass: replace the `transmute` to `RenderPass<'static>` with
  wgpu's safe `RenderPass::forget_lifetime()` (available in wgpu 29).
- `EguiAdapter`: drop `unsafe impl Sync` if nothing needs it (`Mutex<T>: Sync`
  only requires `T: Send`); re-justify or remove `unsafe impl Send` (add a
  `Send` bound to `dyn Modal` instead, if feasible).
- Raw frame-callback path removed (tracked in ENG-F3-03; it is unsound, not
  just unnecessary).
- Remaining `unsafe` is limited to the `wgpu-experimental` opt-in.
