# SG-F4 — Worker pawns gather resources on their own

**Status:** parked

## Summary

Pawns gain basic stats and a simple job loop so they can gather without
direct control. Depends on SG-F3 (a base to return to). Not decomposed into
cards until picked up.

## Scope sketch

- Simple AI loop: go to A, perform an action, return to B (mine here, carry
  the resource back to base, repeat). `Pawn::mine` already takes plain glam
  types so AI pawns can reuse it.
- Basic pawn stats (carry weight, speed).
- Later/nice-to-have: pawn model (first- and third-person), better movement
  (inertia, climbing).
