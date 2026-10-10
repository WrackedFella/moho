# A ray returns the first surface it hits with the game's tag

**Feature:** ENG-F15 (#227)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

A game casts a ray from a point, in a direction, up to a maximum distance, and gets
the first world surface hit: point, normal, distance and the tag the game gave that
mesh. Consumers: P.2 hitscan against walls and AI line of sight.

## Deliverables

- A game tags world meshes when it registers them.
- A ray query returns the nearest hit (point, normal, distance, tag) or nothing.
- A ray can be told to ignore one character (proved in ENG-F15-03).
- Movement capsules never block a ray.

## Acceptance criteria

```gherkin
Scenario: A ray reports the nearest surface
  Given a wall tagged 7 at 5 m and a wall tagged 9 at 10 m along X
  When a ray is cast from the origin along X up to 20 m
  Then it hits tag 7
  And the hit distance is 5 m
  And the hit point is on the near face of the first wall
  And the hit normal points back toward the origin

Scenario: A ray stops at its maximum distance
  Given a wall at 10 m along X
  When a ray is cast from the origin along X up to 5 m
  Then nothing is hit

Scenario: A ray that hits nothing reports nothing
  Given an empty world
  When a ray is cast in any direction
  Then nothing is hit

Scenario: Movement capsules don't stop a ray
  Given a character standing between the origin and a wall
  When a ray is cast through the character at the wall
  Then the wall is hit
```

## Notes

- Ignoring the shooter is proved in ENG-F15-03, once characters have hit volumes.
