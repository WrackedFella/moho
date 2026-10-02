# Planning Standards

How work items are organized and written. Applies to every project line.

## Structure

```
_todo/
  README.md                     <- index: lines, features, status
  _STANDARDS.md                 <- this file
  engine/ENG-F<n>-<slug>/       <- engine crates (anything that ships in moho-engine)
  strategy-game/SG-F<n>-<slug>/ <- strategy/base-builder game
  fps-game/FPS-F<n>-<slug>/     <- FPS prototype
    _feature.md
    <LINE>-F<n>-<NN>-<slug>.md  <- one card per work item
```

A work item belongs to the line whose crates it changes. Cross-line work goes
in `engine/`.

## IDs

`<LINE>-F<n>` is a feature, `<LINE>-F<n>-<NN>` a card: `SG-F1-04`,
`ENG-F3-01`, `FPS-F1`. IDs are unique across the repo and never reused; use
them verbatim in issue titles and branch names, never in code comments.

## Card lifecycle

| Status | Meaning | Required sections |
|---|---|---|
| `not started` | Backlog | Summary, Deliverables |
| `ready` | Spec approved; issue filed and labeled `agent-ready` | + Acceptance criteria, Tech spec |
| `in progress` | On a branch | |
| `done` | Merged to `dev` | |
| `parked` | Deliberately paused | |
| `deferred` | Blocked on an external condition (say what) | |

The implementing PR updates the card's status, so the merged card is the
source of truth.

## Card template

```markdown
# <observable outcome, not the component touched>

**Status:** not started
**Feature:** SG-F1
**Issue:** #<n>            <!-- once filed -->

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
  **domain** (`moho_sim`, `moho_game`, `moho_core` rules: tests reviewed
  before implementation) or **glue** (adapters/wiring: tests and code
  together).
- ADR link if the item makes or relies on an architectural decision.

## Writing rules

Terse; a card is not a design doc. Cut narration, draft history, and
file:line citations (they rot; name types and modules instead). A card over
~100 lines is two cards. Features (`_feature.md`) hold a summary, an item
table with status, and notes only for cross-item constraints.

Name features and cards after the behavior delivered ("Hotbar displays mined
resources"), not the component touched. A feature is a vertical slice:
something observable when finished.

## Issues and branches

- Issue title: `[<ID>] <card title>`; body links the card. The card holds the
  spec; the issue is the queue and discussion thread.
- Labels: `line:engine` | `line:strategy` | `line:fps`; `agent-ready` once
  the card is `ready`.
- Branch from `dev`: `<type>/<ID>-<slug>` (e.g. `feat/SG-F1-04-pickup-feedback`);
  PR back into `dev`. `main` receives promotions from `dev` only.

## Definition of Done (shared; don't repeat per card)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  and `cargo test --workspace --all-features` pass.
- Agent-written tests pass mutation testing on the changed code.
- Comment audit run on the diff.
- Verification steps performed for anything observable.
- Card status updated in the same PR.
