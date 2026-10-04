use crate::actors::ActorStore;
use moho_core::voxel::ChunkStore;

/// Everything in a scene that is rendered, persisted or simulated per entity:
/// sphere/cube actors and terrain chunks.
#[derive(Debug, Default)]
pub struct SceneEntities {
    pub actors: ActorStore,
    pub chunks: ChunkStore,
}
