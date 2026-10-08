---
name: sync-backlog
description: Bring _todo/ in line with what actually landed - item tables, the README index, and board/card disagreements - and route new findings to existing cards or a proposed feature. Use after work merges or when findings need tracking.
argument-hint: "[scope: feature/item IDs, or blank for everything touched recently]"
disable-model-invocation: true
---

Sync the planning docs in `_todo/` with reality, following `_todo/_STANDARDS.md`.

1. **Establish what landed.** Merged PRs into `dev` since the last sync
   (`gh pr list --state merged --base dev`), the current branch's diff, and any
   findings raised in this session.
2. **Status.** The board (WrackedFella project 1) is the record; cards and features
   mirror Status, Gate class and Labels in their header (`_todo/_STANDARDS.md`, Item
   state). The README and the roadmap carry no status. Read the board with devflow's
   `scripts/board get` and the issue's labels with `gh issue view --json labels`.
   Overwrite each header field that differs from GitHub and list the change in the
   report, with one exception: **fill gaps upward.** Where GitHub has no value for a
   field (board Gate class or Status absent from `get`; the issue has no labels) and
   the header holds a concrete one, push the header value instead: `scripts/board set`
   for Gate class and Status, `gh issue edit --add-label` for Labels. Never push
   `unset`, `unknown` or `Draft`, never overwrite a value GitHub already has, and
   never create a label that does not exist; report any label that cannot be applied.
   If the issue is not on the board (`get` exits 4), `set` adds it; report that. List
   every pushed value in the report. Where a merged PR or an open PR disagrees with the board,
   report it for the user; change the board only on the user's say-so. Keep the feature's item table and `_todo/README.md` listing the right
   items. A feature is Done only when its exit criteria are verified, not when its cards are.
3. **Findings.** Route each new concern to an existing card (add a deliverable) or,
   if nothing fits, a new card under the right feature. Propose a new feature (no
   parent issue until approved) only when no feature's scope covers it; never create cards under an
   unapproved feature.
4. **Published copies.** Each issue body is the accepted spec (`_todo/_STANDARDS.md`,
   Issues and branches). Report any card or feature whose file differs from its issue
   body, or whose issue body is only a pointer; republish on the user's say-so.
5. **Clean up finished work** (`_todo/_STANDARDS.md`, Local drafts and cleanup).
   Delete the file of each card whose issue is closed as completed, and the directory
   of each feature whose issue is closed as completed. Point links to them at the issue
   instead. Leave open or not-planned items alone and report them.
6. **Hygiene.** Keep cards terse; no narration or history. Fix broken links between
   cards, features, the README and ADRs.

Report a short table: item → board Status → mismatch found, "new" or "deleted", plus
anything that needs the user's decision. Don't commit; leave that to `/devflow:ship` or the user.
