# Flagstone roadmap

The strategy line's features in order. Engine items are numbered as in the [engine roadmap](../engine/roadmap.md); item status lives on the [project board](https://github.com/users/WrackedFella/projects/1).

| # | Feature | Engine items it needs | Notes |
|---|---|---|---|
| S.1 | [SG-F1](https://github.com/WrackedFella/moho/issues/311) possessing a pawn feels right | 2.1 for feel | — |
| S.2 | [SG-F2](https://github.com/WrackedFella/moho/issues/308) a saved world comes back as I left it: [SG-F2-01](https://github.com/WrackedFella/moho/issues/341), [SG-F2-03](https://github.com/WrackedFella/moho/issues/343), [SG-F2-02](https://github.com/WrackedFella/moho/issues/342) | none | ENG-F10-03 is done, so these no longer race the voxel move; its load-path card covers the #264 gap (scene load skips releasing held keys) |
| S.3 | [SG-F5](https://github.com/WrackedFella/moho/issues/309) run the colony from above, step into a pawn | 2.6, 2.7 (both done) | block and pawn picking are strategy-line; its pan, zoom and possess cards can run alongside S.2 |
| S.4 | [SG-F4](https://github.com/WrackedFella/moho/issues/310) pawns dig what I mark | 2.8 (2.5.1, 2.7 done; 2.9, 2.10 delivered, adopt after the skeleton) | first new persisted type; #303 voxel routing is its card. Before SG-F3 |
| S.4.1 | [SG-F7](https://github.com/WrackedFella/moho/issues/470) light follows what I dig and build: [SG-F7-01](https://github.com/WrackedFella/moho/issues/471) → [SG-F7-02](https://github.com/WrackedFella/moho/issues/472); [SG-F7-03](https://github.com/WrackedFella/moho/issues/473) after #467; [SG-F7-04](https://github.com/WrackedFella/moho/issues/474) (first emissive block) after #473 and [ENG-F24-05](https://github.com/WrackedFella/moho/issues/469); SG-F7-05 with ENG-F29-01; [SG-F7-06](https://github.com/WrackedFella/moho/issues/482) (smooth AO) after SG-F7-05 | ENG-F24-05 (#469) for SG-F7-04; 2.17 ENG-F29-01 for SG-F7-05 | M2 (Justin, 2026-10-21); dug shafts and caves need correct sky light for S.4 |
| S.5 | [SG-F3](https://github.com/WrackedFella/moho/issues/312) buildings and construction (not a gate item) | 2.7 (placement preview) | after SG-F4 |
| S.6 | SG-F6 new worlds look intentional ([draft](../../_todo/flagstone/SG-F6-world-generation/_feature.md), not filed; not a gate item) | none | waits for the user to rank the world-generation problems |


## Decisions

| Date | Decision | Record |
|---|---|---|
| 2026-10-04 | [SG-F2-03](https://github.com/WrackedFella/moho/issues/343) is reproduce-first and no longer waits on the save-format migration | [SG-F2-03](https://github.com/WrackedFella/moho/issues/343) |
| 2026-10-14 | Strategy mode is the primary game; first person is possession of one pawn. Strategy order is SG-F2, SG-F5, SG-F4, SG-F3 (pawns before buildings); SG-F1 any time, SG-F6 once ranked; gameplay UI and sound ride inside each feature | [SG-F5](https://github.com/WrackedFella/moho/issues/309), [SG-F4](https://github.com/WrackedFella/moho/issues/310), [SG-F3](https://github.com/WrackedFella/moho/issues/312) |
| 2026-10-19 | Strategy plan in four milestones named for what the player can do: trust the world (SG-F2, SG-F1-04), run the colony from above (SG-F5), pawns dig what I mark (SG-F4), build and polish (SG-F3, SG-F6); the first two may overlap, and the SG-F4 BA pass starts during SG-F5 | [flagstone-milestones](https://github.com/WrackedFella/moho/issues/308) |
| 2026-10-21 | New SG-F7 (light follows edits) in M2; the first emissive block (glowing crystal, solid-colour emission) lands with it | [SG-F7](https://github.com/WrackedFella/moho/issues/470) |
