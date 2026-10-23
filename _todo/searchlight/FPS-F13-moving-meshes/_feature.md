# FPS-F13 — Enemies move without being uploaded again

**Status:** Draft
**Labels:** feature, line:fps

## End state
Everything in Searchlight that moves (enemies, later loose objects, models and effects) draws through
the engine's placements, so motion costs a transform per frame, not a mesh upload. Fifty moving enemies
in the arena stay inside the frame budget on the reference machine.

## Summary
Today every living enemy re-uploads seven box meshes every frame (a known follow-up since FPS-M1). The
engine's ENG-F38 lets a mesh be registered once and drawn wherever the game places it. This feature
moves enemies onto that path and proves the result at fifty enemies. It also closes ENG-F38's last
exit criterion, which needs a Searchlight consumer.
**Moves toward the end state by:** giving FPS-F4 slice 3, FPS-F9-04 and FPS-F12 a working placement
call site to copy.

## Exit criteria
- No enemy draws through a per-frame `upsert`; shape meshes are registered once at load.
- A struck gun and a flashing segment still show the struck material; dead and hidden enemies draw nothing.
- Fifty moving enemies plus the arena hold the frame budget at 1080p on Justin's GTX 1080.
- The arena looks the same as before: segment shapes, shading and shadows are unchanged.

```gherkin
Scenario: Fifty enemies stay within the frame budget
  Given the arena holds fifty living enemies, all moving
  When the player plays for one minute at 1080p
  Then the frame time stays within the frame budget
```

## Scope
- In: enemy segments through placements; the verification at fifty enemies.
- Out: dummies (static today; move them only if it falls out for free), player body boxes
  (FPS-F4-04, #611, may adopt the same call when it lands), general meshes under non-uniform scale
  (wait for ENG-F39's normal transform), any new engine API.

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| New feature or a card under FPS-F4 or FPS-F12? | New small feature | F4 exits on stance, F12 on art; this one exits on a perf number and closes an engine criterion. A card under either would hold their exit open on an unrelated check. |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Dummies on placements | Static, no cost today | A dummy moves (FPS-F10 test levels) |
| Player body boxes on placements | Card FPS-F4-04 is not built | FPS-F4-04 picks it up |

## Items
| Item |
|---|
| FPS-F13-01, enemies draw through placements |

## Notes
Blocked by ENG-F38-01, which is on `dev` (#666) as of this draft; the card is Ready-eligible once the
Tech Lead specs it.
