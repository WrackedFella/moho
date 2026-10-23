# FPS-F4 — Posture decides what sticks out of cover, and what sticks out gets hit

**Status:** Draft
**Labels:** feature, line:fps

## End state
Searchlight's body is in the world: walk, sprint, crouch and prone, with lean as the peek in
every stance, and the player's hit volumes follow posture, so what sticks out of cover is what
gets hit ([GDD §3.3](../../../wiki/searchlight/game-design-document.md#33-micro-loop-moment-to-moment)).
This feature is all of milestone FPS-M1.2, stance and body, built in three slices alongside
FPS-M2 ([roadmap §5](../../../wiki/searchlight/roadmap.md#5-fps-m12-stance-and-body-fps-f4)).

## Summary
The player leans to peek in every stance, crouches and goes prone with timed transitions, and
has a visible first-person body; the hit volumes follow all of it, so enemies hit only what
crosses the cover. Footsteps bob the view, and the player sets how much.
**Moves toward the end state by:** giving limb wounds (FPS-M4) and stance modifiers (§4.12) a
body to act on.

| Slice | Content | Engine items it needs |
|---|---|---|
| 1 Lean, placeholder body, head bob | Lean while standing with clearance; hit volumes and enemy sight follow it; a placeholder segmented body that follows the lean and casts a shadow; a debug view of the hit volumes; head bob and its setting | none |
| 2 Crouch and prone | Both stances with timed transitions and speed rules; prone's oriented hull; lean extends to crouched and prone; hit volumes and enemy aim follow the stance | ENG-F33 #492, #544 |
| 3 Rigged body | Rigged first-person body and shadow replace the placeholder; enemies on the same rig; hit volumes ride the bones | #543 |

## Exit criteria
- Behind tall arena cover, a standing player who isn't leaning takes no hits from enemies on
  the far side; leaning out lets those enemies see and hit the player, on the head, torso or
  lean-side arm, never the legs or the far arm.
- In the arena, a player crouched behind the 1.4 m crates is hit only when leaning, on head or
  arm; a prone player behind the same crate is not seen at all.
- A wall beside the player stops a lean short, and a low ceiling refuses standing up; the view
  never passes through geometry.
- The player casts a shadow that leans with the player (slice 1).
- Looking down shows the player's rigged legs (slice 3).
- A head bob of 0 % keeps the view still; the setting survives a relaunch.
- Justin playtests and approves each slice's feel.

## Scope
- In: the three slices above; movement rules while leaning, crouched and prone; a head-bob
  setting from 0 to 100 %, saved, set from the console now and from the Settings screen once
  FPS-F3-03 #460 lands.
- Out: hold-or-toggle checkboxes and key rebinding (ENG-F26 #359); the cut lines in roadmap §5
  (weapon collision against the ground, prone dive, enemies going prone, variable crouch height,
  side-step, blind fire, mantle, slide, vault).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| One feature for all of FPS-M1.2, or one per slice? | One umbrella feature; all three slices land alongside FPS-M2 (Justin, 2026-10-23). Slices 2 and 3 are carded when their engine items are scheduled. | On today's engine roadmap ENG-F33 is ENG-M3 and skeletal animation ENG-M5+; landing in FPS-M2 needs the engine lane to pull #492, #544 and #543 forward. |
| Lean input | Q and E, held. Fixed until ENG-F26 #359 brings rebinding and the hold-or-toggle checkbox. | GDD §3.3; roadmap §5 "until then lean is hold". |
| What moves in a lean | Eye and view shift sideways (about 0.35 m) and roll (about 12°); head and upper body follow; legs stay planted. | The peek must expose the head and lean-side arm and keep the legs covered, or lean has no cost. |
| Moving while leaning | Allowed, slower than walking; sprint (FPS-F7) is refused while a lean is held. | Lean is a decision, not free strafing; Tarkov and Road to Vostok precedent. |
| Both lean keys held | The most recently pressed side wins. | Cheapest predictable rule. |
| Lean is sim state | Lean is part of the per-tick command and character state (ADR-0011). | A replay reproduces it; adding it later means retrofitting the command. |
| Head bob is a preference, not sim state | Saved setting, default moderate; camera only, never moves hit volumes or the aim ray. | Motion-sensitive players set 0 %; bob that moved the aim would make the setting a gameplay advantage. |
| Lean clearance | A wall stops the lean short. Workaround from #544: the game checks clearance itself until the engine answers. | The engine request leaves lean's home open; the design must not preclude moving it into the engine. |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
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
| FPS-F4-04 The player sees their own body's shadow, and can show the hit volumes |
| Slice 2 cards: carded when ENG-F33 #492 and #544 are scheduled |
| Slice 3 cards: carded when #543 is scheduled |

## Notes
- FPS-F4-02 builds on -01. FPS-F4-03 is independent of both. FPS-F4-04's boxes are independent; its lean scenario needs -01. Slice 2 extends -01, -02 and -04; slice 3 replaces -04's boxes.
- Reload animation is FPS-F8 (hands and reload), after slice 3, not this feature.
- FPS-F4-03 adds a Head bob slider row to FPS-F3-03 #460's settings table (scope edit to #460
  on approval); until #460 lands the setting is reached from the console.
- TL: can the sim's existing ray queries carry the lean clearance check, or does thin geometry
  need a shape sweep (then it joins #544)?
- TL: does the camera path take a per-tick eye offset and roll for lean and bob without a
  renderer change?
- TL: enemy sight today: does it test the player's hit volumes or one eye point? FPS-F4-02
  needs it to see a leaning head past a corner.
