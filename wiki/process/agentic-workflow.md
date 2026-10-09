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
| 4. Queue | You approve; Tech Lead publishes | You approve the draft (a local file, or a `plan/` branch from a cloud thread). The same thread files the issue (`[ID] title`, full spec; cloud threads use `gh`) and deletes the draft. You set Ready and Agent-eligible on the board, or the Tech Lead does through `scripts/board` in a local terminal | Issue |
| 5. Implement | Orchestrator | `/devflow:orchestrate <issue>` | Branch from `dev`, PR into `dev` |
| 6. Review and merge | You | Review the PR; merge | Board Status → Done (Board sync workflow) |

The wiki step of the orchestrator updates `wiki/` following the conventions in
[`wiki/README.md`](../README.md); new pages are flagged in the PR for your review.
Switch settings and where runs happen: [`_todo/WORKFLOW.md`](../../_todo/WORKFLOW.md#run-options).

## Gates

| Gate | Where | Enforced by |
|---|---|---|
| `just check`: fmt, clippy `-D warnings`, nextest, doctests, comment refs | Every agent stop with Rust changes; CI | Stop hook, CI |
| `just mutants` on changed code | Orchestrator step; PR evidence | Orchestrator, CI |
| `just deny` | CI | CI |
| Format on edit | Every Rust edit in a single-repo session | PostToolUse hook |
| CI matrix | Linux on PRs into `dev`; three OSes on `main` and manual runs | CI |
| Test review | Domain tests, before implementation | `devflow:test-critic` (or you under `required`) |
| Human review | Every PR, including the tests | You |

## Coordinating lanes

The generic rules (one thread per card, shared files belong to planning, cross-lane
requests, broad moves) are in the devflow [coordination](https://github.com/WrackedFella/claude-skills/blob/main/wiki/coordination.md) page.
Moho's instances:

- **Lanes:** engine, strategy, FPS, one Claude Code project each.
- **Engine requests:** a game lane files the *Engine request* issue template; the
  engine lane designs the answer. The layering check enforces the dependency half.
- **Shared files:** `_todo/ROADMAP.md`, `_todo/README.md`, `CLAUDE.md`, the root
  `Cargo.toml` and `scripts/layering.txt` are edited only by planning sessions.
- **Broad moves:** for example ENG-F10's voxel extraction.
- **Lane context:** a line's own crates carry a short `CLAUDE.md` pointing at its design doc.

## Cloud runtime

How cloud threads and Actions runs differ from a local session is in the devflow
[runtimes](https://github.com/WrackedFella/claude-skills/blob/main/wiki/runtimes.md) page. Moho's setup:

- **devflow delivery.** The account-level cloud environment's setup script installs
  devflow at user scope, pinned to a tag, and serves all three line projects. Bump the
  script's `DEVFLOW_REF` together with `.claude/settings.json`. See
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
