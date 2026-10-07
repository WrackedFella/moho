# Architecture Decision Records

One file per decision that constrains future work: `NNNN-<slug>.md`, numbered
in order. Record the decision and its consequences, not the deliberation.
Supersede rather than edit: add a new ADR and set the old one's status to
`Superseded by NNNN`.

Format: **Status** (Proposed | Accepted | Superseded by NNNN), **Context**,
**Decision**, **Consequences**.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-render-api-boundary.md) | Renderer sees game types only through `moho_render_api` | Accepted |
| [0002](0002-voxel-and-materials-stay-in-core.md) | `voxel/` and `MaterialType` stay in `moho_core` | Superseded by 0010 |
| [0003](0003-core-owns-event-types.md) | `moho_core` owns all event types | Accepted; narrowed by 0010 |
| [0004](0004-entity-storage-without-a-general-ecs.md) | Entities live in typed, domain-owned stores; no ECS until a consumer needs one | Accepted |
| [0005](0005-crate-lines-and-dependency-direction.md) | Every crate belongs to one line; dependencies point game → engine, checked in `just check` | Accepted |
| [0006](0006-save-format-contract.md) | Saves use one versioned envelope around a `postcard`/`serde` payload | Accepted |
| [0007](0007-third-party-licence-policy.md) | Third-party licences stay compatible with closed-source commercial sale | Accepted |
| [0008](0008-keep-winit-for-windowing-and-input.md) | Keep winit; gamepads via `gilrs` when needed | Accepted |
| [0009](0009-simulation-time-is-one-fixed-tick.md) | Simulation time comes from one fixed tick; no separate clock | Accepted |
| [0010](0010-world-geometry-is-a-mesh-contract.md) | The engine sees world geometry as meshes; voxels belong to the strategy line | Accepted |
| [0011](0011-simulation-stays-network-ready.md) | The simulation advances only from per-tick commands over plain data, so replay and later netcode stay possible | Proposed |
