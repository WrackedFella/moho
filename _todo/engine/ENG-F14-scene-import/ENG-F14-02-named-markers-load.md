# Named markers load with their transform and properties

**Feature:** ENG-F14 (#226)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

Nodes without a mesh load as named markers (spawn points, loot points, boundaries) with
their world transform and custom properties. The game decides what each one means.
Consumer: P.2's player and enemy spawns (GDD §5, §7 Phase 1).

## Deliverables

- Every node with no mesh loads as a marker: name, world position, rotation, and its
  custom properties as text key-value pairs.

## Acceptance criteria

```gherkin
Scenario: A marker loads with its name and world transform
  Given a level file with a node "spawn_player" at (3, 0, -2) turned 90° about the vertical axis
  When the game loads it
  Then a marker named "spawn_player" is returned
  And its position is (3, 0, -2)
  And it faces 90° about the vertical axis

Scenario: A marker under a parent gets the parent's transform
  Given a node "spawn_enemy" at (1, 0, 0) under a parent at (10, 0, 0)
  When the game loads it
  Then the marker "spawn_enemy" is at (11, 0, 0)

Scenario: Custom properties come with the marker
  Given a node "spawn_enemy" with the custom property "wave" = "2"
  When the game loads it
  Then the marker "spawn_enemy" has the property "wave" with value "2"

Scenario: Mesh nodes are not markers
  Given a level file with two meshes and one empty node
  When the game loads it
  Then exactly one marker is returned

Scenario: Two markers may share a name
  Given two nodes named "spawn_enemy"
  When the game loads it
  Then both markers are returned
```

## Notes

- Blender writes custom properties to glTF `extras` when "Include custom properties" is on.
