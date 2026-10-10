# A level's meshes load from a glTF file and collide

**Feature:** ENG-F14 (#226)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

A game loads a glTF level file and gets world meshes it can hand to the renderer and
physics through the existing world-geometry contract (ADR-0010). Consumer: the FPS
arena prototype (P.2).

## Deliverables

- A glTF 2.0 file (`.gltf` or `.glb`) loads into world meshes, one per mesh primitive,
  with each node's full transform (parents included) applied.
- Each loaded mesh reports the name of its material.
- Loaded meshes collide once registered with physics.
- Load failures return an error naming the file.

## Acceptance criteria

```gherkin
Scenario: Every mesh in the file loads
  Given a level file with two meshes
  When the game loads it
  Then two world meshes are returned

Scenario: Node transforms are applied, parents included
  Given a level file with a unit cube under a parent node moved 10 m along X
  When the game loads it
  Then the loaded cube's vertices lie between 9.5 m and 10.5 m along X

Scenario: A mesh reports its material's name
  Given a level file whose floor uses the material "concrete"
  When the game loads it
  Then the floor mesh reports the material name "concrete"

Scenario: A character lands on a loaded floor
  Given a level file with a floor at height 0
  And its meshes are registered with physics
  When a character dropped from 2 m above the floor is stepped for 2 seconds
  Then the character stands on the floor

Scenario Outline: A bad file fails with an error naming it
  Given <file>
  When the game loads it
  Then loading fails
  And the error names the file

  Examples:
    | file                                   |
    | a path with no file                    |
    | a file that is not glTF                |
    | a .gltf whose buffer file is missing   |
```

## Verification

- Load a greybox fixture into a running game: meshes render lit by the sun with shadows,
  at the positions they have in Blender.

## Notes

- Fixtures are exported from Blender (ENG-F13 audit: a strided accessor and a nested
  transform among them).
