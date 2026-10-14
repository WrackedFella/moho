# ENG-F24 — The game marks places in the world and keeps them in view

**Status:** Draft
**Labels:** feature, line:engine

## End state
A game shows the player where things are and where work will happen: selected pawns, blocks marked for mining, a building's placement preview, and in the FPS an interaction highlight. Marks stay readable against the world and visible when the work is underground. Markers reach the renderer as data, never as game types ([ADR-0001](../../adr/0001-render-api-boundary.md), [ADR-0013](../../adr/0013-ui-reaches-the-renderer-as-paint-data.md)).

## Summary
A game adds, moves and removes markers at world positions, as a translucent fill or an outline in a colour it picks, and can hide the world above a chosen height so marks below the surface show from above. Engine requests #305 (markers) and #306 (height slice); consumers SG-F5 (#309) selection, SG-F4 (#310) mining marks and underground marks, SG-F3 (#312) placement preview.
**Moves toward the end state by:** giving every strategy order a visible result; SG-F3's valid or invalid preview is a colour change on the same marker.

## Exit criteria
```gherkin
Scenario: A marker appears where the game puts it
  Given the game adds a box marker at a world position with a colour
  When the next frame draws
  Then the marker shows at that position in that colour

Scenario: A marker follows and leaves when told
  Given a marker is shown
  When the game moves it, then removes it
  Then it shows at the new position, then no longer shows

Scenario Outline: A marker draws in either style
  Given the game adds a marker with style <style>
  When the next frame draws
  Then <what shows>
  Examples:
    | style            | what shows                                        |
    | translucent fill | the marked shape, with the surface behind it visible |
    | outline          | the edges of the marked shape only                |

Scenario: Lowering the view height hides the world above it
  Given the game sets a view height below the terrain surface
  When the next frame draws
  Then nothing above that height is drawn
  And a marker below that height is visible from above

Scenario: The view height changes live
  Given a view height is set
  When the game raises or lowers it
  Then the next frame draws the world cut at the new height
```
Verification (manual): a marker on a visible block face reads clearly, with no flicker against the face; at a cut, shadows and ambient occlusion show no light leaking from geometry that is hidden.

## Scope
- In: box markers of any size (a block, a thin slab on a face, a box around a pawn); translucent fill and outline; colour per marker; a live view height in the main, shadow and ambient-occlusion passes.
- Out: shapes other than boxes; markers drawn through walls; deciding what is marked (game line).

## Direction-setting decisions
| Question | Decision | Why / cost of the alternative |
|---|---|---|
| How does a game address a marker? | By handle: add, update, remove (as world meshes are, ADR-0010) | Games already hold mesh handles; rebuilding a list each frame is an alternative the TL may weigh |
| Are markers hidden behind terrain? | Yes, depth-tested | Show-through needs a second style and ordering rules; nothing asks yet |
| Height slice: renderer or game remesh? | Renderer, across all passes | A remesh per slider step is slow and the cut shading would differ; any top-down game or an editor reuses the renderer cut |
| Does the cut hide markers too? | Yes, everything in the world above the height | One rule; UI is unaffected |

## Deferred
| Idea | Why it waits | Revisit when |
|---|---|---|
| Capped cut (solid ground drawn as a flat top at the cut) | The open cut may read well enough; capping costs a pass or remesh | Playtest of SG-F4's underground marking finds it unreadable |
| X-ray view | The height slice covers SG-F4 (#306) | Strategy asks |
| Markers seen through walls | No consumer | A selected pawn hidden by terrain must stay findable |
| Spheres, rings, decals | Boxes cover blocks and pawns | A shape a box can't stand in for |
| Animated markers (pulse, fade) | Polish | Playtest asks |

## Items
| Item | Order |
|---|---|
| A game shows a translucent box marker where it puts it | first: the walking skeleton through render API and renderer |
| A marker can draw as an outline | after the first |
| [ENG-F24-03](ENG-F24-03-height-slice.md) The game hides the world above a chosen height (#306) | independent of the outline; needed by SG-F4's underground marking card |

## Notes
- First consumer: SG-F5's first selection card; SG-F4's underground marking card needs the height slice.
