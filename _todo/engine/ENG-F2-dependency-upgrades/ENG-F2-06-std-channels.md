# Event and job channels use the standard library

**Feature:** [ENG-F2](_feature.md)
**Issue:** #99

## Summary

ENG-F13 call: drop `crossbeam-channel`. The workspace uses only unbounded
send/receive, `try_recv`, `try_iter` and `recv_timeout`, all of which
`std::sync::mpsc` provides ([audit](../ENG-F13-dependency-audit/audit.md)).

## Deliverables

- No workspace crate declares `crossbeam-channel`.

## Acceptance criteria

- [ ] Event routing, generation jobs and UI adapter channels behave as before: existing tests pass.
- [ ] `just check` passes with `crossbeam-channel` absent from every manifest.

## Notes

If a later job pool needs to wait on several channels at once (`select!`),
that need reopens the verdict; don't hand-roll a select.
