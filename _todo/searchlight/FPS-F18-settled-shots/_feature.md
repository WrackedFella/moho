# FPS-F18 — Shots land tightest when the player stands still and fires slowly

**Status:** Draft
**Labels:** feature, line:fps

## End state
Accuracy is a product of the body and the gun (GDD §4.12 seam rule): spread is a weapon stat, shaped by posture, movement, wounds (§4.7 arms), wear (§4.3 barrel) and stats (steadiness, recoil control). This feature is the first source of spread on the player's weapon.

## Summary
Moving and rapid fire widen where the player's rounds go; standing still and letting the gun settle brings them back to the point of aim. Run-and-gun becomes a choice with a visible cost, and stand-and-shoot is rewarded, which is what the head bob (FPS-F4-05) only suggests.
**Moves toward the end state by:** giving crouch and prone (FPS-F4 slice 2), arm wounds (FPS-M4), barrel wear (FPS-M3) and the stats layer a spread to act on.

## Exit criteria
- A player standing still who fires at the paced rate puts every round where the sights point, as today.
- A player walking puts rounds in a visibly wider group; half speed is between the two.
- Rapid shots open the group shot by shot; after a short pause the next shot goes where the sights point again.
- A replay with the same seed and commands lands every round in the same place.
- Justin playtests: in the arena, stopping to shoot beats shooting on the move.

## Scope
- In: the player's rifle; spread from movement speed and from recent shots; recovery when settled; Simple (hitscan) and simulated rounds both.
- Out: enemies (they keep their grade-based aim, GDD §4.11); stance modifiers (slice 2 cards); per-weapon values beyond the one rifle (FPS-M2).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Does the head bob itself cost accuracy? | No: the bob stays view-only and a setting; the cost lives in spread (pending Justin) | A bob that moves the aim either can't be turned off (motion sickness, GDD §3.3 makes it a setting) or is an exploit at 0 %. |
| Does the crosshair open with spread (CS) or stay fixed? | Fixed; the player reads spread from impacts (pending Justin) | Limited HUD (§3.3). An opening crosshair is cheap to add later; removing it after players lean on it is not. |
| Where does a settled shot go? | Exactly where the sights point, as today | Keeps FPS-M1's passed feel; a base inaccuracy is a weapon stat for FPS-M2 or FPS-M3. |
| Milestone | FPS-M1.2 Stance and body, beside FPS-F4 | Same question ("does how I move change when I shoot?"); slice 2 extends it. FPS-M3's weapon stats would delay it past FPS-M2. |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Crouch and prone tighten spread | Stances don't exist yet | FPS-F4 slice 2 cards |
| Arm wounds widen spread; wounded legs reshape the bob | Wounds have no effects yet | FPS-M4 (GDD §4.7 edit now) |
| Barrel wear widens spread | Wear is Phase 3 | FPS-M3 |
| Steadiness and recoil control stats | §4.12: stats arrive with their first real source | First of FPS-M3 or FPS-M4 |
| Aim-down-sights tightening | Sights are FPS-M3.1; #806 zoom is a test tool | FPS-M3.1 |
| Aim wander that follows the footsteps (a sim-side figure-8) | Spread already carries the cost; this is steadiness | Steadiness stat |
| Enemies under the same rule | Grades already set enemy aim | Enemy debuffs (FPS-M4 to taste) |
| Opening crosshair | See decisions | After the playtest |

## Items
| Item |
|---|
| FPS-F18-01 Moving scatters the player's shots; standing still brings them back (walking skeleton) |
| FPS-F18-02 Rapid shots open the group; a pause resets it |
| FPS-F18-03 Each shot kicks the sights up, and they settle back to the point of aim (first to cut) |

## Notes
- 01 goes first: it brings spread into being. 02 and 03 are independent of each other.
- TL: spread perturbs the launch direction, so it must draw from the sim's seeded stream to keep replays exact (enemy spread already does).
- TL: the values are arena tuning on the one rifle until FPS-M2 moves weapon stats into item data.
