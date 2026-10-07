# ENG-F13 — Every third-party dependency has a recorded, reasoned verdict

**Issue:** #83

## End state

The engine ships as a separately versioned repo ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md))
that two closed-source games build on. Every library it carries was chosen
deliberately, so the split copies no unexamined choices into three repos.

## Summary

Library choices so far are unaudited. Each direct dependency, and each planned
addition, gets two verdicts: an economical one and a lean-engine one. The user
records a call per crate. Analysis, not code: follow-up work becomes cards
under [ENG-F2](../ENG-F2-dependency-upgrades/_feature.md), or scope in the feature that already reworks the code.
**Moves toward the end state by:** unblocking ENG-F12 (gamepad), ENG-F14
(scene format), ENG-F16 (audio) and ENG-F19 (data format) to add dependencies,
and closing an M1 gate item.

## Exit criteria

- An audit table under this feature lists every direct dependency (including
  dev and optional ones) with: purpose and call sites, maintenance signals,
  licence against [ADR-0007](../../adr/0007-third-party-licence-policy.md),
  advisories, transitive weight, duplicate versions in the tree, and compile
  cost.
- Every row has an **economical verdict** and a **lean-engine verdict**, each
  keep / replace / fork / homebrew / drop with a one-line reason, judged by the
  rubric below.
- Where the two verdicts differ, the lean column estimates the cost: how much of
  the crate is used, rough homebrew size and the tests that would prove it; for
  a fork, the licence, inherited unsafe code and the share of the crate used.
- Planned additions have the same two verdicts: gamepad, scene format (glTF),
  image decoding, `tracing`, an audio alternative, and the data/mod file format.
- Every row has the user's call recorded.
- Each call other than keep has an owner: an ENG-F2 card, or a scope line in
  the feature that already reworks that code. The audit links each call to it.
- `just deny` passes.

## Verdict rubric

1. **Green light:** a large, actively maintained, widely used crate with no red
   flag is keep on both verdicts. This overrides the other criteria.
2. **Red flags:** an advisory or unmaintained notice; a licence outside ADR-0007;
   duplicate versions it brings into the tree; weight out of proportion to the
   surface we use; a stale single-maintainer project; blocking a planned
   feature.
3. **Economical verdict:** keep unless a red flag applies.
4. **Lean-engine verdict:** prefer owning the code where the used surface is
   small and testable; keep where no realistic homebrew exists.

## Scope

- In: direct dependencies of all workspace crates; planned additions; the
  user's call per row; an owner for each call other than keep.
- Out: performing replacements (ENG-F2 cards, or the feature that reworks that code). Rows already decided elsewhere
  are cited, not re-analysed: `bincode` ([ADR-0006](../../adr/0006-save-format-contract.md)),
  `winit` and `gilrs` ([ADR-0008](../../adr/0008-keep-winit-for-windowing-and-input.md)),
  unused declarations ([ENG-F2-03](../ENG-F2-dependency-upgrades/ENG-F2-03-platform-default-features-and-unused-deps.md)).

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| How hard does the audit push for fewer dependencies? | Two verdicts per row; the user calls each | A single lean bar delays ENG-F12/F14; a single economical bar hides what lean would cost |
| Do large professional crates need a homebrew case? | No: green light unless a red flag applies | Proving we can't rebuild wgpu or rapier is wasted analysis |
| Where do follow-up cards go? | ENG-F2, or the feature already reworking that code (`phf` → ENG-F12) | Under ENG-F13 the M1 gate would wait on replacements shipping |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| Scripting runtime and netcode candidates | No consumer before M5; their ADRs will pick candidates against current options | Scripted-mods ADR, netcode ADR |
| Auditing transitive dependencies one by one | `just deny` already covers advisories and licences; weight is captured per direct row | A transitive crate raises a red flag |
| A standing admission checklist for new dependencies | The rubric covers this audit; a policy needs a second use to shape it | M3 repo split |

## Items

| Item |
|---|
| [ENG-F13-01 Every direct dependency has its facts collected](ENG-F13-01-dependency-facts-collected.md) |
| [ENG-F13-02 Large crates are triaged against the green light](ENG-F13-02-large-crates-triaged.md) |
| [ENG-F13-03 Small crates have economical and lean-engine verdicts](ENG-F13-03-small-crates-have-both-verdicts.md) |
| [ENG-F13-04 Planned dependencies have verdicts before features add them](ENG-F13-04-planned-additions-have-verdicts.md) |
| [ENG-F13-05 Each dependency has the user's call, and follow-ups are queued](ENG-F13-05-calls-recorded-and-follow-ups-queued.md) |

## Notes

- Must finish before ENG-F12, ENG-F14, ENG-F16 or ENG-F19 adds a dependency.
- Known small candidates: `once_cell`, `crossbeam-channel`, `ini`/`phf` key
  tables, `noise`'s duplicate `rand`, `log` → `tracing`.
