//! Hands voxel chunk meshes to the renderer through the world-geometry contract.

use crate::App;
use glam::IVec3;
use moho_core::materials::MaterialType;
use moho_core::voxel::VoxelChunk;
use moho_game::scene::SceneEntities;
use moho_render_api::{WorldMesh, WorldMeshError, WorldMeshId};
use moho_renderer::Scene;

const AXIS_BITS: u32 = 21;
const AXIS_OFFSET: i32 = 1 << (AXIS_BITS - 1);

/// Stable renderer key for the chunk at `pos`: 21 bits per signed axis.
pub fn chunk_mesh_id(pos: IVec3) -> WorldMeshId {
    debug_assert!(
        pos.cmpge(IVec3::splat(-AXIS_OFFSET)).all() && pos.cmplt(IVec3::splat(AXIS_OFFSET)).all(),
        "chunk position {pos} does not fit 21 bits per axis"
    );
    let axis = |v: i32| u64::from((v + AXIS_OFFSET).cast_unsigned()) & ((1 << AXIS_BITS) - 1);
    WorldMeshId((axis(pos.x) << (2 * AXIS_BITS)) | (axis(pos.y) << AXIS_BITS) | axis(pos.z))
}

/// The chunk's mesh in the world-geometry contract's form.
pub fn chunk_world_mesh(chunk: &VoxelChunk) -> Result<WorldMesh, WorldMeshError> {
    WorldMesh::new(
        chunk.vertices().to_vec(),
        chunk.normals().to_vec(),
        chunk.ambient_occlusion().to_vec(),
        chunk.block_light_rgb().to_vec(),
        chunk.sky_exposed().to_vec(),
        chunk.geometry_type().to_vec(),
        chunk.indices().to_vec(),
    )
}

/// Register the terrain material once and return its table index; idempotent.
pub fn register_terrain_material(scene: &mut Scene) -> u32 {
    let terrain = MaterialType::VoxelTerrain {
        top_albedo: glam::Vec3::new(0.3, 0.6, 0.3),  // grass green
        side_albedo: glam::Vec3::new(0.6, 0.5, 0.4), // dirt brown
    };
    scene.material_table.find_or_push(&terrain)
}

/// Queue `chunk`'s mesh with the renderer. A refused mesh is logged and the
/// chunk is not drawn.
fn upsert_chunk_mesh(scene: &mut Scene, material_idx: u32, chunk: &VoxelChunk) {
    let id = chunk_mesh_id(chunk.chunk_pos());
    match chunk_world_mesh(chunk) {
        Ok(mesh) => {
            scene.world_meshes_mut().upsert(id, mesh, material_idx);
        }
        Err(error) => {
            tracing::warn!(chunk = ?chunk.chunk_pos(), %error, "chunk mesh refused, not drawn");
            scene.world_meshes_mut().remove(id);
        }
    }
}

/// Put `chunk` in the store and hand its mesh to the renderer, replacing any
/// previous chunk at that position.
pub fn insert_chunk(app: &mut App, chunk: VoxelChunk) -> Option<VoxelChunk> {
    upsert_chunk_mesh(&mut app.scene, app.terrain_material_idx, &chunk);
    app.entities.chunks.insert(chunk)
}

/// Drop the chunk at `pos` from the store and stop drawing it.
pub fn remove_chunk(
    scene: &mut Scene,
    entities: &mut SceneEntities,
    pos: IVec3,
) -> Option<VoxelChunk> {
    scene.world_meshes_mut().remove(chunk_mesh_id(pos));
    entities.chunks.remove(pos)
}

/// Hand the mesh of every chunk in the store to the renderer, after a bulk load.
pub fn upsert_all_chunks(app: &mut App) {
    for chunk in app.entities.chunks.iter() {
        upsert_chunk_mesh(&mut app.scene, app.terrain_material_idx, chunk);
    }
}

/// Stop drawing every chunk in the store, before it is cleared.
pub fn remove_all_chunk_meshes(app: &mut App) {
    let positions: Vec<IVec3> = app
        .entities
        .chunks
        .iter()
        .map(VoxelChunk::chunk_pos)
        .collect();
    for pos in positions {
        app.scene.world_meshes_mut().remove(chunk_mesh_id(pos));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use crate::app::event_loop::event_processor::EventProcessor;
    use crate::app::event_loop::frame_processor::FrameProcessor;
    use crate::game_state::GameState;
    use moho_core::events::WorldEvent;
    use moho_renderer::RendererBackend;
    use proptest::prelude::*;
    use std::collections::HashSet;

    const LIMIT: i32 = 1 << 20;

    proptest! {
        #[test]
        fn chunk_mesh_id_is_unique_within_range(
            a in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            b in (-LIMIT..LIMIT, -LIMIT..LIMIT, -LIMIT..LIMIT),
            step in (-1..=1i32, -1..=1i32, -1..=1i32),
        ) {
            let a = IVec3::new(a.0, a.1, a.2);
            let b = IVec3::new(b.0, b.1, b.2);
            let near = (a + IVec3::new(step.0, step.1, step.2)).clamp(IVec3::splat(-LIMIT), IVec3::splat(LIMIT - 1));

            prop_assert_eq!(a == b, chunk_mesh_id(a) == chunk_mesh_id(b));
            prop_assert_eq!(a == near, chunk_mesh_id(a) == chunk_mesh_id(near));
        }
    }

    #[test]
    fn chunk_mesh_id_is_unique_across_axis_extremes() {
        let corners = [-LIMIT, -1, 0, 1, LIMIT - 1];
        let mut seen = HashSet::new();

        for x in corners {
            for y in corners {
                for z in corners {
                    assert!(
                        seen.insert(chunk_mesh_id(IVec3::new(x, y, z))),
                        "id collision at ({x}, {y}, {z})"
                    );
                }
            }
        }
    }

    /// Records register and unregister calls; handles are never reused.
    #[derive(Default)]
    struct RecordingBackend {
        next_handle: u32,
        registered: Vec<u32>,
        unregistered: Vec<u32>,
    }

    impl RendererBackend for RecordingBackend {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload indexed meshes")
        }
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.registered.push(self.next_handle);
            self.next_handle
        }
        fn unregister_mesh(&mut self, mesh: u32) {
            self.unregistered.push(mesh);
        }
        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), moho_renderer::FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[moho_renderer::InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[moho_renderer::MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: moho_renderer::LightingGpu) {}
        fn add_point_light(
            &mut self,
            _position: glam::Vec3,
            _color: glam::Vec3,
            _intensity: f32,
            _range: f32,
        ) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, _quality: u8) {}
        fn set_ssao_quality(&mut self, _quality: u8) {}
        fn set_frame_callback_raw(&mut self, _ptr: Option<*mut dyn moho_renderer::FrameCallback>) {}
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn moho_renderer::FrameCallback>>>,
        ) {
        }
    }

    #[test]
    fn evicted_chunk_is_not_drawn() {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(8.0, 80.0, 8.0), 0.0, 0.0);
        app.chunk_streamer = Some(crate::app::chunk_streamer::ChunkStreamer::new(
            moho_game::scene_builders::TerrainConfig::default(),
            moho_core::voxel::StreamingConfig {
                load_radius_chunks: 0,
                unload_radius_chunks: 1,
                chunks_per_frame: 1,
            },
            "headless-test-evicted-chunk-not-drawn",
        ));
        let far = IVec3::new(10, 4, 10);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        grid.mutator()
            .place(moho_core::voxel::BlockPos::new(165, 70, 165), 1, None);
        // Unmodified, so eviction doesn't write a chunk file to the working directory.
        grid.clear_chunk_modified(far);
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos: far,
            terrain_dirty: true,
            structure_dirty: false,
        });
        EventProcessor::new().process_world_events(&mut app);
        assert!(
            app.entities
                .chunks
                .get(far)
                .is_some_and(VoxelChunk::has_geometry),
            "the placed block meshes into a chunk with geometry"
        );
        let mut backend = RecordingBackend::default();
        app.scene.world_meshes_mut().flush(&mut backend);
        assert_eq!(backend.registered.len(), 1, "the chunk's mesh is uploaded");
        let handle = backend.registered[0];
        assert_eq!(
            app.scene
                .world_meshes_mut()
                .draws()
                .map(|(h, _)| h)
                .collect::<Vec<_>>(),
            vec![handle],
            "the chunk is drawn while loaded"
        );

        FrameProcessor::new().update_chunk_streaming(&mut app);
        app.scene.world_meshes_mut().flush(&mut backend);

        assert!(app.entities.chunks.get(far).is_none(), "chunk was evicted");
        assert_eq!(
            app.scene.world_meshes_mut().draws().count(),
            0,
            "nothing is drawn for the evicted chunk"
        );
        assert_eq!(backend.unregistered, vec![handle], "its GPU mesh was freed");
    }
}
