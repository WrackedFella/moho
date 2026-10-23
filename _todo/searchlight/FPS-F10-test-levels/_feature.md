# FPS-F10 — The tester loads a proving ground and calls up what they want to test

**Status:** Draft
**Labels:** feature, line:fps

## End state
Every system the line adds (weapons, stance, enemy grades, loose objects, later wear and
wounds) has a baked level built to probe it in minutes: known distances, known cover
heights, slopes and stairs, and controls in the world that spawn what the test needs.
Levels load by name; the hub and zones of FPS-M6 load the same way
([GDD](../../../wiki/searchlight/game-design-document.md) §5, §7).

## Summary
Ships a second baked level, the proving ground, and a sandbox mode with no waves and no
score. The tester launches it by name, walks a range with targets at marked distances,
stands behind cover at each height the stance work tests, and uses controls in the level
to spawn one enemy of a chosen grade or reset the targets. The arena stays the default
and keeps its waves. Today every test is a full wave run in one arena.
**Moves toward the end state by:** giving FPS-M2's three weapons, FPS-F4's stances,
FPS-F5's grades and FPS-F9's loose objects a place to be checked, and starting the range
GDD §7 plans for Phase 3.

## Exit criteria
- The tester launches the arena or the proving ground by name; an unknown name lists the
  levels that exist.
- In the proving ground no wave starts and no score shows.
- A control in the proving ground spawns one enemy of the grade it names; another resets
  every target.
- Targets stand at marked distances (25, 50, 100, 200 m), cover at 0.6, 1.0 and 1.4 m,
  and the level has a slope, stairs and a drop area on a ledge.

## Scope
- In: choosing a level by name at launch; a level states its mode (arena or sandbox); the
  proving ground and its markers; spawn and reset controls built on FPS-F9-02.
- Out: switching level while running; choosing a level from the main menu; levels for
  later phases; noclip, teleport, time scale, freezing AI, damage numbers.

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Generated or baked? | Baked, like the arena (GDD §5) | Generation is FPS-M7 territory and tests need fixed distances |
| Where does a level's mode come from? | The level file names it: arena (waves, score) or sandbox | Picking mode and level separately doubles the combinations to test |
| Test controls in the world or in the console? | In the world, as FPS-F9-02 controls; the existing console grade force (#585) stays | No new console surface, and the proving ground exercises FPS-F9. Cost: a control must be placed per level, so arena-wide commands still go to the console |
| Switching level at runtime | Waits; relaunch to change level | Unloading a level is engine need §10 item 10 (FPS-M6) |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Console `map <name>` while running | Engine level switching (§10 item 10) | The engine lane schedules it, or restart can reload a level cheaply (TL) |
| Level choice in the main menu | Testers launch from the command line | A player-facing level exists (FPS-M6) |
| Hit readout on targets (part, damage, range) | Hit-volume outlines (#611) and the HUD cover FPS-M2 | FPS-M3 weapon wear range |
| Noclip, teleport, time scale, AI freeze | No test needs them yet | A card's verification asks for one |
| Wound range, safe room, hub levels | Later phases (GDD §7) | FPS-M3, FPS-M5, FPS-M6 |

## Items
| Item | Behavior | Depends on |
|---|---|---|
| FPS-F10-01 | The tester launches the proving ground by name and nothing attacks until asked (the level, its markers, the sandbox mode, the name list) | none |
| FPS-F10-02 | Controls in the proving ground spawn an enemy of a chosen grade and reset the targets | FPS-F10-01; FPS-F9-02; FPS-F5-01 #585 |

## Notes
- Milestone FPS-M2; FPS-F10-01 can start at any time.
- Debug tools that stay where they are: the hit-volume outlines (FPS-F4-04 #611), the grade
  force (FPS-F5-01 #585), god mode and fullscreen in the console.
- TL: the launch already takes a level file path; naming levels may only need a list.
- TL: the proving ground's distances need a sightline long enough for 200 m; check the
  far plane and shadow cascades.
