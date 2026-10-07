# Mining requires an equipped tool

**Feature:** [SG-F1](_feature.md)

## Summary

Mining only works with a tool equipped. Hotbar slot 0 is reserved for the
equipped tool; slots 1-7 are resources. Player spawns with a tool equipped.

## Deliverables

- `Pawn.equipped_tool: Option<ToolId>`, `Pawn.selected_slot` — done.
- Number-key (1-8) slot selection, UI-gated — done.
- `Pawn::mine` gates on `equipped_tool` — done.
- Hotbar highlights the selected slot — done.

## Notes

`ToolId` is its own newtype (not reused from `resource_id`'s space), and
gating isn't coupled to exactly one equip slot — both deliberately shaped so
a later full equipment system (armor, other gear) generalizes this instead of
replacing it.

Not independently re-verified interactively this pass (no interactive input
in the dev environment) — implemented, tests green, needs a human `cargo run`
pass to confirm the on-screen behavior.
