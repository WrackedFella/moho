# Mining shows a pickup popup

**Feature:** [SG-F1](https://github.com/WrackedFella/moho/issues/311)
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

A possessed pawn that mines a block sees what it got: a short "+N resource" popup. Mining that yields nothing shows nothing.

## Deliverables

- A fading popup per pickup; concurrent pickups stack.
- A pickup cue (placeholder clip acceptable).
- One interactive playtest of the shipped hotbar and tool gating, noted on the issue.

## Acceptance criteria

```gherkin
Scenario: A mined block shows a pickup popup
  Given a possessed pawn mines a block yielding 1 of a resource
  When the block breaks
  Then a "+1" popup for that resource appears and fades

Scenario: Several pickups stack instead of overwriting
  Given a possessed pawn mines two blocks in quick succession
  When both break
  Then both popups are visible at once

Scenario: A mine that yields nothing shows no popup
  Given a possessed pawn mines a block that yields nothing
  When the block breaks
  Then no popup appears

Scenario: Mining without a tool is refused
  Given the possessed pawn has no tool equipped
  When the player mines a block
  Then the block remains
  And the inventory is unchanged

Scenario: A mined block lands in the hotbar
  Given the possessed pawn has a tool equipped
  When the player mines a block yielding a resource
  Then the hotbar shows that resource count increased
```

## Verification

Manual, once: `cargo run`, equip a tool, mine several blocks, confirm popup, cue and hotbar. Repeat unequipped. Tool gating has never been checked interactively.
