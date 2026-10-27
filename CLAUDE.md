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
just mutants                       # mutation-test domain-path changes since origin/dev; exit 2 = survivor
just deny                          # licenses, advisories, sources
bash scripts/cloud-tools.sh        # cloud threads only, before the first build
gh workflow run CI --ref <branch>  # all three OSes on a branch (PRs into dev run Linux only)
cargo run -p flagstone             # RUST_LOG=debug for logging
cargo run -p searchlight           # the FPS prototype
```

## Workflow contract (used by devflow skills and agents)

- **Base branch:** `dev`. Branch `<type>/<ID>-<slug>`, PR into `dev`. `main` only
  receives promotions from `dev`. Never merge; a human does.
- **Feature integration branches:** until v1, card PRs go straight into `dev`. A
  feature gets `feature/<ID>-<slug>` off `dev` only when its cards can't land on
  their own (one would leave `dev` broken or half-moved without the others). That
  branch is the base for the feature's cards (branch from it, PR into it,
  `MOHO_BASE=origin/feature/<ID>-<slug>` for `just check`/`just mutants`), takes `dev`
  by merge commit, never squash (squash drops the merge base and re-creates resolved
  conflicts), and its PR into `dev` lists `Closes #…` for each card, since card PRs
  don't auto-close issues there. The feature's issue names its integration branch,
  which is deleted once that PR merges. Current: none.
- **Project board:** WrackedFella, project 1 (https://github.com/users/WrackedFella/projects/1).
  **Board header:** yes (the Board sync `Fields` job applies it).
  The board and issue are the only record of item state; nothing in `_todo/` mirrors
  them. Agents change the board only through devflow's `scripts/board`.
- **The issue is the record of an item.** When scope changes, edit the issue body;
  deviations and decisions go in an issue comment. Implementation PRs edit no `_todo/`
  item file (`_todo/_STANDARDS.md`, Issues and branches).
- **Planning:** index `_todo/README.md`, rules `_todo/_STANDARDS.md` (features first,
  IDs like `SG-F1-04`, card lifecycle). Check `wiki/roadmap.md` (cross-line) and your line's roadmap (`wiki/{engine,flagstone,searchlight}/roadmap.md`) for order before
  picking up work.
- **Environment setup command:** `bash scripts/cloud-tools.sh` (cloud threads only;
  text-only threads skip it).
- **Implement a ready item:** `/devflow:orchestrate <issue>`.
- **Domain-logic paths** (gate class `domain`; also the scope of `just mutants` and CI's
  `Mutants (diff)`, which read the globs below through `scripts/domain-paths.sh`):
  - `moho_core/**`
  - `flagstone_voxel/**`
  - `flagstone_game/**`
  - `moho_level/**`
  - `searchlight_rules/**`
  - `moho_physics/src/movement.rs`
  Adapters, UI wiring and config are glue: the weekly full run mutates them, PRs don't.
- **Human review points:** `Card review: required`, `Domain-test review: agent` (values: `required`,
  `agent`, `not required`; `agent` = no pause, `devflow:test-critic` attacks the failing
  tests before implementation and the PR carries its findings). Switches and runtimes: `_todo/WORKFLOW.md`.
- **Local planning files are drafts.** A card or feature in `_todo/` exists only until
  it is approved; the approving thread files the issue and deletes the file
  (`_todo/_STANDARDS.md`, Drafts).
- **ADRs:** `_todo/adr/`. Order of work: `wiki/roadmap.md`.
- **Docs directory:** `wiki/`, sections and conventions in `wiki/README.md`. The
  orchestrator keeps it current through `/devflow:wiki`; new pages are flagged in the
  PR, and PR review is their approval. The wiki is the documentation source of
  truth; docs outside `wiki/` (other than card drafts, ADRs and crate READMEs) need the
  user's OK first.

## Architecture

```
flagstone         binary: winit event loop, wiring
moho_core         event bus, engine-wide events, prefs, persistence
flagstone_voxel   flagstone line: voxel grid, meshing, LOD, lighting, chunk store, terrain materials, world events
flagstone_game    game domain: pawn, tools, controller, scenes, raycast
moho_render_api   engine/game contract: Renderable, RenderMaterial, GPU-layout data
moho_renderer     wgpu backend: meshes, CSM shadows, SSAO, skybox
flagstone_ui      egui: menus, settings, console, HUD
moho_audio        rodio playback, event-driven
moho_physics      rapier3d character controller and colliders
moho_input        key → binding-code mapping
moho_app          fixed-tick loop shared by the games: Game trait, headless loop, GameClock
moho_ui_shell     egui host shared by the games: console, screen text, menu screens, engine settings rows
moho_level        glTF level loader and navmesh (headless, no wgpu or winit)
searchlight_rules FPS line: rifle, magazine and body-part damage rules, platform-free (ADR-0015)
searchlight       FPS line: arena, shot resolution, sim and binary on the engine and searchlight_rules
```

- Engine crates never depend on `flagstone_game` (ADR-0001). World geometry reaches
  the engine as meshes; voxels belong to the flagstone line
  (`flagstone_voxel`, ADR-0010).
- Engine-wide event types live in `moho_core::events`; line-specific ones live
  with their line's crate (ADR-0003, narrowed by ADR-0010).
- Use the event bus for cross-system fan-out; call directly within a system.
- Entities live in typed stores (`ActorStore` in `flagstone_game`, `ChunkStore` in `flagstone_voxel`); no general ECS (ADR-0004).
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
