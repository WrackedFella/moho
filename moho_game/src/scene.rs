use crate::actors::ActorStore;
use moho_voxel::ChunkStore;

/// Everything in a scene that is rendered, persisted or simulated per entity:
/// sphere/cube actors and terrain chunks.
#[derive(Debug, Default)]
pub struct SceneEntities {
    pub actors: ActorStore,
    pub chunks: ChunkStore,
}

impl SceneEntities {
    /// Removes every actor and chunk, invalidating all outstanding `ActorId`s.
    pub fn clear(&mut self) {
        self.actors.clear();
        self.chunks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::Sphere;
    use moho_voxel::MaterialType;
    use moho_voxel::VoxelChunk;

    #[test]
    fn clear_empties_actors_and_chunks() {
        let mut entities = SceneEntities::default();
        let material = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        entities
            .actors
            .spawn_sphere(Sphere::new(glam::Vec3::ZERO, 1.0, material));
        entities.chunks.insert(VoxelChunk::new(
            glam::IVec3::ZERO,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            0,
        ));

        entities.clear();

        assert!(entities.actors.is_empty());
        assert!(entities.chunks.is_empty());
    }
}
