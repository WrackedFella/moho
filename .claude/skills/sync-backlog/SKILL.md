---
name: sync-backlog
description: Bring _todo/ in line with what actually landed - card and feature statuses, item tables, the README index - and route new findings to existing cards or a proposed feature. Use after work merges or when findings need tracking.
argument-hint: "[scope: feature/item IDs, or blank for everything touched recently]"
---

Sync the planning docs in `_todo/` with reality, following `_todo/_STANDARDS.md`.

1. **Establish what landed.** Merged PRs into `dev` since the last sync
   (`gh pr list --state merged --base dev`), the current branch's diff, and any
   findings raised in this session.
2. **Statuses.** For each affected card, set its status from evidence (merged PR, open
   PR, nothing yet); never mark `done` without a merged PR. Update the feature's item
   table and `_todo/README.md` to match. A feature is `done` only when its exit
   criteria are verified, not when its cards are.
3. **Findings.** Route each new concern to an existing card (add a deliverable) or,
   if nothing fits, a new card under the right feature. Propose a new feature (status
   `proposed`) only when no feature's scope covers it; never create cards under an
   unapproved feature.
4. **Hygiene.** Keep cards terse; no narration or history. Fix broken links between
   cards, features, the README and ADRs.

Report a short table: item → old status → new status (or "new"), plus anything that
needs the user's decision. Don't commit; leave that to `/devflow:ship` or the user.
