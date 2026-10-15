# The tester toggles god mode and enemies can no longer hurt the player

**Feature:** FPS-F2
**Status:** Draft
**Gate class:** unset
**Labels:** line:searchlight

## Summary
A tester presses a dedicated key to make the player unkillable, so waves, cover and
chase (FPS-F2-03, FPS-F2-04) can be playtested without restarting after every death.
Hits still land and show, so damage feedback stays testable.

## Deliverables
- A key toggles god mode on and off during play.
- While on, the player takes no main-HP loss, cannot die, and no body part is disabled.
- Hits on the player still register and show their usual feedback.
- The screen shows when god mode is on.
- God mode is off at every launch.

## Acceptance criteria
```gherkin
Scenario: God mode is off at launch
  Given the game has just started
  Then god mode is off
  And the player takes damage from enemy hits as usual

Scenario: The toggle key switches god mode on and off
  Given god mode is off
  When the tester presses the god mode key
  Then god mode is on
  When the tester presses the god mode key again
  Then god mode is off

Scenario: The player takes no health loss from hits
  Given god mode is on
  And the player has 100 health
  When an enemy hits the player in the torso
  Then the player still has 100 health

Scenario: A head hit does not kill the player
  Given god mode is on
  When enemies hit the player in the head until a head hit would be fatal
  Then the player is alive

Scenario: A limb hit does not disable the limb
  Given god mode is on
  When enemies hit the player's arm until a limb hit would disable it
  Then the arm still works

Scenario: Hits still show
  Given god mode is on
  When an enemy hits the player
  Then the player sees the same hit feedback as with god mode off

Scenario: Turning it off keeps current health
  Given god mode is on
  And the player has 70 health
  When the tester turns god mode off
  Then the player still has 70 health

Scenario: God mode survives a restart
  Given god mode is on
  When the tester restarts the game
  Then god mode is still on

Scenario: The screen shows when god mode is on
  Given god mode is off
  When the tester turns god mode on
  Then the screen shows that god mode is on
  When the tester turns god mode off
  Then the screen no longer shows it
```

## Verification
- Fight a full wave with god mode on: nothing the enemies do ends the run, and hit feedback still reads clearly.
- The on-screen indicator is visible but does not obscure play.

## Notes
- Defaults picked (override on review): a key, not a typed console; any health taken before turning it on stays; restart keeps it on.
- Open for the player: is it limited to debug builds, or present in release too? Default: present in all builds, since a tester runs release.
- Cut: infinite ammo, noclip, enemy-ignores-player, spawn controls. Revisit if a playtest needs one.
- TL: the toggle must not become a shipped gameplay binding the player can trip by accident; the indicator needs on-screen text (engine request #315).
- TL: a typed command console is a generic engine need; if wanted, file it as an engine request, not here.
