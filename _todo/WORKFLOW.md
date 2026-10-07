# Moho — Workflow rollout

How the agentic workflow itself evolves: stages, the bar for moving between them, and
evidence from real runs. [`ROADMAP.md`](ROADMAP.md) sequences the product; this file
sequences the process. How the workflow runs today:
[`wiki/process/agentic-workflow.md`](../wiki/process/agentic-workflow.md). Update this
file after each orchestrated run, and when a stage changes or a decision is made.

**Now:** Stage 1, supervised runs.

## Stages

| Stage | What changes | Bar to move on |
|---|---|---|
| 1. Supervised runs | You start and watch each `/devflow:orchestrate`, intervene as needed, and fix the cause of each deviation (card format, skill or gate), not just the PR | About 5 consecutive runs reach a mergeable PR with no mid-run correction, and review finds only taste-level issues (no spec misreads, missed scope or broken gates). Runs cover domain, glue, bug-fix and mechanical cards |
| 2. Parallel lanes, started by you | One agent per line, each in its own worktree ([coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes)). You queue work on the board and start each lane, but don't watch it | Lanes run side by side without conflicts or cross-line edits. The review queue stays short. Engine requests flow without you relaying them |
| 3. The board starts the work | Moving a card to Ready on the [board](https://github.com/users/WrackedFella/projects/1) triggers a run (GitHub Action or scheduled agent; Phase D) | — (steady state) |

Fixed in every stage:

- Only humans merge.
- Domain tests pause for your review. In Stage 3 the agent posts them on the PR and
  waits.
- PRs are where you review. Agents answer comments with commits and replies, and never
  resolve threads.
- Agents run only approved cards. Planning stays with you: Claude projects for design,
  Business Analyst and Tech Lead sessions for cards.

## Trial log

One line per orchestrated run. **Outcome:** clean (mergeable, no correction),
corrected (needed intervention mid-run or after review) or failed. Unknown entries
(—) are backfilled from the PRs.

| Date | Card | Class | PR | Outcome | Deviation → fix |
|---|---|---|---|---|---|
| 2026-10-02 | ENG-F1-06: UI layer carries no unsafe code | glue | #58 | corrected | Deviated from the card (recorded in ENG-F1). Gap found: no skill filed the issue or set `agent-ready` → trial-1 fixes (#59), devflow v0.2.0 |
| 2026-10-04 | ENG-F8-01: `moho_sim` merged into `moho_game` | glue | #67 | — | — |
| 2026-10-04 | ENG-F7-01: typed chunk and actor stores | domain | #68 | — | — |
| 2026-10-04 | ENG-F8-02: layering check in `just check` | glue | #69 | — | — |
| 2026-10-05 | ENG-F7-02: legion removed | glue | #70 | — | — |
| 2026-10-05 | ENG-F1-08: headless app wiring tests | glue | #71 | — | — |

Clean-run streak: unknown until the entries above are backfilled.

## Open questions

- Per-line `CLAUDE.md` files ([lane rules](../wiki/process/agentic-workflow.md#coordinating-lanes))
  don't exist yet; they are needed before Stage 2.
- Stage 3 runtime: local or cloud runs. Affects cost, secrets, and how the domain-test
  pause works without a terminal.
- Whether cloud sessions install the project-pinned devflow plugin (unverified).

## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-02 | Adopt the phased workflow: A gate, B agent config, C CI, E process; D (GitHub-triggered agents) deferred until local agent PRs go cleanly | [agentic workflow](../wiki/process/agentic-workflow.md) |
| 2026-10-02 | Roles are user-invoked skills; workers are agents (test-writer and implementer on Sonnet, reviewer on Opus) | devflow v0.1.0 |
| 2026-10-06 | Board Status is the only record of item state | [`CLAUDE.md`](../CLAUDE.md) |
| 2026-10-07 | Three-stage rollout with a bar per stage; Phase D is Stage 3 | this file |
| 2026-10-07 | Parallel lanes: one card in progress per lane, one worktree each; cross-line needs go through engine requests | [coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes) |
