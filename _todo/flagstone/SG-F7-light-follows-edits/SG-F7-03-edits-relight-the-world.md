# Digging and building relight the world in game

**Feature:** SG-F7
**Status:** Draft
**Gate class:** unset
**Labels:** line:strategy

## Summary

flagstone publishes `BlockPlaced`, `BlockRemoved` and `BlocksBatchModified`, but nothing routes them into `LightSystem::handle_event`; nothing calls `recompute_sky_exposure`; and since ENG-F9-05 nothing fills block light on world load. So the light a mesh reads never changes. Wire the existing light system into the game loop.

## Deliverables

- Every block edit the player or a pawn makes reaches the light system.
- Edited and newly loaded or generated chunks get sky exposure recomputed before they are meshed.
- Loading or generating a world fills block light from its emissive blocks.
- Relit chunks are re-meshed (existing `ChunkMeshDirty` path).

## Acceptance criteria

```gherkin
Scenario: Mining a block reaches the light system
  Given a running world
  When the player mines a block
  Then the light system queues a job for that position

Scenario: Digging a shaft exposes its floor to the sky
  Given solid ground
  When a vertical shaft is dug to depth 5
  Then after the next light update the shaft floor is sky-exposed
  And the shaft's chunks are marked for re-meshing

Scenario: A loaded world has its block light filled
  Given a saved world containing an emissive block
  When it is loaded
  Then the emissive block's neighbours are lit before the first frame is drawn
```

## Tech spec
## Verification

- In game: dig a shaft from the surface; its floor should brighten once open to the sky, and darken again when covered.

## Notes

- No in-game material is emissive today, so only sky light is visible until a game card adds one (feature Deferred).
