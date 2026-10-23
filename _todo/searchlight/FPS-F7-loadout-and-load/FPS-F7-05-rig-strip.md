# The rig strip shows after a reload or switch and on the check-gear key

**Feature:** FPS-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:fps

## Summary
A small strip shows what each rig slot holds after a reload or weapon switch, then
fades; holding the check-gear key shows it on demand (GDD §3.3). It tells the player
what their next reload will take without an ammo count. Depends on #595 (FPS-F6-01).

## Deliverables
- A HUD strip listing the rig's slots and what each holds.
- The strip appears after a reload or weapon switch and fades after a time in data.
- Holding the check-gear key shows the strip until it is released.
- Magazines show as present or empty, never a round count.

## Acceptance criteria
```gherkin
Scenario: The strip shows after a reload
  Given the rig holds 2 full magazines and 1 empty one
  When the player finishes a reload
  Then the strip shows 3 slots, one of them empty

Scenario: The strip fades
  Given the strip is showing after a reload
  When the fade time passes with no reload, switch or check-gear
  Then the strip is hidden

Scenario: Holding check-gear keeps the strip up
  Given the strip is hidden
  When the player holds the check-gear key past the fade time
  Then the strip is showing until the key is released

Scenario: The strip never counts rounds
  Given a rig slot holds a half-full magazine
  When the strip shows
  Then the slot shows a magazine and no round count
```

## Verification
- Fight a wave: the strip helps judge the next reload and does not distract. Record the
  fade time tried on this issue.

## Notes
- Default: an empty magazine reads as empty, any other as present. Whether the strip
  shows a fill level (full, about half) is Justin's call after playtest.
