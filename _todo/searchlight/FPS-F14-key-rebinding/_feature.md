# FPS-F14 — The player rebinds keys from Settings

**Status:** Draft
**Labels:** feature, line:fps

## End state

A player can make Searchlight's controls their own: every action has a key or mouse button they chose, kept across launches, with no way to end up with a dead control. Hold and toggle choices for held actions (GDD §3.3) sit beside the binding. See the [GDD](../../../wiki/searchlight/game-design-document.md) §3.3 and the [roadmap](../../../wiki/searchlight/roadmap.md) FPS-M1.1.

## Summary

Settings gets a Controls page listing every player action with its current key or mouse button. The player picks an action, presses the new input, and Apply makes it live and saves it. A key already used by another action swaps, so nothing is left unbound. One button resets the page to defaults. This is the FPS card the engine's keybind capture (ENG-F18) has waited on, and it is cheapest now: the action list is small and every later action (rig hotkeys, stance, interact) joins a list that already rebinds.

**Moves toward the end state by:** giving FPS-F4 slice 2 (crouch, prone) and FPS-M2 (rig hotkeys) a binding list to join, and the hold/toggle checkbox a row to sit on.

## Exit criteria

```gherkin
Scenario: A rebound key does the action
  Given the player rebinds Reload to T in Settings and chooses Apply
  When the player presses T in the arena
  Then the player reloads
  And pressing the old key does nothing

Scenario: A rebinding survives a relaunch
  Given the player applied a rebinding
  When the player relaunches Searchlight
  Then Settings shows the new key and the game uses it

Scenario: A key in use swaps
  Given Reload is on R and Lean left is on Q
  When the player binds Reload to Q
  Then Reload is on Q and Lean left is on R
  And Settings says the two swapped

Scenario: Back discards
  Given the player captured a new key and did not Apply
  When the player goes back
  Then the old key still works and Settings shows it

Scenario: Reset restores the defaults
  Given the player rebound several actions
  When the player resets Controls and chooses Apply
  Then every action is on its default input

Scenario: Menu keys cannot be taken
  Given the player is capturing a key
  When the player presses Escape
  Then capture is cancelled and the binding is unchanged
```

- Verification (manual): rebind Fire to the right mouse button and Move forward to an arrow key, apply, play a wave; both work, and the left button no longer fires.

## Scope

- In: one Controls page in the FPS-F3 settings screen; every current player action listed; capture of one key or mouse button per action; staged until Apply (as FPS-F3-03); swap on conflict; reset to defaults; save and reload; Escape cancels capture; the console key and Escape stay unbindable.
- Out: see Deferred.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does it sit? | Own feature in FPS-M1.1, built after the engine's settings controls (#696) and before FPS-M2 adds rig hotkeys | Same screen and staging as FPS-F3; every action added later is then listed from day one. Waiting until FPS-M2 means retrofitting a longer list |
| What does a conflict do? | Swap: the displaced action takes the captured action's old input | No unbound state and no confirm dialog. A replace-and-unbind rule can strand Fire or Move; a refuse rule makes rearranging a layout a chore |
| Can an action be unbound? | No | A hardcore game with a dead Reload is a bug report, not a choice. Revisit with a request |
| Bindings per action | One editable input per action in the list; other inputs on that action (a hold, a pad button) are kept untouched | Smallest list that works; ENG-F26 already keeps what the page does not edit |
| Hold, toggle and chord capture | Not in this feature; the page must leave room for a hold/toggle checkbox beside a row | Check-mag's hold on R and chords are the only consumers today. Hold/toggle has a consumer when crouch and sprint arrive (FPS-F4 slice 2) |
| Reset scope | One button for the whole page | Per-action reset is polish; one button covers "I broke my layout" |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Hold/toggle checkbox beside held actions (GDD §3.3) | No toggleable action ships yet (lean is hold; crouch and sprint come later) | FPS-F4 slice 2; own card under FPS-F4 or a follow-up here |
| Capture of holds and chords (Hold R, Ctrl+R) | Needs a visible hold/chord UI; ENG-F26 lists it as owed to the keybind card, no player asks yet | Check-mag or unjam needs a rebind players ask for |
| A second binding per action | Doubles the list for no tested need | Playtest complaint |
| Clear (unbind) | See decisions | Request |
| Per-action reset | One-button reset covers the need | Request |
| Gamepad bindings | No gamepad support in the FPS | A gamepad feature |
| Key hints in prompts and tutorials showing the current key | No prompts exist yet | The first on-screen prompt (FPS-F9 interact or FPS-M2 inventory) |
| Console command to bind a key | Settings is the player path | Tester ask |

## Items

Not decomposed until the feature is approved. Proposed slices, each demoable:

| Slice | Observable |
|---|---|
| 1 Rebind one action and keep it | Controls page lists actions; capture, Apply, works in play, survives relaunch (skeleton: one real action end to end) |
| 2 Swap on conflict | Binding a used key swaps and says so |
| 3 Reset Controls to defaults | One button restores the defaults |

Slices 2 and 3 are independent of each other after slice 1.

## Notes

TL: open questions:
- How the page meets ENG-F26 #359: does the hold on R for Check magazine stay when Reload moves off R, and what does a tap on a hold-bound key do after Reload moves?
- Hold and toggle (lean, crouch): the page must carry a checkbox later without a rework. Does it need an engine row kind (#697 -17 or a new ENG-F18 keybind item), and who files it? Engine work is the Engine project's; file an engine request, do not design it here.
- Keys the page must refuse (Escape, the console key, F-keys the tester uses): who owns the list?
- Does Restart (F8, a host/tester action) appear on the page?
- Keyboard layouts: show the key by position or by label?
