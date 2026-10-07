# Planned dependencies have verdicts before features add them

**Feature:** [ENG-F13](_feature.md)
**Issue:** #88

## Summary

ENG-F12, ENG-F14, ENG-F16 and ENG-F19 will each add a dependency. Each need
gets a chosen candidate, with both verdicts, before those features start.

## Deliverables

- A verdict row in [`audit.md`](audit.md) for each need: gamepad input
  (ENG-F12), scene format (ENG-F14), image decoding (ENG-F14), structured
  logging/tracing, an audio alternative to `rodio` (ENG-F16), and the
  data/mod file format (ENG-F19).

## Acceptance criteria

- [ ] Each need lists its candidates (including "homebrew" and "none needed")
      with licence, maintenance signals and transitive weight.
- [ ] Each need has an economical and a lean-engine verdict naming one
      candidate, with a one-line reason under the rubric.
- [ ] Gamepad confirms or challenges ADR-0008's `gilrs` choice; scene format
      confirms or challenges the 2026-10-05 glTF 2.0 recommendation.
- [ ] Each chosen candidate's licence is allowed by ADR-0007.
