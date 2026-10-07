# Every direct dependency has its facts collected

**Feature:** [ENG-F13](_feature.md)
**Issue:** #85

## Summary

The mechanical half of the audit: gather, with no judgement, the facts every
verdict rests on, so later passes can read a table instead of the code.

## Deliverables

- A facts table in [`audit.md`](audit.md) with one row per direct dependency
  of every workspace crate (normal, dev, build, optional).

## Acceptance criteria

- [ ] Every direct dependency declared in any workspace `Cargo.toml` has a row; none is missing.
- [ ] Each row records: version, declaring crates, kind (normal/dev/optional),
      call-site count and the modules using it (or "unused"), licence
      expression, open advisories, transitive dependency count, duplicate
      versions it brings into the tree, compile time from one clean
      `cargo build --timings`, last release date, maintainer count, and
      downloads.
- [ ] Rows already decided elsewhere (`bincode`, `winit`, `gilrs`, ENG-F2-03's
      unused list) are marked with the deciding record.
- [ ] Each fact is sourced from a command or crates.io, not estimated; unknown values say "unknown".

## Notes

Mechanical; suited to a cheap model.
