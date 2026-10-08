# Terrain generation uses in-house Perlin noise

**Feature:** [ENG-F2](_feature.md)
**Issue:** [#105](https://github.com/WrackedFella/moho/issues/105)
**Status:** unknown
**Gate class:** unset
**Labels:** line:engine

## Summary

ENG-F13 call: replace `noise` (last release 2024-03; brings a duplicate
`rand` 0.8) with an in-house 2D Perlin function of about 60–90 lines
([audit](../ENG-F13-dependency-audit/audit.md)). Only `Perlin::new(seed)` and
2D sampling are used, by biome selection and terrain height in the strategy
game.

## Deliverables

- No workspace crate declares `noise`; `rand` 0.8 is gone from the lockfile.

## Acceptance criteria

```gherkin
Scenario: Noise is deterministic per seed
  Given two noise sources built from the same seed
  When both are sampled at the same points
  Then the values are identical

Scenario: Different seeds give different noise
  Given noise sources built from two different seeds
  When both are sampled at the same non-lattice points
  Then the values differ

Scenario: Noise is zero on integer lattice points
  Given a noise source
  When it is sampled at integer coordinates
  Then every value is zero

Scenario: Noise stays in range
  Given a noise source
  When it is sampled across a large grid of points
  Then every value lies within [-1, 1]

Scenario: Noise is continuous
  Given a noise source
  When it is sampled at two points a tiny distance apart
  Then the values differ by a tiny amount
```

- [ ] A golden-value test pins a handful of samples so later changes to terrain are deliberate.

## Verification

- A new world generates plausible terrain and biomes (shape will differ from
  the old generator for the same seed).

## Notes

- Changes terrain for a given seed. Acceptable before v1.0; chunks already
  saved still load, but seams with newly generated chunks may not line up in
  old worlds.
- Domain gate: terrain generation rules live in `moho_game`; tests are reviewed
  before implementation.
- Strategy-line code filed under ENG-F2 because the work is a dependency
  removal; land after [ENG-F10](../ENG-F10-world-geometry-from-any-source/_feature.md), whose
  "behaviour unchanged" check must not be mixed with a terrain change.
