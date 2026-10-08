# The strategy game's app state lives on the strategy line

**Feature:** ENG-F12

## Summary

`moho_types` is an engine-row crate that holds only the strategy game's
`GameState` (menu, playing, console, paused) and its transition coordinator.
Moving both to the strategy line empties it, so it is deleted
([ADR-0012](../../adr/0012-engine-crate-map-for-m2.md)).

## Deliverables

- `GameState` and the transition coordinator live in a strategy-line crate.
- `moho_types` is gone from the workspace, the layering file and every manifest.
- The strategy game behaves as before.

## Acceptance criteria

- [ ] No crate, manifest, workspace member or `scripts/layering.txt` row names `moho_types`.
- [ ] The coordinator's existing tests pass unchanged, in their new home.
- [ ] The binary has one `GameState` type: the identity conversion between two copies is gone.
- [ ] `just check` passes, including the layering check.

## Tech spec

**Design.**
- Move `moho_types/src/app_state.rs` and `state_coordinator.rs` into `moho_ui`
  as a public `app_state` module; `moho_ui` re-exports `GameState`,
  `StateTransitionActions` and `StateTransitionCoordinator` at its root, where
  `GameState` is already re-exported today.
- Why `moho_ui`: it and the binary are the only users, the binary already
  depends on it, and the state decides UI visibility and cursor mode. Not
  `moho_game`: app flow is not a game rule, and `moho_game` is a domain path
  (review gate) for no gain.
- The binary's `src/game_state.rs` re-export and the `moho_types` →
  `moho_ui` match in `main.rs` (`apply_transition`) collapse to the one type.
- Delete `moho_types/`; remove it from root `Cargo.toml` (`members`,
  `[dependencies]`), `moho_ui/Cargo.toml`, and the engine row of
  `scripts/layering.txt` (ADR-0012 puts these edits in this PR).
- Doc links: `wiki/architecture/input-and-state.md`, `overview.md` and the
  `wiki/README.md` crate list point at the new home (`/devflow:wiki`).

**Out of scope.**
- Changing any state, transition or side effect. Renames.
- Moving the binary's state handling into `moho_app` (ENG-F11).
- The ignored placeholder `tests/console_toggle.rs` (ENG-F1 hygiene).

**Test map.** A move: the proof is existing tests compiling against the new path.
| Criterion | Proof |
|---|---|
| No `moho_types` anywhere | `rg -w moho_types --glob '!_todo/**'` is empty except the wiki history note, if any |
| Coordinator tests unchanged | `moho_ui::app_state::state_coordinator::tests::*` (moved verbatim) |
| One `GameState` | the binary compiles with the conversion match deleted |
| Layering | `just check` |

**Gate class:** glue.

**Risks.**
- ENG-F11 extracts the loop from `src/`, which names `GameState` in many
  files. Land this card first: it is small, and the loop then moves with the
  final path.
- Edits shared files (root `Cargo.toml`, `scripts/layering.txt`) as ADR-0012
  directs; no other edit to them.
