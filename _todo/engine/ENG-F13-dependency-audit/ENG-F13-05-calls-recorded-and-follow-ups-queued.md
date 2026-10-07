# Each dependency has the user's call, and follow-ups are queued

**Feature:** [ENG-F13](_feature.md)
**Issue:** #89

## Summary

Closes the feature: verdicts from ENG-F13-02 to -04 are reviewed against the
rubric, the user records a call on every row, and each call other than keep
becomes work under ENG-F2.

## Deliverables

- Reviewed [`audit.md`](audit.md) with a "Call" column filled on every row.
- An owner for each call other than keep: an ENG-F2 card, or a scope line in
  the feature that already reworks that code.

## Acceptance criteria

- [ ] An independent review checks each verdict follows the rubric and cites
      its facts; disputed rows are corrected or flagged to the user.
- [ ] Every row has the user's call.
- [ ] Every non-keep call has an owner, linked from the audit's "Where each call
      is carried out" table; ENG-F2's item table lists its new cards.
- [ ] The roadmap backlog line for dependency candidates points to the cards
      or is removed.
- [ ] `just deny` passes.
