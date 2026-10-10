# Hit volumes follow a character and rays report the part hit

**Feature:** ENG-F15 (#227)
**Status:** Draft
**Gate class:** unset
**Labels:** line:engine

## Summary

A game attaches tagged hit volumes (head, torso, limbs) to a character at offsets from
its position and facing. The volumes follow the character every tick, and a ray that
strikes one returns its tag. Consumer: P.2 per-body-part hits from day one (GDD §7
Phase 1, §4.7).

## Deliverables

- A game attaches box volumes to a character, each with an offset, size and tag, and
  removes them with the character.
- Volumes follow the character's position and facing each tick.
- Rays return the tag of the volume they hit; volumes never block movement.

## Acceptance criteria

```gherkin
Scenario: A ray at the head hits the head volume
  Given a character with a "head" volume above a "torso" volume
  When a ray is cast level at the head's height through the character
  Then the hit tag is "head"

Scenario: Volumes follow the character
  Given a character with a "head" volume
  And the character has moved 3 m along X since the volume was attached
  When a ray is cast at the head's new position
  Then the hit tag is "head"

Scenario: Volumes turn with the character
  Given a character with an "arm_right" volume 0.4 m to its right
  And the character has turned 180°
  When a ray is cast at the point 0.4 m to the character's original left
  Then the hit tag is "arm_right"

Scenario: A ray between volumes passes through
  Given a character whose volumes leave a gap beside the torso
  And a wall behind the character
  When a ray is cast through the gap
  Then the wall is hit

Scenario: The shooter's own volumes are ignored
  Given two characters facing each other
  When the first casts a ray from inside its own head volume at the second's torso, ignoring itself
  Then the hit tag is the second character's "torso"

Scenario: Volumes don't block movement
  Given two characters whose volumes overlap the space between them
  When one walks into that space
  Then it stops only where the movement capsules meet

Scenario: Removing a character removes its volumes
  Given a character with a "head" volume
  When the character is removed
  Then a ray through where its head was hits nothing
```

## Notes

- Tags are the game's; the engine attaches no meaning to "head" or "torso".
