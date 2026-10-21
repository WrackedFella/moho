# Architecture Decision Records

One file per decision that constrains future work: `NNNN-<slug>.md`, numbered
in order. Record the decision and its consequences, not the deliberation.
Supersede rather than edit: add a new ADR and set the old one's status to
`Superseded by NNNN`.

Format: **Status** (Proposed | Accepted | Superseded by NNNN), **Context**,
**Decision**, **Consequences**.

ADRs written before ENG-F27 (2026-10-15) keep the crate names of their day. Read
`moho_game` as `flagstone_game`, `moho_ui` as `flagstone_ui`, `moho_voxel` as
`flagstone_voxel` and the "strategy line" as flagstone.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-render-api-boundary.md) | Renderer sees game types only through `moho_render_api` | Accepted |
| [0002](0002-voxel-and-materials-stay-in-core.md) | `voxel/` and `MaterialType` stay in `moho_core` | Superseded by 0010 |
| [0003](0003-core-owns-event-types.md) | `moho_core` owns all event types | Accepted; narrowed by 0010 |
| [0004](0004-entity-storage-without-a-general-ecs.md) | Entities live in typed, domain-owned stores; no ECS until a consumer needs one | Accepted |
| [0005](0005-crate-lines-and-dependency-direction.md) | Every crate belongs to one line; dependencies point game → engine, checked in `just check` | Accepted |
| [0006](0006-save-format-contract.md) | Saves use one versioned envelope around a `postcard`/`serde` payload | Accepted; extended by 0016 |
| [0007](0007-third-party-licence-policy.md) | Third-party licences stay compatible with closed-source commercial sale | Accepted |
| [0008](0008-keep-winit-for-windowing-and-input.md) | Keep winit; gamepads via `gilrs` when needed | Accepted |
| [0009](0009-simulation-time-is-one-fixed-tick.md) | Simulation time comes from one fixed tick; no separate clock | Accepted |
| [0010](0010-world-geometry-is-a-mesh-contract.md) | The engine sees world geometry as meshes; voxels belong to the strategy line | Accepted |
| [0011](0011-simulation-stays-network-ready.md) | The simulation advances only from per-tick commands over plain data, so replay and later netcode stay possible | Accepted; narrowed by 0018 |
| [0012](0012-engine-crate-map-for-m2.md) | Engine capabilities land as modules first; crate map and seam list for M1–M2 | Accepted |
| [0013](0013-ui-reaches-the-renderer-as-paint-data.md) | UI reaches the renderer as plain paint data; the renderer owns UI drawing | Accepted |
| [0014](0014-characters-move-by-velocity.md) | Characters move by velocity; a movement step owns the rules and reports impacts, physics only resolves contact | Proposed |
| [0015](0015-fps-rules-are-a-platform-free-crate.md) | FPS rules are a platform-free crate; every shot resolves through one path | Accepted |
| [0016](0016-game-lines-own-their-save-kinds.md) | Game lines own their save kinds and versions; the envelope stays content-blind | Accepted |
| [0017](0017-ui-shell-and-debug-values-are-engine-seams.md) | The UI shell and debug values are engine seams | Proposed |
| [0018](0018-paused-commands-apply-at-the-tick-boundary.md) | While paused, a command applies at the next tick boundary without a tick; pause and speed are loop state | Proposed |
| [0021](0021-game-ai-lives-in-the-lines.md) | Game AI lives in the lines; the engine answers queries, and AI acts through player commands, sim-only perception and seeded randomness | Proposed |
