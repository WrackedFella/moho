# Small crates have economical and lean-engine verdicts

**Feature:** [ENG-F13](_feature.md)
**Issue:** #87

## Summary

The crates where owning the code is a real option get both verdicts and an
honest estimate of what lean would cost, so the user can call each.

## Deliverables

- A verdict row in [`audit.md`](audit.md) for each of: `once_cell`,
  `crossbeam-channel`, `ini`, `phf`, `log`, `env_logger`, `thiserror`,
  `pollster`, and the dev dependencies `proptest`, `criterion`, `tempfile`.

## Acceptance criteria

- [ ] Each row has an economical and a lean-engine verdict (keep / replace /
      fork / homebrew / drop), each with a one-line reason under the rubric.
- [ ] Where the verdicts differ: the used surface, a rough homebrew size, and
      the tests that would prove it; a replace names the replacement (std or
      another crate); a fork records licence, inherited unsafe code and the
      share of the crate used.
- [ ] `log`'s row weighs `tracing` as its replacement candidate.
- [ ] Every verdict cites a fact from ENG-F13-01's table.
