# Moho — Workflow rollout

How the agentic workflow itself evolves: stages, the bar for moving between them, and
evidence from real runs. [`ROADMAP.md`](ROADMAP.md) sequences the product; this file
sequences the process. How the workflow runs today:
[`wiki/process/agentic-workflow.md`](../wiki/process/agentic-workflow.md). Update this
file after each orchestrated run, and when a stage changes or a decision is made.

**Now:** Stage 1, supervised runs.

## Stages

Every stage runs the same pipeline. The stages differ only in where you take part:

```
Feature planning → Refinement (feature → work items) → Implementation → PR
```

| Stage | Feature planning | Refinement | Implementation | Bar to move on |
|---|---|---|---|---|
| 1. Supervised runs | You, in a planning session (BA, TL as needed) | You with BA and TL, card by card | You start and watch each `/devflow:orchestrate`, review domain tests, and fix the cause of each deviation (card format, skill or gate), not just the PR | About 5 consecutive runs reach a mergeable PR with no mid-run correction, and review finds only taste-level issues (no spec misreads, missed scope or broken gates). Runs cover domain, glue, bug-fix and mechanical cards |
| 2. Agents refine, you approve cards | You, in a planning session | BA and TL decompose the feature into cards and specs without you, as local drafts in `_todo/`. **You review every draft**; on approval, agents publish it as an issue on the board (Ready), which is canonical from then on | The line's project runs each approved card in its own thread to a PR. You review domain tests; otherwise you don't watch | Your judgment: you trust refinement enough to stop reviewing cards. Ask the question after the first feature completes Stage 2 |
| 3. Agents run from the feature | You, in a planning session. **Approving the feature starts the work** | BA and TL refine without you, and escalate questions to you only when the feature leaves them unresolved | Threads run each card to a PR with no domain-test pause | — (steady state) |

Stage 3 is switched on in `CLAUDE.md` by setting both human review points
(`Card review`, `Domain-test review`) to `not required`; devflow (v0.8.0 and later)
reads them.

In Stages 2 and 3, each line's Claude Code project is that line's lane
([coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes)), and
each card runs in its own thread (a cloud session on its own branch).

Fixed in every stage:

- Only humans merge. You review every PR.
- PRs are where you review. Agents answer comments with commits and replies, and never
  resolve threads.
- Feature planning stays with you. Agents implement only cards from an approved
  feature, and in Stages 1–2 only cards you approved.

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
- Stage 3 runtime: how approving a feature starts its project (board automation, a
  GitHub Action, or the project's coordinator), and where BA/TL questions reach you.
- Agent pre-review before human PR review: put the `devflow:reviewer` verdict in the PR
  body, and possibly add a second independent pass. To decide once the trial log
  shows what human review catches that the reviewer missed.
- Whether cloud sessions install the project-pinned devflow plugin. The docs say a
  single-repo cloud session reads the committed `.claude/settings.json`; unconfirmed
  in a real run.
- Whether a project thread can run the full Stage 2 run: `just check` needs the Rust
  toolchain in the cloud image, `scripts/board` needs GitHub Projects access, and the
  domain-test pause needs a way to reach you (the Threads panel or the PR).

## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-02 | Adopt the phased workflow: A gate, B agent config, C CI, E process; D (GitHub-triggered agents) deferred until local agent PRs go cleanly | [agentic workflow](../wiki/process/agentic-workflow.md) |
| 2026-10-02 | Roles are user-invoked skills; workers are agents (test-writer and implementer on Sonnet, reviewer on Opus) | devflow v0.1.0 |
| 2026-10-06 | Board Status is the only record of item state | [`CLAUDE.md`](../CLAUDE.md) |
| 2026-10-07 | Three-stage rollout with a bar per stage; Phase D is Stage 3 | this file |
| 2026-10-07 | Parallel lanes: one card in progress per lane, one worktree each; cross-line needs go through engine requests | [coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes) |
| 2026-10-07 | Stage 2 runs on Claude Code Projects: one project per line is the lane, and threads replace worktrees. Independent cards run in parallel threads; dependent ones run in sequence or on a `feature/` branch. Human approval sits after Tech Lead, before orchestrate. Supersedes the worktree half of the row above | [coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes) |
| 2026-10-07 | Stages are defined by human involvement across one pipeline (feature planning → refinement → implementation → PR). Stage 2: you plan features and review cards. Stage 3: you plan features and answer escalations; approving the feature starts the work, and there is no domain-test pause. The 2→3 move is your judgment, raised after the first Stage 2 feature. Replaces "Phase D is Stage 3": the board trigger is now a runtime detail | this file |
| 2026-10-07 | Human review points are project settings (`Card review`, `Domain-test review`, default required) read by devflow's BA, Tech Lead and Orchestrator; Stage 3 turns both off. Local `_todo/` files are drafts: you review them, agents publish on approval, the issue is canonical, and finished items' files are deleted | devflow v0.8.0, [`CLAUDE.md`](../CLAUDE.md), [`_STANDARDS.md`](_STANDARDS.md#local-drafts-and-cleanup) |
| 2026-10-07 | The orchestrator keeps `wiki/` current through `/devflow:wiki`, where a change warrants it: onboarding docs on structures, patterns and conventions. New wiki pages are approved in PR review, replacing "new docs need the user's OK first" for `wiki/` | devflow v0.9.0, [`CLAUDE.md`](../CLAUDE.md) |
