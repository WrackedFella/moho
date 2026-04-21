# TD-07: Pick One Serialization Strategy

**Priority:** Medium — two systems create inconsistency and inflate compile times.

## Problem

Two serialization approaches are used in parallel with no clear rule for which to use:

| Serializer | Where used |
|---|---|
| `serde_json` | `moho_sim::Simulation::snapshot` / `restore` (the toy headless sim) |
| `bincode` (2.0, `Encode`/`Decode`) | `moho_sim::SimulationController::snapshot_bytes` / `restore_from_bytes`; scene envelope in `src/save.rs`; `WorldSpec` persistence |

Additionally, scene entity data goes through `legion`'s serialization using `serde`,
while the outer envelope uses `bincode`. `LightDesc` derives both `Encode + Decode`
and `Serialize + Deserialize` — deriving four traits for one type suggests uncertainty
about which path is authoritative.

## Acceptance Criteria

1. One serializer is chosen for all save/snapshot paths. Recommendation: **bincode**
   for all binary on-disk formats (compact, fast, already used in save envelope and
   `SimulationController`) and **serde_json** only for human-readable config (e.g.
   `config/prefs.ini` stays as-is, debug dumps stay as JSON).

2. `moho_sim::Simulation::snapshot/restore` are changed to bincode or removed
   entirely (the toy `Simulation` may not need persistence — it exists only for
   headless determinism tests).

3. `LightDesc` (and similar structs) derive only what is actually used. If bincode is
   the save format, drop `Serialize + Deserialize`. If the ECS serialization path
   needs serde, document it explicitly.

4. `serde_json` is removed from `Cargo.toml` [workspace.dependencies] if no longer
   needed, or its uses are consolidated to the config/debug path.

### E. `LightDesc` derives four traits redundantly

`moho_renderer::scene::LightDesc` derives both `Encode + Decode` (bincode) and
`Serialize + Deserialize` (serde). This is a direct symptom of the split strategy —
both paths were added at different times and neither was cleaned up. Whoever resolves
the serialization choice should also remove the unused derive pair from `LightDesc`
and any other types in the same situation.

## Files

- `moho_sim/src/lib.rs` — `Simulation::snapshot/restore`
- `moho_sim/src/simulation.rs` — `SimulationController` snapshot
- `moho_renderer/src/scene.rs` — `LightDesc` derives
- `src/save.rs` — envelope format
- `Cargo.toml` — workspace deps
