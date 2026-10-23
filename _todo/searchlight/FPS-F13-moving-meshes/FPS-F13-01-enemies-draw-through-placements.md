<!-- board: gate=glue; eligible=yes; priority=P2 -->
# Fifty moving enemies hold the frame budget

**Feature:** FPS-F13
**Status:** Draft
**Gate class:** glue
**Labels:** line:fps
**Note:** Blocked by ENG-F38-01 (`WorldMeshes::upsert_movable` and `place`); it is on `dev` (#666). Do not mark Ready before the Tech Lead step.

## Summary
Enemies stop re-uploading seven box meshes every frame. Each segment shape is registered once at load
and placed each frame. This resolves the known follow-up "enemy meshes re-uploaded per frame" and
closes ENG-F38's last exit criterion.

## Deliverables
- One unit box mesh (or one per segment shape) is registered once at load as a movable mesh.
- Each frame, each living enemy's segments are placed at `position + facing * offset`, rotated by
  `facing`, scaled to `half * 2`, with the enemy material, or the struck material for the flashing gun.
- Dead or hidden enemies are not placed; nothing calls remove for them.
- No `upsert` for enemies remains in the frame path.
- Dummies may use the same path; optional.
- Fifty moving enemies plus the arena hold the frame budget, checked by Justin at 1080p on a GTX 1080.

## Acceptance criteria
```gherkin
Scenario: Enemies look as before
  Given an arena with living enemies
  When the player looks at an enemy
  Then each segment has the same shape, position and facing as before
  And its surfaces are lit from the correct side

Scenario: A firing enemy's gun flashes
  Given a living enemy
  When the enemy fires
  Then its gun shows the struck colour for the flash duration
  And the other segments keep the enemy colour

Scenario: A killed enemy disappears
  Given a living enemy in view
  When the player kills it
  Then none of its segments draw

Scenario: An enemy that turns keeps its shape
  Given a living enemy
  When it turns in place
  Then its segments turn with it around its position
```

## Tech spec
_Tech Lead._

## Verification
- Fifty moving enemies plus the arena hold the frame budget (Justin, GTX 1080, 1080p).
- Boxes stay correctly shaded under non-uniform scale (axis-aligned faces keep exact normals).

## Notes
- TL: placements last one frame, so they must be made from the per-frame render hook (`sync_enemies`
  is called there today), not the fixed tick.
- TL: segment materials differ per placement, so the engine's per-instance material must be enough;
  if one mesh per shape is needed for gun versus body, say so in the spec.
- Engine gaps: none found. File any as an engine request, not a design here.
