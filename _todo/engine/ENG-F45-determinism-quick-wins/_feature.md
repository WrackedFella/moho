# ENG-F45 — The replay is measured across operating systems, and engine sim math stops depending on the OS

**Status:** Draft
**Labels:** feature, line:engine

## End state

Co-op on both lines (ADR-0011) needs a simulation whose determinism level is known and protected: L0 (same binary,
same machine) for snapshot prediction, L1 or L3 (same build on any machine, or any OS and ISA) if strategy co-op picks
lockstep. The netcode ADR (ENG-M5+) picks the model. Until then the engine keeps both open and learns what each costs.

## Summary

Today the #502 replay proves L0 only, and nothing tells us whether the engine's sim agrees across Linux, Windows and
macOS (aarch64). This feature turns the existing three-OS CI runs into that measurement, removes the two known
OS-dependent inputs inside the engine's own sim (platform libm in rapier and in character movement), and stops new
ones arriving. Each piece is cheap and harmless under either netcode model.
**Moves toward the end state by:** giving the netcode ADR a measured L3 result instead of a guess, and leaving the
remaining cross-platform gap (glam SIMD paths, R.2 section 2.4) isolated and named. The ENG-M3 two-sim proof (row 3.5)
reuses the state hash.

## Exit criteria

```gherkin
Scenario: A three-OS CI run reports whether the replay agrees across platforms
  Given a CI run on Linux, Windows and macOS
  When the replay run finishes on each
  Then the run's summary shows each platform's final-tick state hash
  And states whether all platforms agree
  And names the first tick at which any two disagree

Scenario: Cross-platform disagreement is reported, not a failure
  Given the platforms' replay hashes disagree
  When the three-OS CI run completes
  Then the run does not fail on that account

Scenario: Rapier's physics step uses no platform math library
  Given the workspace's physics dependency
  When the dependency features are listed
  Then rapier and parry are built with their enhanced-determinism mode
  And the replay still reproduces bit for bit on one machine

Scenario: New platform-libm calls in engine sim code fail the gate
  Given engine sim code that calls a platform transcendental such as f32::sin_cos
  When `just check` runs
  Then it fails and names the call site
```

- No engine sim path (character movement and its slide) calls a platform transcendental; presentation code
  (camera) is unaffected.
- The flagstone and searchlight lanes have a lane note listing their sim call sites and the rule.

## Scope

- In: a per-tick state hash for the replay scene; three-OS hash comparison in CI (report only); rapier
  `enhanced-determinism`; one deterministic math source for engine sim transcendentals, also used by glam's own
  transcendentals; a lint that refuses platform transcendentals in engine sim code; lane notes for the game call sites;
  a wiki page stating the determinism levels and the engine's rules.
- Out: game-line call sites (each line migrates under its own card, from the lane note); glam SIMD paths (`Quat`
  products, `Vec4`/`Vec3A` dot and normalize); an engine `Game` state-hash seam; anything that picks lockstep or
  snapshot.

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| Does cross-OS disagreement fail CI? | No: it is reported. | A blocking check is an L3 commitment, which only the netcode ADR may make; blocking would also stop `dev` → `main` promotions on a known, unfixed gap (glam SIMD). Flip to blocking when an ADR picks L3 |
| Which math library feeds sim transcendentals? | The `libm` crate, the same one rapier's `enhanced-determinism` and glam's `libm` feature use. | One implementation on both sides of the physics boundary, so values set up in our code and consumed by rapier come from the same functions. `alice-det-math` has a stronger cross-platform test suite but is a second implementation and a young crate; revisit if the measurement shows `libm` drifting |
| Which code counts as engine sim code for the lint? | Code that writes simulation state; presentation (camera, renderer, UI) is exempt. | Linting the whole workspace would force presentation code through `libm` or into allow attributes, which `CLAUDE.md` forbids. The Tech Lead draws the boundary |
| Where does the comparison run? | Wherever CI already runs all three OSes (pushes to `main`, manual runs). No new schedule. | PRs into `dev` are Linux-only by design; a weekly run is a cheap later addition if drift goes unnoticed between promotions |
| Left open | Lockstep or snapshot; L1 or L3; glam `scalar-math`; fixed-point for strategy. | Netcode ADR, ENG-M5+, from R.2 sections 4, 6 and 8 |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| glam SIMD paths out of sim code, or glam `scalar-math` | L3-only; unknown scope and speed cost (renderer too) | The cross-OS report shows divergence after this feature, and the netcode ADR picks L3 |
| Engine `Game::state_hash` seam and per-system hashes | No consumer until the two-sim proof or a netcode model | ENG-M3 row 3.5 (two-sim proof) |
| `enhanced-determinism` cost on a body-heavy scene | No body-heavy scene exists yet | ENG-F34 (characters and bodies), ENG-M3 |
| Guard against `target-cpu` / `+fma` build flags | Nobody sets them; recorded as a rule in the wiki page instead | A release build profile is defined (ENG-M4) |
| Weekly three-OS run | Promotions and manual runs may be enough | The report goes stale between promotions |
| `JobId` global counter, `VoxelGrid` `HashMap` iteration | Flagstone line, harmless until ids or chunk order reach state | Strategy sim rules iterate chunks or hash job ids |

## Items

| Item | Observable outcome | Depends on | Agent-run fit |
|---|---|---|---|
| ENG-F45-01 | A three-OS CI run reports each platform's replay state hash, whether they agree, and the first disagreeing tick | — | Interactive: edits `.github/workflows`, which the headless run's token may not push |
| ENG-F45-02 | Rapier steps in enhanced-determinism mode; replay still bit-equal; step time compared before and after | — | Headless: one feature flag, deny and replay prove it |
| ENG-F45-03 | Engine movement computes sin/cos through the deterministic math source (glam's too), and the gate refuses new platform transcendentals in engine sim code; lane notes go to flagstone and searchlight; the wiki page on determinism levels and rules lands here | — | Headless after refine; the lint boundary is the one judgement call |

All three are independent. Run -02 and -03 first if the first cross-OS report should show their effect; run -01 first
to record a before-and-after.

## Notes

- R.2 findings: `/mnt/project-files/determinism-research/findings.md` (sections 2.6, 2.7, 3, 4.B, 5, 7).
- `parallel` stays banned for `rapier3d` and `parry3d` in `deny.toml`; this feature does not revisit it.
