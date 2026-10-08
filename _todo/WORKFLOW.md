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

Where each stage runs:

| Stage | Runs in |
|---|---|
| 1 | A local terminal, or an implementation thread in a line's project that you start and watch. Both satisfy "you start and watch", and a thread also exercises the cloud runtime Stages 2 and 3 depend on |
| 2 | Line projects: planning threads draft on `plan/` branches ([drafts](_STANDARDS.md#local-drafts-and-cleanup)), implementation threads run each approved card. Remote runs ([below](#remote-runs)) are optional |
| 3 | Remote runs started by approval, with line projects for planning and escalations |

## Remote runs

The unattended path (formerly Phase D): a GitHub Action runs the orchestrator on one
issue, with no thread and no terminal.

- **Trigger:** a human applies the `agent-ready` label to the issue of a Ready,
  Agent-eligible item, or dispatches the workflow with an issue number
  (`.github/workflows/claude.yml`). The label starts a run; it is not an item status
  ([cards](_STANDARDS.md#card-readiness)).
- **Runner:** `anthropics/claude-code-action` with the prompt
  `/devflow:orchestrate <issue>`. The workflow installs the toolchain with
  `scripts/cloud-tools.sh` and devflow from the marketplace (the action cannot pin a
  tag, so the run follows the marketplace's default branch; the repo pin in
  `.claude/settings.json` does not apply).
- **Access:** the default Claude App token only. There is no PAT and no Projects or
  GraphQL access in the agent, so it never touches the board. The Board sync workflow
  moves Status from PR and issue events, and Ready and Agent-eligible stay human-set.
- **Human gates become PR-level.** The run ends at a PR into `dev` and never merges.
  The orchestrator's domain-test pause has nobody to answer it in an Actions run, so
  until Stage 3 sets `Domain-test review: not required`, apply `agent-ready` to `glue`
  items only and run `domain` items as threads.
- **Runtime:** one run per issue at a time, 60 minutes, 200 turns. The action skips
  PRs that edit workflow files, so changes to `claude.yml` need a human merge.

Fixed in every stage:

- Only humans merge. You review every PR.
- PRs are where you review. Agents answer comments with commits and replies, and never
  resolve threads.
- Feature planning stays with you. Agents implement only cards from an approved
  feature, and in Stages 1–2 only cards you approved.

## Trial log

One line per orchestrated run. **Outcome:** clean (mergeable, no correction),
corrected (needed intervention mid-run or after review) or failed. Unknown entries
(—) are backfilled from the PRs. Rows from 2026-10-04 were backfilled on 2026-10-08
from the PR bodies: they show deviations a PR recorded, not corrections made in
conversation, so the owner confirms each outcome.

| Date | Card | Class | PR | Outcome | Deviation → fix |
|---|---|---|---|---|---|
| 2026-10-02 | ENG-F1-06: UI layer carries no unsafe code | glue | #58 | corrected | Deviated from the card (recorded in ENG-F1). Gap found: no skill filed the issue or set `agent-ready` → trial-1 fixes (#59), devflow v0.2.0 |
| 2026-10-04 | ENG-F8-01: `moho_sim` merged into `moho_game` | glue | #67 | clean | Card's `git grep ':!_todo'` pathspec fails on this git; the card was fixed in the PR. First `just mutants` run had 22 survivors (accessors never tested in `moho_sim`) → tests added. No fix needed beyond the card |
| 2026-10-04 | ENG-F7-01: typed chunk and actor stores | domain | #68 | corrected | Forked `/devflow:comment-audit` ran in the session's primary checkout, not the worktree, and edited nothing; audit done by hand. 2 mutation survivors killed with tests → devflow fork/checkout bug, open (moot for one-clone cloud threads; recheck on the first thread run) |
| 2026-10-04 | ENG-F8-02: layering check in `just check` | glue | #69 | corrected | Windows-only failure (bash stripped backslashes from `just_executable()`) caught by full-platform CI and fixed; `/simplify` regressed errexit, caught in review; reviewer found two rule branches with no assertion; same comment-audit wrong-checkout bug → platform-CI rule worked as intended |
| 2026-10-05 | ENG-F7-02: legion removed | glue | #70 | corrected | Generated GitNexus skill copies committed by accident (reviewer caught); a sibling session edited the tree; two scope forks decided by the user (`collect` split, `paste` advisory moved to ENG-F2-04); 18 mutation survivors in untestable binary wiring → ENG-F1-08. Card scope was too large for one run |
| 2026-10-05 | ENG-F1-08: headless app wiring tests | glue | #71 | corrected | `/devflow:comment-audit` and the card's manual verification not done; stacked on #70 to clear its survivors (0 missed) → run ended with pipeline steps skipped |

Clean-run streak: 0. Of the six runs only #67 was clean, and the latest (#71) was
corrected. Stage 1's bar of about 5 consecutive clean runs is unmet.

## Open questions

- Whether a thread's or Action's first message can be a slash command, and whether the
  Skill-tool calls inside `orchestrate` (`/simplify`, `/devflow:comment-audit`,
  `/devflow:wiki`) succeed there. Settle with the first `claude.yml` dispatch and the
  first supervised thread run; record the result here and in the trial log.
- Stage 3 trigger: whether approving a feature applies `agent-ready` to its cards
  automatically (Board sync cannot, since it only moves Status) or a human still labels
  each card, and where BA/TL questions reach you.
- Agent pre-review before human PR review: put the `devflow:reviewer` verdict in the PR
  body, and possibly add a second independent pass. To decide once the trial log
  shows what human review catches that the reviewer missed.
- Per-line `CLAUDE.md` files ([lane rules](../wiki/process/agentic-workflow.md#coordinating-lanes))
  don't exist yet; they are needed before Stage 2. A subdirectory `CLAUDE.md` loads
  when a thread touches that directory, so a few lines at each line's crate root is
  enough.
- Forked `/devflow:comment-audit` ran in the wrong checkout in #68 and #69. Recheck in a
  one-clone cloud thread before changing devflow.

Settled by runs and decisions below: cloud threads get devflow from the environment's
setup script rather than the repo pin; the toolchain is installed on demand by
`scripts/cloud-tools.sh`; cloud sessions cannot reach GraphQL or Projects v2, so board
Status is event-driven.

Expected but unverified: a thread's domain-test pause reaches you as its waiting-on-you
state, with the tests pushed as the branch's `test(...)` commit so you can read them on
GitHub. Confirm on the first supervised thread run.

## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-02 | Adopt the phased workflow: A gate, B agent config, C CI, E process; D (GitHub-triggered agents) deferred until local agent PRs go cleanly. D is built as [Remote runs](#remote-runs) | [agentic workflow](../wiki/process/agentic-workflow.md) |
| 2026-10-02 | Roles are user-invoked skills; workers are agents (test-writer and implementer on Sonnet, reviewer on Opus) | devflow v0.1.0 |
| 2026-10-06 | Board Status is the only record of item state | [`CLAUDE.md`](../CLAUDE.md) |
| 2026-10-07 | Three-stage rollout with a bar per stage; Phase D is Stage 3. *Superseded in part:* Phase D is a runtime detail, see the stage-definition row below | this file |
| 2026-10-07 | Parallel lanes: one card in progress per lane, one worktree each; cross-line needs go through engine requests. *Superseded:* worktrees by the Claude Code Projects row below; the rest stands | [coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes) |
| 2026-10-07 | Stage 2 runs on Claude Code Projects: one project per line is the lane, and threads replace worktrees. Independent cards run in parallel threads; dependent ones run in sequence or on a `feature/` branch. Human approval sits after Tech Lead, before orchestrate. Supersedes the worktree half of the row above | [coordination rules](../wiki/process/agentic-workflow.md#coordinating-lanes) |
| 2026-10-07 | Stages are defined by human involvement across one pipeline (feature planning → refinement → implementation → PR). Stage 2: you plan features and review cards. Stage 3: you plan features and answer escalations; approving the feature starts the work, and there is no domain-test pause. The 2→3 move is your judgment, raised after the first Stage 2 feature. Replaces "Phase D is Stage 3": the board trigger is now a runtime detail | this file |
| 2026-10-07 | Human review points are project settings (`Card review`, `Domain-test review`, default required) read by devflow's BA, Tech Lead and Orchestrator; Stage 3 turns both off. Local `_todo/` files are drafts: you review them, agents publish on approval, the issue is canonical, and finished items' files are deleted | devflow v0.8.0, [`CLAUDE.md`](../CLAUDE.md), [`_STANDARDS.md`](_STANDARDS.md#local-drafts-and-cleanup) |
| 2026-10-07 | The orchestrator keeps `wiki/` current through `/devflow:wiki`, where a change warrants it: onboarding docs on structures, patterns and conventions. New wiki pages are approved in PR review, replacing "new docs need the user's OK first" for `wiki/` | devflow v0.9.0, [`CLAUDE.md`](../CLAUDE.md) |
| 2026-10-07 | Board Status is event-driven, not agent-written: the Board sync workflow moves Status forward on PR and issue events (secret `BOARD_TOKEN`); Ready and Agent-eligible stay human-set. Cloud sessions cannot reach GraphQL or Projects v2, so `scripts/board` runs locally only | moho #109, [`_STANDARDS.md`](_STANDARDS.md#item-state-lives-on-the-board) |
| 2026-10-08 | Cloud environment: devflow is installed by the environment's setup script (user scope), not Project settings or the repo pin; the Rust toolchain and gate tools are on demand through `scripts/cloud-tools.sh`, run by threads that build or test. devflow v0.10.0 removed `disable-model-invocation`, so orchestrate, BA and TL can be invoked by agents | moho #112, #127, #128; devflow v0.10.0 |
| 2026-10-08 | Remote runs use `claude-code-action`: a human-applied `agent-ready` label on an issue runs `/devflow:orchestrate <issue>` with the default Claude App token, no PAT and no board access. Rejected: a PAT wrapper, and giving the agent Projects or GraphQL access. Human gates become PR-level | moho #126, [Remote runs](#remote-runs) |
