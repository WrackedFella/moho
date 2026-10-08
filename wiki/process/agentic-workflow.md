# Agentic workflow

Humans decide what to build and merge the results; agents implement against approved
specs behind deterministic gates. Roles, agents and generic skills come from the
`devflow` plugin ([claude-skills](https://github.com/WrackedFella/claude-skills)),
pinned in `.claude/settings.json` for local sessions (cloud sessions ignore the pin; see
[Cloud runtime](#cloud-runtime)). Work-item state lives on the
[Moho project board](https://github.com/users/WrackedFella/projects/1); cards mirror Status,
Gate class and Labels in their header, kept current by a sync agent with board access.

## Flow

| Step | Who | How | Output |
|---|---|---|---|
| 1. Shape a feature | You + Business Analyst | `/devflow:business-analyst <topic>` | `_feature.md` with exit criteria, you approve scope; feature issue filed |
| 2. Write work items | You + Business Analyst | same session | Cards with Gherkin acceptance criteria |
| 3. Specify | You + Tech Lead | `/devflow:tech-lead <item>` | Tech spec, test map, gate class; ADR if needed |
| 4. Queue | You approve; Tech Lead publishes | You approve the draft (a local file, or a `plan/` branch from a cloud thread). The Tech Lead files the issue (`[ID] title`, full spec); a cloud thread, which cannot write board fields, leaves the card as a local draft and `/sync-backlog` files it from a full-access session. Ready and Agent-eligible are set by you on the board, or by the Tech Lead through `scripts/board` in a local terminal | Issue, canonical from here on |
| 5. Implement | Orchestrator | `/devflow:orchestrate <issue>` | Branch from `dev`, test-first commits, PR into `dev` |
| 6. Review and merge | You | Review the PR; merge | Board Status → Done (set by the Board sync workflow) |

During step 5 the Orchestrator pauses for your review of failing tests when the item's
gate class is `domain` and `CLAUDE.md` sets `Domain-test review: required`. Under
`agent` (the current setting) it does not pause: `devflow:test-critic` tries to break the
pushed tests in a fresh context (at most 2 rounds), the test-writer fixes what it finds,
and the PR lists the findings. Otherwise it runs
through: failing tests → implementation → adversarial challenges (as tests) → refactor
under green plus `/simplify` → mutation testing → `/devflow:comment-audit` →
`/devflow:wiki` → full-platform CI when the change is platform-sensitive →
fresh-context `devflow:reviewer` → `/devflow:ship`. Each stage commit is pushed, so a
cloud sandbox that cannot resume loses nothing.

The wiki step updates `wiki/` where the change adds or alters a structure, pattern or
convention a new developer needs, following the conventions in
[`wiki/README.md`](../README.md). Otherwise the PR says why no docs were needed. New
pages are flagged in the PR for your review.

## Gates

| Gate | Where | Enforced by |
|---|---|---|
| `just check`: fmt, clippy `-D warnings`, nextest, doctests, comment refs | Every agent stop with Rust changes; CI | Stop hook, CI |
| `just mutants` on changed code | Orchestrator step; PR evidence | Orchestrator, CI (Phase C) |
| `just deny` | CI | CI (Phase C) |
| Format on edit | Every Rust edit in a single-repo session | PostToolUse hook |
| CI matrix | Linux on PRs into `dev`; three OSes on `main` and manual runs | CI |
| Test review | Domain tests, before implementation | `devflow:test-critic` (or you under `required`) |
| Human review | Every PR, including the tests | You |

## Coordinating lanes

Several agents can work at once, one lane per line (engine, strategy, FPS), plus an
optional lane for cheap work against a written spec. Each kind of information has one
home, so lanes never need to talk to each other:

| Layer | Holds | Written by |
|---|---|---|
| Claude Code projects (one per line) | The lane: design discussion, planning and implementation threads | You and the project's coordinator; decisions leave as cards, ADRs or hand-off blocks |
| `_todo/` cards, ADRs, `wiki/` | Decisions and specs | Planning sessions |
| Project board | State and priority | Board sync workflow (Status); you (Ready, Agent-eligible, priority) |
| Card header fields | Mirror of board Status, Gate class, labels | Sync agent (copies from GitHub) |
| Session transcripts | Nothing durable | — |

Rules:

- **One thread per card.** Each thread is a cloud session with its own branch and its
  own copy of the repo, so threads see only what is committed: hand-offs travel through
  cards, issues and the
  board, not project files. Independent cards may run in parallel; cards that depend on
  each other or share files run in sequence or on a `feature/` integration branch. Add
  parallel threads only while the review queue stays short: human review is the
  bottleneck, not agents.
- **Approval comes before implementation.** A project may run Business Analyst and
  Tech Lead threads. They leave cards as local drafts in `_todo/`, which you review;
  on approval, agents publish each one as an issue on the board (Ready). An
  implementation thread starts only for a published card. From Stage 3 of the
  [rollout](../../_todo/WORKFLOW.md#stages), approving the feature is enough.
- **Cross-line needs go through an engine request,** not a shared edit. A game lane
  files the request (issue template *Engine request*), and the engine lane designs the
  answer. Game lanes never edit engine crates as a side effect; the layering check
  enforces the dependency half of this.
- **Shared files belong to planning.** Only planning sessions and `/sync-backlog` edit
  `_todo/ROADMAP.md`, `_todo/README.md`, `CLAUDE.md`, the root `Cargo.toml` and
  `scripts/layering.txt`. An implementation PR edits its own card and code.
- **Broad moves are announced and kept short.** A change that touches many files across
  lines (such as ENG-F10's voxel extraction) pauses the affected lanes, lands on a
  short-lived integration branch, and lanes rebase afterwards.
- **Line context loads by directory.** A line's own crates carry a short `CLAUDE.md`
  pointing at its design doc and constraints, so an agent working there picks it up
  without being told.
- **Sync after each batch of merges:** `/devflow:sitrep project`, then `/sync-backlog`,
  then update each project's instructions if a line's rules changed.

## Cloud runtime

Cloud threads and Actions runs start from a fresh clone and differ from a local session:

- **devflow comes from the environment, not the repo pin.** Anthropic's docs say plugins
  enabled in a repository's `.claude/settings.json` are not loaded in cloud sessions, and
  name **Project settings > Plugins** as the route for project threads. The setup script
  is documented for command-line tools and packages, not plugins. This workflow installs
  devflow at user scope from the environment's setup script anyway, so one account-level
  environment serves all three line projects and pins a tag. Two consequences: the
  script's `DEVFLOW_REF` must be bumped by hand with `.claude/settings.json`, and the
  environment is a filesystem snapshot rebuilt when the script changes or after about
  seven days, so a new tag reaches threads only after a rebuild.
  [Anthropic: cloud environments](https://code.claude.com/docs/en/cloud-environments),
  [projects](https://code.claude.com/docs/en/claude-projects).
- **The toolchain is on demand.** Threads that build or test run
  `bash scripts/cloud-tools.sh` first; text-only threads skip it and start faster.
- **GitNexus comes with that script.** It installs the npm dependency (skipping the
  ONNX download the sandbox proxy resets) and builds the gitignored index; `.mcp.json`
  registers the server and `.claude/settings.json` enables it. Full-text search stays
  off in the sandbox; `impact`, `context` and `detect_changes` work. Re-run
  `npx gitnexus analyze --skip-agents-md` after commits to refresh a stale index.
- **No board access.** Cloud sessions cannot reach GraphQL or Projects v2, so only the
  Board sync workflow (`BOARD_TOKEN`) moves Status, forward, on PR and issue events.
  Agents never run `scripts/board` there.
- **Unattended runs** use `claude-code-action` (`.github/workflows/claude.yml`): a human
  applies `agent-ready` to a Ready, Agent-eligible issue and the run executes
  `/devflow:orchestrate <issue>` with the default token. On a `feature` issue the label
  runs `/devflow:refine <issue>` instead, which publishes the feature's cards as
  sub-issues for you to label in turn. See
  [`_todo/WORKFLOW.md`](../../_todo/WORKFLOW.md#remote-runs).

## Writing a good work item

Agents implement exactly what the card says, so vague cards produce vague code.

- Name the observable outcome, not the component.
- Every acceptance scenario has an observable or assertable `Then`.
- Say what's out of scope.
- Put anything that can only be checked by playing under Verification.

## Other commands

`/devflow:sitrep [ID]` for status, `/next` for the next increment, `/sync-backlog`
after merges, `/devflow:ship` to open a PR by hand.
