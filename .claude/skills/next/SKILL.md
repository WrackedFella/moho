---
name: next
description: Short status, then the next logical increment of work and whether it needs planning, design discussion or can start now. Use when asking "what's next?".
disable-model-invocation: true
---

1. Run `/devflow:sitrep` for the project overview.
2. Pick the next increment, in this order: finishing something in flight (open PR
   feedback, red CI, an In progress item on the board) beats starting new work; then the
   highest-priority Ready item on the board (`scripts/board next`); then the feature closest to its exit criteria.
   Respect pauses noted in `_todo/README.md`.
3. Say what it needs before work can start:
   - **Ready:** board Status is Ready; start with `/devflow:orchestrate <issue>`.
   - **Needs spec:** acceptance criteria or tech spec missing →
     `/devflow:business-analyst` or `/devflow:tech-lead`.
   - **Needs a decision:** name the decision and the options in one line each.

Answer in under 10 lines. Don't start the work unless the user says to.
