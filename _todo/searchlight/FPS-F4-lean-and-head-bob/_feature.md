# FPS-F4 — The player leans to peek, and what sticks out of cover gets hit

**Status:** Draft
**Labels:** feature, line:fps

## End state
Searchlight's body is in the world: walk, sprint, crouch and prone, with lean as the peek in
every stance, and the player's hit volumes follow posture, so what sticks out of cover is what
gets hit ([GDD §3.3](../../../wiki/searchlight/game-design-document.md#33-micro-loop-moment-to-moment)).
Milestone FPS-M1.2, stance and body, delivers it in three slices that track the engine
([roadmap §5](../../../wiki/searchlight/roadmap.md#5-fps-m12-stance-and-body-fps-f4)).

## Summary
Slice 1 of FPS-M1.2. The player leans left or right from standing to look and shoot around a
corner; the lean exposes the head and the lean-side arm and keeps the rest behind cover, so
enemies can only hit what crosses the corner. Footsteps bob the view, and the player sets how
much. Needs nothing new from the engine.
**Moves toward the end state by:** answering "does exposing what sticks out change how I take a
corner?" before crouch and prone exist, and fixing the lean rules slice 2 extends to crouched
and prone stances.

## Exit criteria
- Behind tall arena cover, a standing player who isn't leaning takes no hits from enemies on
  the far side; leaning out lets those enemies see and hit the player.
- Hits on a leaning player land on the head, torso or lean-side arm, never the legs or the far
  arm.
- A wall beside the player stops the lean short; the view never passes through geometry.
- Head bob follows walking and sprinting, and a head bob of 0 % keeps the view still; the
  setting survives a relaunch.
- Justin playtests and approves the feel.

## Scope
- In: lean left and right while standing, with clearance; hit volumes and enemy sight follow
  the lean; movement rules while leaning; head bob scaled by speed; a head-bob setting from 0 to
  100 %, saved, set from the console now and from the Settings screen once FPS-F3-03 lands.
- Out: crouch and prone (slice 2); a visible body (slice 3); hold-or-toggle checkboxes; key
  rebinding.

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| One feature for all of FPS-M1.2, or one per slice? | One per slice; this is slice 1. Slices 2 and 3 become their own features when their engine items land. | Slices 2 and 3 wait on engine M3 and M5+; an umbrella feature could not close against its exit for months. |
| Lean input | Q and E, held. Fixed until ENG-F26 #359 brings rebinding and the hold-or-toggle checkbox. | GDD §3.3; roadmap §5 "until then lean is hold". |
| What moves in a lean | Eye and view shift sideways (about 0.35 m) and roll (about 12°); head and upper body follow; legs stay planted. | The peek must expose the head and lean-side arm and keep the legs covered, or lean has no cost. |
| Moving while leaning | Allowed at reduced speed; no sprint while a lean is held. | Lean is a decision, not free strafing; Tarkov and Road to Vostok precedent. |
| Both lean keys held | The most recently pressed side wins. | Cheapest predictable rule. |
| Lean is sim state | Lean is part of the per-tick command and character state (ADR-0011). | A replay reproduces it; adding it later means retrofitting the command. |
| Head bob is a preference, not sim state | Saved setting, default moderate; camera only, never moves hit volumes or the aim ray. | Motion-sensitive players set 0 %; bob that moved the aim would make the setting a gameplay advantage. |
| Lean clearance | A wall stops the lean short. Workaround from #544: the game checks clearance itself until the engine answers. | The engine request leaves lean's home open; the design must not preclude moving it into the engine. |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Lean while crouched and prone (roll onto a shoulder) | No crouch or prone until ENG-F33 #492 and #544 | Slice 2 feature |
| Crouch, prone, timed stance changes | Engine hulls per mode | Slice 2 feature (ENG-M3) |
| Visible first-person body and shadow; enemies on the same rig | Skinned characters, #543 | Slice 3 feature |
| Hold-or-toggle checkbox per held action | Needs tap/hold key actions, ENG-F26 #359 | Key rebinding |
| Weapon sway and ADS bob | Belong to the weapon model and stats | FPS-M2, §4.12 |
| Lean speed or exposure from stats | No stat layer yet | §4.12 stance source |
| Enemies leaning | Enemy AI slices | Enemy AI slice 2 |
| Blind fire, side-step, variable lean amount | GDD cut lines | Not planned |

## Items
| Item |
|---|
| FPS-F4-01 The player leans around a corner to look and shoot |
| FPS-F4-02 Enemies see and hit what a lean exposes |
| FPS-F4-03 Footsteps bob the view, and the player sets how much |

## Notes
- FPS-F4-02 builds on -01. FPS-F4-03 is independent of both.
- FPS-F4-03 adds a Head bob slider row to FPS-F3-03 #460's settings table (scope edit to #460
  on approval); until #460 lands the setting is reached from the console.
- TL: can the sim's existing ray queries carry the lean clearance check, or does thin geometry
  need a shape sweep (then it joins #544)?
- TL: does the camera path take a per-tick eye offset and roll for lean and bob without a
  renderer change?
- TL: enemy sight today: does it test the player's hit volumes or one eye point? FPS-F4-02
  needs it to see a leaning head past a corner.
