# FPS-F14 — The player rebinds keys from Settings

**Status:** Draft
**Labels:** feature, line:fps

## End state

A player can make Searchlight's controls their own: every action has a primary and a secondary input they chose, as a plain key or button, a hold, or a chord, kept across launches. Held actions are hold or toggle by a checkbox beside the binding (GDD §3.3). See the [GDD](../../../wiki/searchlight/game-design-document.md) §3.3 and the [roadmap](../../../wiki/searchlight/roadmap.md) FPS-M1.1.

## Summary

Settings gets a Controls page listing every player action with its primary and secondary input. The player picks a slot, performs the input (press a key or button, hold it, or press it with Ctrl, Shift or Alt), and Apply makes it live and saves it. An input already used by another action asks before taking it. A slot can be cleared. Held actions get a hold/toggle checkbox. Reset restores defaults for the page or for one action. This is the FPS card the engine's keybind capture (ENG-F18) has waited on, and it is cheapest now: the action list is small and every later action (rig hotkeys, stance, interact) joins a list that already rebinds.

**Moves toward the end state by:** giving FPS-F4 slice 2 (crouch, prone) and FPS-M2 (rig hotkeys) a binding list to join.

## Exit criteria

```gherkin
Scenario: A rebound key does the action
  Given the player rebinds Reload's primary to T in Settings and chooses Apply
  When the player presses T in the arena
  Then the player reloads
  And the old key does nothing

Scenario: A secondary input works alongside the primary
  Given Reload has primary R and the player sets its secondary to T
  When the player presses either key
  Then the player reloads

Scenario: A rebinding survives a relaunch
  Given the player applied a rebinding
  When the player relaunches Searchlight
  Then Settings shows the new input and the game uses it

Scenario: Taking an input in use asks first
  Given Lean left is on Q
  When the player captures Q for Reload
  Then a dialog names Lean left and asks the player to confirm
  And nothing changes until the player confirms

Scenario Outline: The conflict dialog resolves
  Given the dialog is asking about Q, held by Lean left
  When the player chooses <choice>
  Then <result>
  Examples:
    | choice  | result                                                   |
    | Replace | Reload has Q and Lean left shows that slot as Unbound    |
    | Cancel  | both actions keep their inputs                           |

Scenario: A press and a hold of one key do not conflict
  Given Reload is on R
  When the player captures Hold R for Check magazine
  Then no dialog appears

Scenario: A slot can be cleared
  Given Reload has primary R and secondary T
  When the player clears the secondary and chooses Apply
  Then T does nothing and R still reloads

Scenario: An unbound action is visible
  Given the player cleared every input of Move forward
  When the player views Controls
  Then Move forward shows as Unbound, marked so it is not missed

Scenario Outline: Capture records what the player did
  Given the player is capturing a slot
  When the player <input>
  Then the slot shows <binding>
  Examples:
    | input                           | binding  |
    | presses Y                       | Y        |
    | presses the right mouse button  | Mouse 2  |
    | holds Y past the hold threshold | Hold Y   |
    | holds Ctrl and presses Y        | Ctrl+Y   |

Scenario: A held action toggles
  Given Lean left is marked toggle in Controls and applied
  When the player presses its key once and then again
  Then the lean holds after the first press and ends after the second

Scenario: Back discards
  Given the player captured a new input and did not Apply
  When the player goes back
  Then the old input still works and Settings shows it

Scenario: Reset one action
  Given the player rebound Reload and Fire
  When the player resets Reload and chooses Apply
  Then Reload is on its defaults and Fire keeps the player's choice

Scenario: Reset the page
  Given the player rebound several actions and cleared one
  When the player resets Controls and chooses Apply
  Then every action is on its defaults

Scenario: Menu keys cannot be taken
  Given the player is capturing an input
  When the player presses Escape
  Then capture is cancelled and the slot is unchanged
```

- Verification (manual): rebind Fire to the right mouse button and Move forward to an arrow key, apply, play a wave; both work, and the left button no longer fires. Set Check magazine to Hold T and Ctrl+T on its two slots and exercise both.

## Scope

- In: one Controls page in the FPS-F3 settings screen; every current player action listed with primary and secondary slots; capture of a key, mouse button, hold or Ctrl/Shift/Alt chord; staged until Apply (as FPS-F3-03); confirm dialog on conflict; clear a slot; hold/toggle checkbox for held actions; reset per action and for the page; save and reload; Escape cancels capture; the console key and Escape stay unbindable.
- Out: see Deferred.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Where does it sit? | Own feature in FPS-M1.1, after the engine's settings controls (#696), before FPS-M2 adds rig hotkeys | Same screen and staging as FPS-F3; later actions are listed from day one |
| What counts as a conflict? | Same input, same press kind, same modifiers, on a different action | A tap on R and Hold R are different bindings (ENG-F26); Ctrl keeps its own action under Ctrl+R. Blocking those would forbid the default check-mag layout |
| What does a conflict do? | Dialog; Replace takes the input and the other action's slot becomes Unbound; Cancel changes nothing | Player's call (Justin, 2026-10-27). Cost: a player can strand an action, so Unbound must show plainly and Reset must fix it |
| Can a slot be cleared? | Yes, either slot | Player's call. An action may end up with no input; shown as Unbound |
| Bindings per action | Two editable slots: primary and secondary | Player's call. Existing defaults with two inputs (Check magazine: C and Hold R) fit without special cases |
| How does capture record a hold or chord? | By performing it: hold past the threshold records Hold, modifier held with a key records the chord | Proposed; TL and the Engine project settle the capture method. Alternatives: a press-kind picker per slot (more UI, no timing in capture) |
| Hold/toggle | A checkbox beside each held action's row; the choice persists | GDD §3.3; applies to actions the game marks toggleable (today lean) |
| Reset scope | A button per action and one for the page | Player's call |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Gamepad bindings | No gamepad support in the FPS | A gamepad feature |
| Key hints in prompts showing the current key | No on-screen prompts exist yet | The first prompt (FPS-F9 interact or FPS-M2 inventory) |

Dropped: console command to bind a key (not needed).

## Items

Not decomposed until the feature is approved. Proposed slices, each demoable; order is the build order.

| Slice | Observable | Needs from the engine |
|---|---|---|
| 1 Rebind one action and keep it | Controls page lists actions; capture a key or button into the primary slot, Apply, works in play, survives relaunch (the skeleton) | engine keybind capture row (engine request) |
| 2 Conflict dialog | Taking a used input asks, Replace or Cancel; shows Unbound | none beyond 1 |
| 3 Secondary slot | Each action takes a second input | none beyond 1 |
| 4 Clear and Reset | Clear a slot; reset per action and for the page | none beyond 1 |
| 5 Hold and chord capture | Hold Y, Ctrl+Y recorded and working | ENG-F26 #359 and capture of holds and chords |
| 6 Hold/toggle checkbox | Lean toggles by checkbox | ENG-F26 #359 toggle mode |

Slices 2, 3 and 4 are independent after 1. Slices 5 and 6 are the largest and the most engine-dependent; they go last, and are the cut line if the feature must shrink.

## Notes

TL: open questions:
- How the page meets ENG-F26 #359: is its hold/toggle and hold-timing already landed, and what remains for slices 5 and 6? A tap on a key that also has a hold binding fires on release; how does the list show that?
- Capture of holds and chords, the confirm dialog and the hold/toggle checkbox need engine row kinds (ADR-0026 keeps widget kinds closed). File engine requests stating what the player must be able to do; the Engine project designs them.
- Keys the page must refuse (Escape, the console key, tester F-keys): who owns the list?
- Does Restart (F8, a host/tester action) appear on the page?
- Keyboard layouts: show the key by position or by label?
- Is an unbound Fire or Move acceptable at the sim, or does the game refuse to Apply an action set with no way to move?
