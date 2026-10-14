# The game hides the world above a chosen height

**Feature:** ENG-F24
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary
A game sets a view height, live, and the renderer draws nothing in the world above it, so work below the surface shows from the strategy camera. Engine request #306; consumer SG-F4 (#310), "Lowering the view height reveals underground work".

## Deliverables
- A game sets, changes and clears a view height; the next frame honours it.
- The cut applies to world meshes, markers, shadows and ambient occlusion alike.
- The strategy game can set it (wiring a control to it is SG-F4's card).

## Acceptance criteria
```gherkin
Scenario: Geometry above the view height is not drawn
  Given terrain that rises above height H
  When the game sets the view height to H
  Then no world geometry above H is drawn

Scenario: A marker below the cut shows from above
  Given a marker on a block below the surface
  When the game sets the view height just above that block
  Then the marker is visible from the strategy camera

Scenario: Clearing the view height restores the full world
  Given a view height is set
  When the game clears it
  Then the next frame draws the whole world

Scenario: Shadows come only from what is drawn
  Given a view height cuts through terrain between the sun and a lit floor
  When the next frame draws
  Then the floor is lit as if the hidden terrain were absent
```

## Verification
1. In the strategy game, lower the view height through a hill with a cave: the cave floor shows from above, lit, with no dark band from the hidden hill.
2. Move the height up and down quickly: no frame shows stale geometry and the frame rate holds.
3. With the view height cleared, the scene matches a build without this card.

## Notes
- Cut faces are open (no cap); capping is deferred on ENG-F24.
