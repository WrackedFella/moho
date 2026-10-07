# Agentic workflow

Humans decide what to build and merge the results; agents implement against approved
specs behind deterministic gates. Roles, agents and generic skills come from the
`devflow` plugin ([claude-skills](https://github.com/WrackedFella/claude-skills)),
pinned in `.claude/settings.json`. Work-item state lives on the
[Moho project board](https://github.com/users/WrackedFella/projects/1), not in the cards.

## Flow

| Step | Who | How | Output |
|---|---|---|---|
| 1. Shape a feature | You + Business Analyst | `/devflow:business-analyst <topic>` | `_feature.md` with exit criteria, you approve scope; feature issue filed |
| 2. Write work items | You + Business Analyst | same session | Cards with Gherkin acceptance criteria |
| 3. Specify | You + Tech Lead | `/devflow:tech-lead <item>` | Tech spec, test map, gate class; ADR if needed |
| 4. Queue | You approve; Tech Lead publishes | You approve the local draft. The Tech Lead files the issue (`[ID] title`, full spec), adds it to the board with Status → Ready and Agent-eligible → Yes | Issue, canonical from here on |
| 5. Implement | Orchestrator | `/devflow:orchestrate <issue>` | Branch from `dev`, test-first commits, PR into `dev` |
| 6. Review and merge | You | Review the PR; merge | Board Status → Done |

During step 5 the Orchestrator pauses for your review of failing tests when the item's
gate class is `domain` and `CLAUDE.md` requires domain-test review. Otherwise it runs through: failing tests → implementation →
adversarial challenges (as tests) → mutation testing → `/simplify` →
`/devflow:comment-audit` → fresh-context `devflow:reviewer` → `/devflow:ship`.

## Gates

| Gate | Where | Enforced by |
|---|---|---|
| `just check`: fmt, clippy `-D warnings`, nextest, doctests, comment refs | Every agent stop with Rust changes; CI | Stop hook, CI |
| `just mutants` on changed code | Orchestrator step; PR evidence | Orchestrator, CI (Phase C) |
| `just deny` | CI | CI (Phase C) |
| Human review | Domain tests; every PR | You |

## Coordinating lanes

Several agents can work at once, one lane per line (engine, strategy, FPS), plus an
optional lane for cheap work against a written spec. Each kind of information has one
home, so lanes never need to talk to each other:

| Layer | Holds | Written by |
|---|---|---|
| Claude Code projects (one per line) | The lane: design discussion, planning and implementation threads | You and the project's coordinator; decisions leave as cards, ADRs or hand-off blocks |
| `_todo/` cards, ADRs, `wiki/` | Decisions and specs | Planning sessions |
| Project board | State and priority | Agents via `scripts/board`; you |
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

## Writing a good work item

Agents implement exactly what the card says, so vague cards produce vague code.

- Name the observable outcome, not the component.
- Every acceptance scenario has an observable or assertable `Then`.
- Say what's out of scope.
- Put anything that can only be checked by playing under Verification.

## Other commands

`/devflow:sitrep [ID]` for status, `/next` for the next increment, `/sync-backlog`
after merges, `/devflow:ship` to open a PR by hand.
