# Agentic workflow

Humans decide what to build and merge the results; agents implement against approved
specs behind deterministic gates. How the workflow itself works (flow, roles, agents,
skills, the project contract, coordination and runtimes) is documented in the
[devflow wiki](https://github.com/WrackedFella/claude-skills/blob/main/wiki/README.md). This page holds what is specific to Moho; the
switches and todo list are in [`_todo/WORKFLOW.md`](../../_todo/WORKFLOW.md).

Work-item state lives on the [Moho project board](https://github.com/users/WrackedFella/projects/1),
and each item's record is its GitHub issue: the body is the spec, comments hold
deviations and decisions. Files in `_todo/` are drafts until approved. The wiki is the
source of truth for documentation.

## Moho's flow

The generic flow is in the devflow [overview](https://github.com/WrackedFella/claude-skills/blob/main/wiki/overview.md). In Moho:

| Step | Who | How | Output |
|---|---|---|---|
| 1. Shape a feature | You + Business Analyst | `/devflow:business-analyst <topic>` | Draft `_feature.md`; on approval the feature issue is filed and the draft deleted |
| 2. Write work items | You + Business Analyst | same session | Cards with Gherkin acceptance criteria |
| 3. Specify | You + Tech Lead | `/devflow:tech-lead <item>` | Tech spec, test map, gate class; ADR if needed |
| 4. Queue | You approve; Tech Lead publishes | You approve the draft (a local file, or a `plan/` branch from a cloud thread). The same thread files the issue (`[ID] title`, full spec; cloud threads use `gh`) and deletes the draft. The Board sync workflow sets Ready from the tech spec and gate class; Agent-eligible comes from the card's board header | Issue |
| 5. Implement | Orchestrator | `/devflow:orchestrate <issue>` | Branch from `dev`, PR into `dev` |
| 6. Review and merge | You | Review the PR; merge | Board Status → Done (Board sync workflow) |

The wiki step of the orchestrator updates `wiki/` following the conventions in
[`wiki/README.md`](../README.md); new pages are flagged in the PR for your review.
Switch settings and where runs happen: [`_todo/WORKFLOW.md`](../../_todo/WORKFLOW.md#run-options).

## Gates

| Gate | Where | Enforced by |
|---|---|---|
| `just check`: fmt, clippy `-D warnings`, nextest, doctests, comment refs | Every agent stop with Rust changes; CI | Stop hook, CI |
| `just mutants` on changed domain code | Orchestrator step; PR evidence; a survivor fails the PR check | Orchestrator, CI |
| Weekly full mutation run | Whole workspace, glue included; survivors reviewed, not gated | Scheduled CI |
| `just deny` | CI | CI |
| Format on edit | Every Rust edit in a single-repo session | PostToolUse hook |
| CI matrix | Linux on PRs into `dev`; three OSes on `main` and manual runs | CI |
| Test review | Domain tests, before implementation | `devflow:test-critic` (or you under `required`) |
| Human review | Every PR, including the tests | You |

## Coordinating lanes

The generic rules (one thread per card, shared files belong to planning, cross-lane
requests, broad moves) are in the devflow [coordination](https://github.com/WrackedFella/claude-skills/blob/main/wiki/coordination.md) page.
Moho's instances:

- **Lanes:** engine, strategy, FPS, one Claude Code project each (labels `line:engine`,
  `line:strategy`, `line:fps`). Projects can't see each other's threads or memory, so
  everything one lane needs from another is written down here, in issues, and carried
  between projects by the user as a relay note (below).
- **Engine requests:** a game lane files the *Engine request* issue template (label
  `engine-request` plus its line label); the engine lane designs the answer. The layering
  check enforces the dependency half. A request closes when the engine work that answers
  it merges; the game line's card tracks adoption.
- **Engine notices:** the engine lane's way back. Any engine decision that changes what a
  game line can use, must change, or should expect is written as a comment on the issue
  the game lane already tracks, then relayed. See [Hand-offs between lanes](#hand-offs-between-lanes).
- **Shared files:** `wiki/roadmap.md`, `_todo/README.md`, `CLAUDE.md`, the root
  `Cargo.toml` and `scripts/layering.txt` are edited only by planning sessions.
- **Broad moves:** for example ENG-F10's voxel extraction.
- **Lane context:** a line's own crates carry a short `CLAUDE.md` pointing at its design doc.

### Hand-offs between lanes

A hand-off is complete only when both halves exist: the issue in the repo (the record)
and the relay note the user pastes into the receiving project (the trigger). The thread
that makes the hand-off ends its reply with the relay note, in a fenced block, one per
receiving lane:

```text
Relay to: <Engine | Strategy | FPS>
From: <lane>, <date>
Item: #<issue>, <ID>, <short description>
What changed or is asked: <1–3 lines>
Action for the receiving lane: <FYI only | update <card or feature> | re-plan <ID> | reply on #<issue>>
```

**Game lane → Engine (request).**

1. File the *Engine request* issue (`engine-request` + line label). State what the game
   must be able to do, not a solution. Name the consumer (feature or card ID), when it is
   needed (milestone), and the fallback if it doesn't come (cut line or stopgap).
2. Mark the dependent card as blocked by the request. Don't work around a missing engine
   feature inside engine crates, and don't copy engine code into game crates.
3. End the reply with a relay note to the Engine project.

**Engine → game lanes (decision).** Decisions that need a notice: accepting, rejecting,
deferring or reshaping a request; adding or changing a seam or public API; accepting or
superseding an ADR; a roadmap reorder that moves a request; a breaking change; a
dependency upgrade at milestone close. No label or new issue: the record goes where the
game lane already looks. For each, the deciding thread does both before reporting the
decision as done:

1. Comment on the issue the game lane tracks: the `engine-request` when there is one,
   otherwise the affected game feature issue (found by its `line:` label). The comment
   states the decision, why, what the game lane should do now, and links the ADR, PR or
   roadmap change.
2. End the reply with a relay note per affected game project.

**Game lane receiving a notice.** On the relay note, start a thread to read the comment,
update the affected cards and feature issues (or propose the re-plan when a change needs
the user's approval), and reply on the same issue with what changed. Before planning a
new feature or milestone, check the lane's open `engine-request` issues for engine
comments it hasn't acted on.

**Triage.** The engine lane triages each new `engine-request` in its first thread on it:
accept into the roadmap with a milestone, reshape with a reason, or decline with the
fallback the game lane should take. Each outcome is a decision above and is relayed.

## Cloud runtime

How cloud threads and Actions runs differ from a local session is in the devflow
[runtimes](https://github.com/WrackedFella/claude-skills/blob/main/wiki/runtimes.md) page. Moho's setup:

- **devflow delivery.** The account-level cloud environment's setup script installs
  devflow at user scope from `main` and refreshes it on each rebuild; it serves all three
  line projects. There is no version to bump. See
  [Anthropic: cloud environments](https://code.claude.com/docs/en/cloud-environments) and
  [projects](https://code.claude.com/docs/en/claude-projects).
- **Toolchain.** Threads that build or test run `bash scripts/cloud-tools.sh` first; it installs the pinned Rust toolchain and gate tools from release downloads.
- **GitNexus starts itself.** `.mcp.json` launches `scripts/gitnexus-mcp.sh`: it installs
  the npm dependency if missing (skipping the ONNX download the sandbox proxy resets),
  indexes in the background, and serves. The first calls on a fresh clone see no repo
  until the index finishes (~20 s); `cloud-tools.sh` runs the same script with
  `--setup`. Full-text search stays off in the sandbox; `impact`, `context` and
  `detect_changes` work. Refresh a stale index with
  `node_modules/.bin/gitnexus analyze --skip-agents-md`.
- **Board.** Only the Board sync workflow (`BOARD_TOKEN`) moves Status; agents never run
  `scripts/board` in cloud sessions or Actions.
- **Unattended runs** use `claude-code-action` (`.github/workflows/claude.yml`) with the
  default token; the `agent-ready` routing is described in
  [`_todo/WORKFLOW.md`](../../_todo/WORKFLOW.md#run-options).
