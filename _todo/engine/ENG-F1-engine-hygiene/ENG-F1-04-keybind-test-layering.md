# Collapse redundant keybind-conflict test layers

**Status:** not started
**Feature:** ENG-F1

## Summary

Keybind conflict/capture behavior is tested at 3 overlapping tiers
(`keybind_capture/mod.rs` unit tests, `SettingsMenu` unit tests, and
`moho_ui/tests/settings_menu.rs` integration) — all substantially re-confirm
the same conflict→modal→confirm/cancel flow with no unique coverage at the
top tier.

## Deliverables

- Collapse to two tiers: core-logic unit tests + one integration test through
  `SettingsMenu`. Drop whichever middle layer is cheaper to lose.
