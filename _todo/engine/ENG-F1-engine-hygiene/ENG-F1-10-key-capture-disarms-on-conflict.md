# Key capture stops listening once a conflict is raised

**Feature:** [ENG-F1](_feature.md)
**Status:** Draft
**Gate class:** glue
**Labels:** line:engine

**Note:** Low priority, opportunistic: pick up when next touching keybind capture.

## Summary

In the keybind settings, pressing a lone modifier key that another action already holds
raises the "key already bound" prompt but leaves the row listening. A second key pressed
while the prompt is open is bound immediately, and confirming the prompt then overwrites
it. Ordinary keys already stop listening when they raise the prompt; modifier keys
should behave the same.

Found by the #181 thread (ENG-F22-14, PR #194); unchanged by #149 (ENG-F12-02, bindings
are data).

## Deliverables

- A conflict raised by any captured key, modifier or not, ends listening on that row.
- While the conflict prompt is open, no key press changes any binding.
- Confirm applies only the key the prompt names; Cancel changes nothing.

## Acceptance criteria

```gherkin
Background:
  Given the settings menu is on the controls tab
  And Sprint is bound to Shift
  And Move Forward is bound to W
  And Move Backward is bound to S

Scenario Outline: A conflicting key ends listening
  Given the player is rebinding Move Forward
  When the player presses <key>, which <holder> holds
  Then the conflict prompt names <key> and <holder>
  And Move Forward is no longer listening for a key

  Examples:
    | key   | holder        |
    | Shift | Sprint        |
    | S     | Move Backward |

Scenario: Keys pressed while the prompt is open bind nothing
  Given the player is rebinding Move Forward
  And the player pressed Shift and the conflict prompt is open
  When the player presses Alt
  Then Move Forward is still bound to W
  And no action is bound to Alt

Scenario: Confirm applies only the prompted key
  Given the player is rebinding Move Forward
  And the player pressed Shift, then Alt, while the conflict prompt is open
  When the player confirms the prompt
  Then Move Forward is bound to Shift
  And Sprint no longer holds Shift
  And no action is bound to Alt

Scenario: Cancel leaves every binding as it was
  Given the player is rebinding Move Forward
  And the player pressed Shift, then Alt, while the conflict prompt is open
  When the player cancels the prompt
  Then Move Forward is bound to W
  And Sprint is bound to Shift
  And no row is listening for a key
```

## Tech spec

Pending (Tech Lead).

## Notes

- Fold the new tests into the tiers [ENG-F1-04](ENG-F1-04-keybind-test-layering.md)
  keeps, rather than adding a third.
