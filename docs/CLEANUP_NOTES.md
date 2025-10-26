# Cleanup Notes (2025-10-25)

This file summarizes the low-risk cleanups and findings from a quick repository sweep.

## Findings (low-risk)

- `moho_core/src/scene_builders.rs` contains a TODO: `Implement proper greedy meshing before re-enabling smoothing` — a mesh optimization backlog item.

These TODOs were left in place because they are valid development tasks; consider opening issues for them if you want them tracked.

## Recommended follow-ups (not performed)

- Extract core game simulation from `src/main.rs` into a separate crate or module for better testability and future multiplayer work.
- Convert continuous input application to discrete `PlayerInput` commands for easier networking/replay.
- Add short input-grace frames (1–3 frames) after scene loads if camera jitter persists despite input clearing.
- Add form validation and unit tests for the `NewWorldMenu`.
- Consider consolidating conceptual documentation into `docs/` while keeping crate `README.md` files focused on usage.

If you want, I can create GitHub issues for the recommended refactors or implement one of the small follow-ups (form validation or input grace-frames).

