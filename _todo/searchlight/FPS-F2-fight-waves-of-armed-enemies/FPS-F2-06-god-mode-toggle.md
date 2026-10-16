# The tester types `tgm` in the console and enemies can no longer hurt the player

**Feature:** FPS-F2
**Status:** Draft
**Gate class:** unset
**Labels:** line:searchlight

**Note:** needs a console in the FPS: engine request #402 (text for it: #315).

## Summary
A tester types `tgm` (Toggle God Mode) in the developer console to make the player
unkillable, so waves, cover and chase (FPS-F2-03, FPS-F2-04) can be playtested without
restarting after every death. Hits still land and show, so damage feedback stays
testable. The setting is saved and is still set at the next launch.

## Deliverables
- Console command `tgm` flips god mode and prints the value it set.
- `tgm 1` sets it on and `tgm 0` sets it off, even if already at that value.
- While on, the player takes no main-HP loss, cannot die, and no body part is disabled.
- Hits on the player still register and show their usual feedback.
- The setting survives a restart and a relaunch.
- The screen shows when god mode is on.

## Acceptance criteria
```gherkin
Scenario: God mode is off on first launch
  Given the game has never saved a god mode setting
  When the game starts
  Then god mode is off

Scenario: tgm flips the value and prints it
  Given god mode is <before>
  When the tester enters "tgm"
  Then god mode is <after>
  And the console prints "God mode: <after>"

  Examples:
    | before | after |
    | off    | on    |
    | on     | off   |

Scenario Outline: tgm with an argument sets the value
  Given god mode is <before>
  When the tester enters "<command>"
  Then god mode is <after>
  And the console prints "God mode: <after>"

  Examples:
    | before | command | after |
    | off    | tgm 1   | on    |
    | on     | tgm 1   | on    |
    | on     | tgm 0   | off   |
    | off    | tgm 0   | off   |

Scenario Outline: tgm refuses an unusable argument
  Given god mode is on
  When the tester enters "<command>"
  Then god mode is still on
  And the console prints how to use tgm

  Examples:
    | command |
    | tgm 2   |
    | tgm on  |
    | tgm 1 0 |

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
  When the tester enters "tgm 0"
  Then the player still has 70 health

Scenario: God mode survives a restart
  Given god mode is on
  When the tester restarts the game
  Then god mode is still on

Scenario: God mode survives a relaunch
  Given god mode is on
  When the tester quits and starts the game again
  Then god mode is on

Scenario: The screen shows when god mode is on
  Given god mode is off
  When the tester enters "tgm 1"
  Then the screen shows that god mode is on
  When the tester enters "tgm 0"
  Then the screen no longer shows it
```

## Verification
- Fight a full wave with god mode on: nothing the enemies do ends the run, and hit feedback still reads clearly.
- The on-screen indicator is visible but does not obscure play.

## Notes
- Defaults picked (override on review): toggling leaves current health as it is; "restart" means the Enter restart after death.
- Open for Justin: is `tgm` limited to debug builds? Default: all builds, since playtests run release.
- Open for Justin: a persisted god mode left on is easy to forget. The on-screen indicator guards that; it can drop to the console reply alone if text on screen slips.
- Cut: infinite ammo, noclip, enemies ignoring the player, spawn commands. Each would be its own console command; revisit if a playtest needs one.
- TL: the console, its on-screen text and the saved setting are engine requests (#402, #315); this card is the first consumer and cannot start before #402 lands.
