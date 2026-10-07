# ENG-F20 — A wgpu or egui upgrade touches one crate

## End state

The engine tracks fast-moving upstream stacks (wgpu releases a major version
several times a year) at a predictable, small cost per upgrade, under
[ENG-F2](../ENG-F2-dependency-upgrades/_feature.md)'s upgrade policy. Both
games and the split engine repo ([ENG-F5](../ENG-F5-physical-repo-split/_feature.md))
inherit that cost, so it should stay bounded.

## Summary

Upgrading wgpu has meant semi-major refactoring, so upgrades were deferred
until drift made them worse. Two things spread the cost: wgpu types are named
outside the renderer (the UI adapter and the binary's renderer setup), and
`egui-wgpu` ties every egui release to one wgpu release, so neither can move
alone. When this ships, a wgpu upgrade changes the renderer crate only, and
egui and wgpu can be upgraded on their own schedules.
**Moves toward the end state by:** making ENG-F2's per-milestone upgrades
routine, and giving the split engine a stable surface for its consumers.

## Exit criteria

- wgpu types are named only in the renderer crate. Game crates, the UI crate
  and the binaries reach the GPU through the renderer's own interface.
- `just check` fails when a crate outside the renderer names wgpu (same kind of
  rule as the layering check).
- An egui upgrade doesn't force a wgpu upgrade, and the reverse (per the
  decision below).
- Proof: the next wgpu major upgrade after this feature (an ENG-F2 card)
  changes source files only inside the renderer crate. Its diff is compared to
  [ENG-F2-08](../ENG-F2-dependency-upgrades/ENG-F2-08-graphics-stack-current.md)'s
  baseline.
- UI and rendering look and behave as before (manual check).

## Scope

- In: wgpu containment; the egui-to-GPU integration; the enforcement check.
- Out: egui widget code (menus, HUD, console are written against egui and
  churn with it; containing that would mean wrapping egui, which isn't worth
  it); winit containment (ENG-F11 owns the window and loop); rapier (already
  contained in `moho_physics`).

## Direction-setting decisions

| Question | Decision | Why / cost of the alternative |
|---|---|---|
| How are egui and wgpu versions decoupled? | **Open, for the Tech Lead.** Candidate: the renderer draws egui's output itself (egui emits plain triangles and textures), replacing `egui-wgpu` | Keeping `egui-wgpu` keeps the lockstep and gives up the third exit criterion; an in-house painter is a few hundred lines to own and must track egui's paint-output format |
| Is egui wrapped behind an engine UI API? | No | Wrapping a UI library costs more than its churn |

## Deferred

| Idea | Why it waits | Revisit when |
|---|---|---|
| winit containment | ENG-F11 moves the window and loop into an engine crate first | ENG-F11 done |
| Support for a second graphics backend | No consumer; wgpu already covers Vulkan, Metal, DX12, GL | A platform wgpu can't reach |

## Items

| Item |
|---|

## Notes

- Sequence after [ENG-F11](../ENG-F11-shared-app-loop/_feature.md) and with or
  after [ENG-F18](../ENG-F18-shared-ui-shell/_feature.md): both move the code
  that names wgpu today (the binary's renderer setup, the UI adapter). They
  should not add new wgpu use outside the renderer.
- wgpu alternatives were considered in the [ENG-F13 audit](../ENG-F13-dependency-audit/audit.md#alternatives-considered-for-the-largest-stacks);
  none matches its maturity and reach, so the answer to churn is containment.
