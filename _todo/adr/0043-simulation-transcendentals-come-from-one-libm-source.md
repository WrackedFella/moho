# 0043 — Simulation transcendentals come from one libm source

**Status:** Accepted on merge of #1000

## Context

The sim must stay replayable and network-ready ([0011](0011-simulation-stays-network-ready.md)).
Rust's basic float operations give the same bits on every target, but `f32::sin`, `cos`,
`atan2`, `exp`, `powf` and the like call the platform math library, which differs between
OSes and even between Windows versions. R.2 determinism research and feature
[ENG-F45](https://github.com/WrackedFella/moho/issues/714) found these calls in engine
movement and in both game lines' sim code. glam reached `libm` only as a side effect of
rapier's `enhanced-determinism`.

The netcode ADR (ENG-M5+) will choose lockstep or snapshots. Either choice needs sim maths
that doesn't depend on the OS, and lockstep needs it everywhere. A rule that each lane follows
only when it remembers will erode, so it needs a gate. A clippy `disallowed-methods` lint can't
exempt test code without allow attributes, which `CLAUDE.md` forbids.

## Decision

1. Simulation code (code that writes simulation state) takes transcendentals from
   `moho_core::sim_math`, thin wrappers over the `libm` crate. A function the sim needs and
   `sim_math` lacks is added there, never called from the platform. `sqrt`, `abs`, `powi`,
   `to_radians` and basic arithmetic are exact or correctly rounded and stay unrestricted.
2. glam is built with its `libm` feature workspace-wide, so glam's own transcendentals (for
   example `Quat::from_rotation_y`) use the same source.
3. `just check` enforces rule 1 for the files listed in `scripts/sim-math.txt`, through
   `scripts/check-sim-math.sh`. The list is the boundary: a listed file fails the gate on any
   platform transcendental outside test code. Presentation code (camera, rendering, audio, UI)
   is never listed.
4. Each lane migrates its own call sites. A line adds a sim file to the list when that file is
   migrated, and asks for a missing wrapper through an `engine-request`. The sites still to
   migrate are listed in each line's wiki README.

## Consequences

- Same-binary replays no longer depend on the OS math library in the listed files. This is
  necessary for cross-OS (L1) determinism but not sufficient: glam's SIMD paths (`Quat`
  products, `Vec4`/`Vec3A` dot and normalize) remain an open gap for the netcode ADR.
- Transcendentals run at `libm`'s scalar speed, and glam values may move by about an ulp
  compared with `std`.
- The gate is textual. It misses calls through a trait or an alias, and it only sees files
  someone listed. Reviewers hold the rest of the rule.
- Unlisted sim files are known gaps until migrated: today `moho_physics/src/world/loose.rs`
  (`brake_rolling`, which needs a `tan` wrapper) and the game-line sites.
- How-to and current state live in `wiki/architecture/determinism.md`, which links this ADR.
  The netcode ADR builds on it rather than restating it.
