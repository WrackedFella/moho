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
| [0002](0002-voxel-and-materials-stay-in-core.md) | `voxel/` and `MaterialType` stay in `moho_core` | Accepted |
| [0003](0003-core-owns-event-types.md) | `moho_core` owns all event types | Accepted |
