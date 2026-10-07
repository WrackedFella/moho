---
name: sync-backlog
description: Bring _todo/ in line with what actually landed - item tables, the README index, and board/card disagreements - and route new findings to existing cards or a proposed feature. Use after work merges or when findings need tracking.
argument-hint: "[scope: feature/item IDs, or blank for everything touched recently]"
---

Sync the planning docs in `_todo/` with reality, following `_todo/_STANDARDS.md`.

1. **Establish what landed.** Merged PRs into `dev` since the last sync
   (`gh pr list --state merged --base dev`), the current branch's diff, and any
   findings raised in this session.
2. **Status.** Cards, features, the README and the roadmap carry no status; the board
   (WrackedFella project 1) is the record. Read it with devflow's `scripts/board get`.
   Where a merged PR, an open PR or a card disagrees with the board, report it for the
   user; never edit a card's status to resolve it, and change the board only on the
   user's say-so. Keep the feature's item table and `_todo/README.md` listing the right
   items. A feature is Done only when its exit criteria are verified, not when its cards are.
3. **Findings.** Route each new concern to an existing card (add a deliverable) or,
   if nothing fits, a new card under the right feature. Propose a new feature (no
   parent issue until approved) only when no feature's scope covers it; never create cards under an
   unapproved feature.
4. **Hygiene.** Keep cards terse; no narration or history. Fix broken links between
   cards, features, the README and ADRs.

Report a short table: item → board Status → mismatch found (or "new"), plus anything that
needs the user's decision. Don't commit; leave that to `/devflow:ship` or the user.
