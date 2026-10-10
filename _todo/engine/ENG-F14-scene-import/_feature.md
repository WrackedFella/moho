# ENG-F14 — A game loads a static level from a file

**Status:** Draft (revision of #226, which was filed from an unapproved draft; on approval this text replaces the issue body)
**Labels:** feature, line:engine

## End state

Games load hand-authored levels (arenas, ranges, later whole zones) from files made in
standard tools, and decide for themselves what each named marker means
([FPS GDD v0.2](../../../wiki/fps/game-design-document.md) §5, §10 item 1).

## Summary

A glTF file loads as world meshes that render and collide, plus named markers with
their transforms and custom properties. Consumer: the FPS arena prototype (P.2), whose
map is a baked file with spawn markers (GDD §7 Phase 1). Strategy is a possible later
consumer, not a reason to build anything now.
**Moves toward the end state by:** letting P.2 start on a real map without writing its
own importer, and fixing the scene-import seam (ADR-0012, seam 8) before ENG-F5 freezes it.

## Exit criteria

- A glTF fixture with two meshes and one named marker loads both meshes, with their node
  transforms applied, and the marker with its transform and properties.
- A character placed above a loaded floor lands on it (the meshes collide).
- A missing or malformed file fails with an error naming the file.

## Scope

- In: static meshes (node transforms applied), every mesh colliding, the material name
  each mesh uses, named markers with transform and custom properties, load errors.
- Out: what a marker means (FPS line); character meshes and anything that moves;
  textures; unloading or switching levels; streaming; skinning and animation.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| File format | glTF 2.0 via `gltf` 1.4 (ENG-F13, accepted 2026-10-07) | Settled |
| Which meshes collide? | Every mesh in the file is also its collider | Collision-only or non-colliding nodes need an authoring convention; nothing in the arena needs one yet |
| What is a marker? | A node with no mesh: its name, transform and custom properties (glTF `extras`) | The game picks markers by name; the engine gives them no meaning |
| Materials | A mesh reports the name of its material; the game maps names to its own surfaces | The renderer has no textures; flat surfaces are enough for a greybox arena |
| Characters | Not imported. Phase 1 enemies draw as the renderer's existing box instances, one per body part | Moving imported meshes need a renderer transform path; deferring it keeps F14 static. GDD §10 item 3 allows rigid segmented bodies |
| One level or many | One level, loaded once; nothing may stop a later unload (the game keeps the mesh ids) | Level switching is GDD Phase 6 |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Moving rigid character meshes from a file (old exit criterion) | Phase 1 enemies can be boxes; needs a renderer path for moving meshes | FPS line asks for character art |
| Textures and UVs | Renderer has no texture path; greybox arena doesn't need it | FPS art pass, as its own feature |
| Collision-only and non-colliding nodes | No map needs them yet | A map needs invisible walls or simplified collision |
| Unload and switch levels | GDD Phase 6 | FPS Phase 6 (GDD §10 item 10) |
| Lights and cameras from the file | Engine has one sun | A map needs authored lights |

## Items

| Item |
|---|
| ENG-F14-01 A level's meshes load from a glTF file and collide |
| ENG-F14-02 Named markers load with their transform and properties |

## Notes

- Order: 01 then 02 (same importer). Independent of ENG-F15, ENG-F21 and ENG-F20.
