# Moho — Workflow

How features reach a merged PR: the flow, who does each step, and the switches that
change how it runs. Humans decide what to build and merge the result; agents write specs,
tests and code behind deterministic gates. Moho specifics:
[`wiki/process/agentic-workflow.md`](../wiki/process/agentic-workflow.md). How the workflow
works in general: the [devflow wiki](https://github.com/WrackedFella/claude-skills/blob/main/wiki/README.md). Update this file
when a switch or a todo changes.

```
Feature → Cards (spec per card) → Implementation (one card per run) → PR → Merge
```

## Run options

| Switch | Where | Values | Effect |
|---|---|---|---|
| `Card review` | [`CLAUDE.md`](../CLAUDE.md) | `required` (now), `not required` | `required`: refine and the Tech Lead leave cards unlabeled and a human approves each one (applies `agent-ready`). `not required`: the Tech Lead, refine and publish label the cards that pass the promotion checks (Gherkin criteria, test map, gate class, footprint, no open question, not human-driven), and the workflow dispatches a run per card. Approving the feature is then enough to start work |
| `Domain-test review` | [`CLAUDE.md`](../CLAUDE.md) | `agent` (now), `required`, `not required` | Applies to `domain` cards. `agent`: no pause; `devflow:test-critic` attacks the pushed failing tests (max 2 rounds) and the PR lists its findings. `required`: the orchestrator stops after the tests for a human to approve. `not required`: no review; the PR lists the tests as unreviewed |
| `agent-ready` label | Issue | applied by a human (or by the Tech Lead, refine or publish under `Card review: not required`) | Starts a run. A `feature` issue goes to `/devflow:refine`; any other issue goes to `/devflow:orchestrate`. Not a status |
| `Agent-eligible` | Board | Yes / No | `No` keeps the card for a human to drive. Set by hand, with Ready |
| Gate class | Card | `domain`, `glue` | Decides whether `Domain-test review` applies |
| Where it runs | — | Actions, Project thread, local terminal | See below |

Where it runs:

| Runtime | Start it with | Notes |
|---|---|---|
| GitHub Actions (`claude.yml`) | Label `agent-ready`, or manual dispatch with an issue number | Unattended. Default Claude App token only, no board access. One run per issue at a time; refine 30 min / 100 turns, orchestrate 60 min / 200 turns. devflow follows the marketplace's default branch, not the repo pin. Edits to workflow files need a human merge |
| `@claude` comment (`claude-comment.yml`) | A comment by an owner, member or collaborator on an issue or PR | For small jobs such as resolving a PR's merge conflict |
| PR review (`pr-review.yml`) | Dispatched after an orchestrate run opens a PR, or a PR marked ready for review | `/devflow:pr-review` posts a verdict as a comment (`PR approval` unset, so no approval). On `changes`, `/devflow:respond` fixes the findings and the review runs again: two fix rounds, then a human. Comment `@claude /devflow:pr-review` to run it by hand |
| Project thread | `/devflow:orchestrate <issue>` or a planning skill | Cloud session on its own branch. Run `bash scripts/cloud-tools.sh` first when it must build. Cannot reach the board. Files an approved card as an issue with `gh` and deletes its draft; the Board sync workflow moves Status |
| Local terminal | Same slash commands | The repo pin in `.claude/settings.json` applies; the Stop hook runs the gate |

Always on: only humans merge; agents answer PR comments with commits and replies and never
resolve threads; a feature is planned by a human before agents implement any of its cards.

## Board state

Board Status is the record of item state and moves by event (Board sync workflow, secret
`BOARD_TOKEN`): `agent-ready` on a work item or a draft PR → In progress; PR ready for
review → In review; merged PR or closed issue → Done; an issue body with a `Tech spec`
heading and a board header naming the gate → Ready. Status only moves forward. Running
the workflow by hand (Backfill, dry run by default) moves every item to the Status its
issue implies. Agent-eligible is set by a human, through the card's board header.

## Todo

- **PR agent:** `pr-review.yml` reviews and hardens each PR before a human sees it (comment-only; a human approves and merges). Open: an approving identity separate from the PR author, so `PR approval: agent` can submit approvals; check how often the loop converges within two fix rounds before widening it.
- Confirm in Actions and threads that the Skill-tool calls inside `orchestrate`
  (`/simplify`, `/devflow:comment-audit`, `/devflow:wiki`) succeed.
- Verify refine on a real feature: questions land as a comment, a re-run adds no
  duplicate cards, and parallel card runs don't collide on shared manifests.
- Add a short `CLAUDE.md` at each line's crate root so a thread picks up its line's rules.
- Recheck that forked `/devflow:comment-audit` runs in the thread's checkout (it ran in
  the wrong one in a local worktree run).
- Recheck Project settings for a Plugins option; if present, move devflow delivery from
  the environment setup script to it.
- Run benchmarks weekly or on demand instead of on every `dev` push; delete the stale
  `.github/CICACHE.md` and `.github/docker`.
