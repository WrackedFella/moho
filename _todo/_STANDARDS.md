# Planning Standards

How work items are organized and written. Applies to every project line.

## Structure

```
_todo/
  README.md                     <- index: lines and features
  _STANDARDS.md                 <- this file
  ROADMAP.md                    <- order of work: milestones, gates, decisions
  adr/NNNN-<slug>.md            <- architecture decision records (index: adr/README.md)
  engine/ENG-F<n>-<slug>/       <- engine crates (anything that ships in moho-engine)
  strategy-game/SG-F<n>-<slug>/ <- strategy/base-builder game
  fps-game/FPS-F<n>-<slug>/     <- FPS prototype
    _feature.md                 <- draft only, until the feature issue is filed
    <LINE>-F<n>-<NN>-<slug>.md  <- draft card, until its issue is filed
    <reference files>           <- kept, e.g. ENG-F13's audit.md
```

The GitHub issue is the record of a feature or work item. A file in `_todo/` holds an
item only while it is an unapproved draft; filing the issue deletes the file.

A work item belongs to the line whose crates it changes. Cross-line work goes
in `engine/`.

## IDs

`<LINE>-F<n>` is a feature, `<LINE>-F<n>-<NN>` a card: `SG-F1-04`,
`ENG-F3-01`, `FPS-F1`. IDs are unique across the repo and never reused; use
them verbatim in issue titles and branch names, never in code comments.

## Item state: the board and the issue

The [project board](https://github.com/users/WrackedFella/projects/1) and the issue are
the only record of item state. Nothing in the repository mirrors them.

- **Board:** Status, Priority, Gate class and Agent-eligible. The Board sync workflow
  moves Status forward on PR and issue events (In progress when `agent-ready` is applied
  to a work item or a draft PR opens, In review on a ready PR, Done on merge or close).
  Ready and Agent-eligible are set by a human. Agents change board fields only through
  devflow's `scripts/board` (local terminal only); a cloud thread does not touch the
  board, and an item in the wrong state is a workflow bug to report.
- **Issue:** the spec (body), labels, and the history of decisions (comments).
- **Draft files** carry a header for the thread that will file them: `**Status:** Draft`,
  the intended `**Gate class:**` (`domain`, `glue` or `unset`) and `**Labels:**`
  (`line:engine` | `line:strategy` | `line:fps`, plus `feature` on features,
  `engine-request`). The header is not part of the issue body.

The README index and the roadmap carry no status. Reasons an item is paused or blocked go
in an issue comment (or a draft's `**Note:**` line), not in `Status`.

## Features first

Planning produces a feature before any cards. A feature is the unit of
planning and the gate for progress; cards are how it gets built.

1. Draft `_feature.md`: the end state, the outcome, its exit criteria, and deferred
   scope.
2. The user approves scope and exit criteria; the feature then gets its parent issue
   (see Issues and branches) and the draft file is deleted. A feature with no `feature`
   issue is a proposal.
3. Only then decompose into cards. A card exists only under an approved
   feature, and only for work inside that feature's scope. Work that falls
   outside it is a new feature proposal, not a stray card.
4. The feature is Done on the board (and its issue closed) when its exit criteria are verified, not merely when
   its cards are done. Missing coverage becomes a new card.

A feature written without exit criteria or scope gets them the next time it is planned,
not in a sweep.

## Milestones

[`ROADMAP.md`](ROADMAP.md) orders work as milestones (M0, M1, ...). Each has
an **Intent** line and a **Gate** list; a milestone closes when every gate item
holds.

Only the next open milestone is carded. Later milestones hold intent and gate
only (their features may exist as drafts, without cards) and are detailed
when the previous gate closes.

## Feature template

```markdown
# <LINE>-F<n> — <observable outcome>

**Status:** Draft          <!-- draft header; see Item state -->
**Labels:** feature, line:engine

## End state                  <!-- where this is heading; link the vision doc/GDD -->
Two or three sentences: what the finished system does, and for whom.

## Summary
One paragraph: what the player/caller can do when this ships, and why it matters now.
**Moves toward the end state by:** what this unlocks next.

## Exit criteria
- Observable, verifiable conditions that close the feature (Gherkin welcome
  for the headline behaviors). These are the gate.

## Scope
- In: ...
- Out: ...   <!-- what a reader might assume is included but isn't -->

## Direction-setting decisions  <!-- forks expensive to reverse; open ones the design must not preclude -->
| Question | Decision | Why / cost of the alternative |
|---|---|---|

## Deferred                   <!-- ideas cut from this increment; nothing is lost -->
| Idea | Why it waits | Revisit when |
|---|---|---|

## Items
| Item |
|---|

## Notes                      <!-- cross-item constraints only -->
```

## Card readiness

A card's readiness is its board Status. What each state needs in the issue body:

| Board Status | Required sections |
|---|---|
| Backlog | Summary, Deliverables |
| Needs spec | Summary, Deliverables; acceptance criteria or tech spec pending |
| Ready | + Acceptance criteria, Tech spec; issue filed with Gate class set |

Ready plus Agent-eligible Yes is what makes an item available to the orchestrator.
Paused or blocked items stay at their Status; say why in an issue comment.

The `agent-ready` label is a run trigger, not a status. A human applies it to the issue
of an item that is Ready and Agent-eligible Yes, and that starts a remote run
([Run options](WORKFLOW.md#run-options)). Nothing reads it as readiness, and nothing
removes it. The issue type picks the run: on a work item the orchestrator implements it,
and on a `feature` issue the refine skill splits it into work items.

## Card template

```markdown
# <observable outcome, not the component touched>

**Feature:** SG-F1
**Status:** Draft          <!-- draft header; see Item state -->
**Gate class:** unset      <!-- domain | glue, set by the Tech Lead -->
**Labels:** line:strategy

## Summary
1-3 sentences: what this does and why.

## Deliverables
- Checkable outcomes, not tasks.

## Acceptance criteria        <!-- Business Analyst -->
## Tech spec                  <!-- Tech Lead -->
## Verification               <!-- manual steps, only if observable in-game -->
## Notes                      <!-- only for a gotcha that would be re-litigated -->
```

### Acceptance criteria (Business Analyst)

Behavior only, from the player's or caller's point of view. Use Gherkin when
the item has observable behavior; a checklist otherwise (refactors, upgrades).

```gherkin
Scenario: Mining without a tool is refused
  Given the player has no tool equipped
  When the player mines a stone block
  Then the block remains
  And the inventory is unchanged
```

One behavior per scenario; outcomes must be observable or assertable. Use
`Scenario Outline` + `Examples` for tables of cases. Rendering and feel can't
be asserted, so those go under Verification instead.

### Tech spec (Tech Lead)

- Crates/modules and public interfaces touched; anything out of scope.
- Test map: each scenario → the test that proves it
  (`crate::module::tests::scenario_expected_result`), and its gate class:
  **domain** (the paths `CLAUDE.md` lists under Domain-logic paths: tests reviewed
  before implementation, by you under `required` or by `devflow:test-critic` under `agent`) or **glue**
  (adapters/wiring: tests and code together).
- ADR link if the item makes or relies on an architectural decision.

## Writing rules

Terse; a card is not a design doc. Cut narration, draft history, and
file:line citations (they rot; name types and modules instead). A card over
~100 lines is two cards. A feature stays a gate, not a design doc: summary,
exit criteria, scope and an item table.

Name features and cards after the behavior delivered ("Hotbar displays mined
resources"), not the component touched. A feature is a vertical slice:
something observable when finished.

## Issues and branches

- Issue title: `[<ID>] <title>`. The issue is the record: once it is filed, the spec
  lives only in its body and nothing in `_todo/` copies it.
- The body is the full spec and stands alone: every section of the card or feature,
  minus the draft header lines (`**Feature:**`, `**Status:**`, `**Gate class:**`,
  `**Labels:**`) and template comments. Refer to
  other items as `#N` (with ID and a few words) and to the parent through the
  sub-issue link, never by `_todo/` path. An item with no issue yet (a proposed
  feature) is named by ID and a few words until it is filed. Link ADRs and wiki
  pages by URL on `dev`.
- **When scope changes, edit the issue body directly.** Before Ready the edit needs no
  comment; from Ready on, also comment saying what changed and why.
- **Deviations and decisions go in an issue comment:** what an implementation did
  differently from the spec, a question settled mid-run, why an item is blocked. The
  PR links the comment when it carries one.
- An approved feature gets a parent issue labeled `feature` (issue types are not
  available on a user-owned repo); each card's issue is a sub-issue of it, so feature
  progress is visible on GitHub.
- Labels: `line:engine` | `line:strategy` | `line:fps`; `feature` on feature issues;
  `engine-request` on engine requests; `agent-ready` to start a remote run: on a work item it starts the orchestrator, on a
  `feature` issue it starts refinement into work items. Templates: *Feature*, *Work item*, *Engine
  request*.
- Every issue goes on the board; Status, Priority, Gate class and Agent-eligible are set there.
- Branch from `dev`: `<type>/<ID>-<slug>` (e.g. `feat/SG-F1-04-pickup-feedback`);
  PR back into `dev`. `main` receives promotions from `dev` only.

## Drafts

- A card or feature starts as a draft file in `_todo/`. Drafts are not on the board.
- **Where the draft lives while it is reviewed.** In a local terminal the user reads the
  file in the working tree. A cloud thread's files exist only on its branch, so the
  planning thread commits drafts to `plan/<ID>-<slug>` (branched from the item's base
  branch) and pushes after each draft, because unpushed work is lost if the sandbox
  does not resume. The user reviews in the thread, or on a draft PR when the draft is
  long or needs inline comments, and answers there.
- **On approval,** the same thread files the issue (`[ID] title`, full spec, labels from
  the draft header, sub-issue of its feature's issue) and deletes the draft file. A
  draft that never reached the base branch leaves with its `plan/` branch or PR; one
  that did is deleted there by the filing thread. Cloud threads file with `gh` and the
  repo token. Without card review (see `CLAUDE.md`), the Tech Lead files as soon as the
  card is ready.
- **Remote refinement skips the draft file.** A `feature` issue labeled `agent-ready`
  starts `/devflow:refine`, which publishes the cards it writes straight to issues, as
  sub-issues of the feature, and writes nothing to the repository. The unlabeled issue
  is the draft. While `Card review` is required, you read each card and apply
  `agent-ready` to it, and that label is the approval and the start of its run. Once
  `Card review` is `not required`, refine applies the label itself and the workflow
  dispatches the runs. An open question reaches you as a comment on the feature.
- Reference files that are not items (such as `engine/ENG-F13-dependency-audit/audit.md`)
  stay. A feature directory is deleted once it holds neither drafts nor reference files.
  Links to items point at their issues, never at `_todo/` files.

## Definition of Done (shared; don't repeat per card)

- `just check` passes.
- Agent-written tests survive mutation testing on the changed code (`just mutants`).
- `/devflow:comment-audit` run on the diff.
- `/devflow:wiki` run on the diff: the PR lists the pages updated or says none were
  needed.
- Verification steps performed for anything observable.
- Board Status moved by the workflow. Deviations from the spec are in an issue comment,
  and scope changes are in the issue body; the PR edits no `_todo/` item file.
