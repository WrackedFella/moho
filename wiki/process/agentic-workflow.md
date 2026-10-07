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
| 4. Queue | You | File the issue (`[ID] title`, links card), add to the board; Status → Ready, Agent-eligible → Yes | |
| 5. Implement | Orchestrator | `/devflow:orchestrate <issue>` | Branch from `dev`, test-first commits, PR into `dev` |
| 6. Review and merge | You | Review the PR; merge | Board Status → Done |

During step 5 the Orchestrator pauses for your review of failing tests when the item's
gate class is `domain`. Otherwise it runs through: failing tests → implementation →
adversarial challenges (as tests) → mutation testing → `/simplify` →
`/devflow:comment-audit` → fresh-context `devflow:reviewer` → `/devflow:ship`.

## Gates

| Gate | Where | Enforced by |
|---|---|---|
| `just check`: fmt, clippy `-D warnings`, nextest, doctests, comment refs | Every agent stop with Rust changes; CI | Stop hook, CI |
| `just mutants` on changed code | Orchestrator step; PR evidence | Orchestrator, CI (Phase C) |
| `just deny` | CI | CI (Phase C) |
| Human review | Domain tests; every PR | You |

## Writing a good work item

Agents implement exactly what the card says, so vague cards produce vague code.

- Name the observable outcome, not the component.
- Every acceptance scenario has an observable or assertable `Then`.
- Say what's out of scope.
- Put anything that can only be checked by playing under Verification.

## Other commands

`/devflow:sitrep [ID]` for status, `/next` for the next increment, `/sync-backlog`
after merges, `/devflow:ship` to open a PR by hand.
