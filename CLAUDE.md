# CLAUDE.md

Moho: a Rust workspace with a shared engine and two game lines (strategy
base-builder, FPS). Engineering standards come from the `devflow` plugin
(`devflow:rust-standards`); this file holds only what is specific to moho.

## Gate and commands

`just check` (fmt, clippy `-D warnings`, nextest, doctests, comment refs) must pass
before any commit. Never silence a lint, skip a test, or add to the lint allow-list
to get green; the Ratchet group in the root `Cargo.toml` only shrinks.

```bash
just check                         # the gate
just test -E 'package(moho_core)'  # one crate; -E 'test(name)' for one test
just mutants                       # mutation-test changes since origin/dev; exit 2 = survivor
just deny                          # licenses, advisories, sources
gh workflow run CI --ref <branch>  # all three OSes on a branch (PRs into dev run Linux only)
cargo run                          # RUST_LOG=debug for logging
```

## Workflow contract (used by devflow skills and agents)

- **Base branch:** `dev`. Branch `<type>/<ID>-<slug>`, PR into `dev`. `main` only
  receives promotions from `dev`. Never merge; a human does.
- **Feature integration branches:** a feature whose cards only make sense together
  gets `feature/<ID>-<slug>` off `dev`; it is the base branch for that feature's
  cards (branch from it, PR into it, `MOHO_BASE=origin/feature/<ID>-<slug>` for
  `just check`/`just mutants`). Card PRs don't auto-close issues there; the
  feature's PR into `dev` lists `Closes #…` for each card. The feature's card table
  in `_todo/` names its integration branch. Current: none.
- **Planning:** index `_todo/README.md`, rules `_todo/_STANDARDS.md` (features first,
  IDs like `SG-F1-04`, card lifecycle). Check `_todo/ROADMAP.md` for order before
  picking up work.
- **Implement a ready item:** `/devflow:orchestrate <issue>`.
- **Domain-logic paths** (failing tests need human review before implementation):
  rules in `moho_game`, `moho_core`. Adapters, UI wiring and config are glue.
- **ADRs:** `_todo/adr/`. Project documentation lives in `wiki/`; new docs there or
  elsewhere need the user's OK first; cards and ADRs that follow the standards don't.
  Order of work: `_todo/ROADMAP.md`.

## Architecture

```
src/            binary: winit event loop, wiring
moho_core       event bus, voxel grid/meshing/lighting, materials, input, prefs
moho_game       game domain: pawn, tools, controller, GameClock, scenes, raycast
moho_render_api engine/game contract: Renderable, RenderMaterial, GPU-layout data
moho_renderer   wgpu backend: meshes, CSM shadows, SSAO, skybox
moho_ui         egui: menus, settings, console, HUD
moho_audio      rodio playback, event-driven
moho_physics    rapier3d character controller and colliders
moho_input      key → binding-code mapping
moho_types      AppState, StateCoordinator
```

- Engine crates never depend on `moho_game` (ADR-0001). World geometry reaches
  the engine as meshes; voxels belong to the strategy line (ADR-0010). Until
  ENG-F10 moves it to `moho_voxel`, `voxel/` still sits in `moho_core`.
- Engine-wide event types live in `moho_core::events`; line-specific ones live
  with their line's crate (ADR-0003, narrowed by ADR-0010).
- Use the event bus for cross-system fan-out; call directly within a system.
- Entities live in typed stores (`ActorStore` in `moho_game`, `ChunkStore` in `moho_core`); no general ECS (ADR-0004).
- Shared dependency versions go in `[workspace.dependencies]`.

## GitNexus (code graph, index name `moho`)

- **Hand edits that change a symbol's behavior or signature:** run
  `impact({target, direction: "upstream"})` first; warn the user on HIGH/CRITICAL.
- **Mechanical edits** (`clippy --fix`, rustfmt, GitNexus `rename`, comment- or
  attribute-only changes): skip per-symbol `impact`; review `detect_changes` instead.
- **Before every commit:** `detect_changes()`. A high rating driven only by breadth is
  reported with that reason; escalate only if signatures, public APIs or behavior
  changed.
- **"No callers resolved" is unknown, not safe:** Rust method calls are often missed;
  confirm with a text search.
- Renames use GitNexus `rename`, never find-and-replace.
- Refresh a stale index (>10 commits behind) with
  `node .gitnexus/run.cjs analyze --skip-agents-md` (the flag keeps it from rewriting
  this file). Usage guides: `.claude/skills/gitnexus/`.
